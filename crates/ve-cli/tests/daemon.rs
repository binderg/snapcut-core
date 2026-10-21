//! The daemon end to end: the CLI starts it, edits go through it, and
//! subscribers on both transports see `project.changed`.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use serde_json::{Value, json};
use ve_daemon::client::Client;
use ve_daemon::project::locate;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ve-cli-tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn ve(cwd: &Path, args: &[&str]) -> Output {
    // Daemons started by tests go away on their own if a test fails before `daemon stop`.
    Command::new(env!("CARGO_BIN_EXE_ve")).current_dir(cwd).env("VE_DAEMON_IDLE_SECS", "5").args(args).output().unwrap()
}

fn ok(cwd: &Path, args: &[&str]) -> String {
    let o = ve(cwd, args);
    assert!(o.status.success(), "ve {args:?}: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap()
}

/// Just enough of a WebSocket client (RFC 6455) for tests: text frames, no fragmentation.
struct Ws(TcpStream, std::collections::VecDeque<Value>);

impl Ws {
    /// Connect and return the HTTP status of the upgrade (101 on success).
    fn connect(url: &str, origin: Option<&str>) -> (u16, Self) {
        let rest = url.strip_prefix("ws://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut s = TcpStream::connect(host).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let origin = origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default();
        write!(
            s,
            "GET /{path} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n{origin}\r\n"
        )
        .unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut byte).unwrap();
            head.push(byte[0]);
        }
        let head = String::from_utf8(head).unwrap();
        let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, Self(s, Default::default()))
    }

    fn send(&mut self, v: &Value) {
        let payload = v.to_string().into_bytes();
        let mut frame = vec![0x81u8];
        // Client frames must be masked; an all-zero key leaves the payload as is.
        match payload.len() {
            n if n < 126 => frame.push(0x80 | n as u8),
            n => {
                frame.push(0x80 | 126);
                frame.extend_from_slice(&(n as u16).to_be_bytes());
            }
        }
        frame.extend_from_slice(&[0, 0, 0, 0]);
        frame.extend_from_slice(&payload);
        self.0.write_all(&frame).unwrap();
    }

    fn recv(&mut self) -> Value {
        loop {
            let mut h = [0u8; 2];
            self.0.read_exact(&mut h).unwrap();
            let len = match h[1] & 0x7f {
                126 => {
                    let mut b = [0u8; 2];
                    self.0.read_exact(&mut b).unwrap();
                    u16::from_be_bytes(b) as usize
                }
                127 => {
                    let mut b = [0u8; 8];
                    self.0.read_exact(&mut b).unwrap();
                    u64::from_be_bytes(b) as usize
                }
                n => n as usize,
            };
            let mut payload = vec![0u8; len];
            self.0.read_exact(&mut payload).unwrap();
            if h[0] & 0x0f == 1 {
                return serde_json::from_slice(&payload).unwrap();
            }
        }
    }

    /// The response to request `id`. Notifications may arrive first (an
    /// edit's own `project.changed` can beat its response); they are kept.
    fn response(&mut self, id: u64) -> Value {
        loop {
            let m = self.recv();
            if m["id"] == id {
                return m;
            }
            self.1.push_back(m);
        }
    }

    /// Next message that is not a response.
    fn notification(&mut self) -> Value {
        if let Some(m) = self.1.pop_front() {
            return m;
        }
        loop {
            let m = self.recv();
            if m.get("id").is_none() {
                return m;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn daemon_serves_edits_and_events() {
    let dir = scratch("daemon");
    ok(&dir, &["init", "edit", "--fps", "30", "--size", "160x120"]);
    let proj = dir.join("edit");

    // `start` spawns it; `status` finds it.
    let started = ok(&proj, &["daemon", "start"]);
    assert!(started.contains("websocket  ws://127.0.0.1:"), "{started}");
    let status: Value = serde_json::from_str(&ok(&proj, &["--json", "daemon", "status"])).unwrap();
    assert_eq!(status["running"], true);
    let ws_url = status["websocket"].as_str().unwrap().to_owned();

    // The WebSocket wants the token, and no foreign origin.
    let wrong = ws_url.split("?token=").next().unwrap().to_owned() + "?token=nope";
    assert_eq!(Ws::connect(&wrong, None).0, 401);
    assert_eq!(Ws::connect(&ws_url, Some("https://example.com")).0, 403);
    let (code, mut ws) = Ws::connect(&ws_url, Some("http://localhost:5173"));
    assert_eq!(code, 101);

    // Subscribe on both transports.
    ws.send(&json!({"jsonrpc": "2.0", "id": 1, "method": "events.subscribe"}));
    assert_eq!(ws.response(1)["result"]["revision"], 0);
    let mut pipe = Client::connect(&locate(&proj).unwrap()).await.unwrap();
    assert_eq!(pipe.call("events.subscribe", json!({})).await.unwrap().unwrap()["revision"], 0);

    // An edit from the CLI reaches both.
    let added = ok(&proj, &["--actor", "agent:test", "marker", "add", "--at", "10f", "--name", "chapter"]);
    assert!(added.contains("rev 1"), "{added}");
    for event in [pipe.notification().await.unwrap().unwrap(), ws.notification()] {
        assert_eq!(event["method"], "project.changed", "{event}");
        let p = &event["params"];
        assert_eq!(p["revision"], 1);
        assert_eq!(p["command"], "marker.add");
        assert_eq!(p["actor"], json!({"kind": "agent", "id": "test"}));
        assert!(p["patch"].as_array().unwrap().iter().any(|op| op["path"] == "/metadata/ve/project/revision"), "{p}");
    }

    // Edits over the WebSocket work too, with the same optimistic concurrency.
    ws.send(&json!({"jsonrpc": "2.0", "id": 2, "method": "history.undo", "params": {"base_rev": 0}}));
    assert_eq!(ws.response(2)["error"]["code"], -31000);
    ws.send(&json!({"jsonrpc": "2.0", "id": 3, "method": "history.undo", "params": {"base_rev": 1}}));
    let undo = ws.response(3);
    assert_eq!(undo["result"]["revision"], 2, "{undo}");
    for event in [pipe.notification().await.unwrap().unwrap(), ws.notification()] {
        assert_eq!(event["params"]["command"], "history.undo", "{event}");
    }

    // An edit made outside the engine is picked up, not overwritten.
    let file = proj.join("project.otio");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, text.replacen("\"name\": \"edit\"", "\"name\": \"renamed\"", 1)).unwrap();
    let summary = ok(&proj, &["summary"]);
    assert!(summary.contains("\"renamed\""), "{summary}");
    assert_eq!(ws.notification()["method"], "project.reloaded");

    // `stop` ends it.
    drop(pipe);
    drop(ws);
    assert!(ok(&proj, &["daemon", "stop"]).contains("stopped"));
    std::thread::sleep(Duration::from_millis(500));
    assert!(ok(&proj, &["daemon", "status"]).contains("no ve daemon"));
}
