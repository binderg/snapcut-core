//! Client side of the local socket: what the CLI (and tests) use to talk to a
//! project's daemon, starting one if none is running.

use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use interprocess::local_socket::tokio::Stream;
use interprocess::local_socket::tokio::prelude::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, ReadHalf, WriteHalf};

use crate::ExeIdentity;
use crate::project::{self, Located};
use crate::server::{socket_address, socket_name};

/// How long to wait for a freshly spawned daemon to answer.
const START_TIMEOUT: Duration = Duration::from_secs(10);
/// After a spawned daemon exits (another one won the lock, say), how long to keep trying to connect.
const LOSER_GRACE: Duration = Duration::from_secs(2);

/// A connection to a daemon over its local socket.
pub struct Client {
    lines: Lines<BufReader<ReadHalf<Stream>>>,
    write: WriteHalf<Stream>,
    next_id: u64,
    /// Notifications that arrived while waiting for a response.
    pending: VecDeque<Value>,
}

impl Client {
    /// Connect to the project's running daemon, if there is one.
    pub async fn connect(loc: &Located) -> io::Result<Self> {
        let address = socket_address(loc);
        let stream = Stream::connect(socket_name(&address)?).await?;
        let (read, write) = tokio::io::split(stream);
        Ok(Self { lines: BufReader::new(read).lines(), write, next_id: 1, pending: VecDeque::new() })
    }

    /// Send a request and wait for its response (the whole JSON-RPC response object).
    pub async fn request(&mut self, method: &str, params: Value) -> io::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let mut line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        self.write.write_all(line.as_bytes()).await?;
        loop {
            let msg = self.read().await?.ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "the daemon closed the connection"))?;
            if msg.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(msg);
            }
            if msg.get("id").is_none() {
                self.pending.push_back(msg);
            }
        }
    }

    /// Call and split the response into result or error object.
    pub async fn call(&mut self, method: &str, params: Value) -> io::Result<Result<Value, Value>> {
        let resp = self.request(method, params).await?;
        Ok(match resp.get("error") {
            Some(e) => Err(e.clone()),
            None => Ok(resp["result"].clone()),
        })
    }

    /// The next notification (after `events.subscribe`), or `None` when the daemon hung up.
    pub async fn notification(&mut self) -> io::Result<Option<Value>> {
        if let Some(n) = self.pending.pop_front() {
            return Ok(Some(n));
        }
        loop {
            match self.read().await? {
                Some(msg) if msg.get("id").is_none() => return Ok(Some(msg)),
                Some(_) => {}
                None => return Ok(None),
            }
        }
    }

    async fn read(&mut self) -> io::Result<Option<Value>> {
        loop {
            let Some(line) = self.lines.next_line().await? else { return Ok(None) };
            if line.trim().is_empty() {
                continue;
            }
            return serde_json::from_str(&line).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
        }
    }
}

/// Connect to the project's daemon, starting `ve_exe daemon run` if none is
/// running. A daemon running a different `ve` binary (an older build) is
/// stopped and replaced, so a rebuilt CLI never talks to stale code.
pub async fn connect_or_spawn(project: &Path, ve_exe: &Path) -> io::Result<Client> {
    let loc = project::locate(project).map_err(io::Error::other)?;
    if let Ok(mut client) = Client::connect(&loc).await {
        let info = client.call("daemon.info", json!({})).await?;
        let theirs = info.ok().and_then(|i| serde_json::from_value::<ExeIdentity>(i["exe"].clone()).ok());
        if theirs.as_ref() == Some(&ExeIdentity::of(ve_exe)) {
            return Ok(client);
        }
        tracing::info!("restarting the ve daemon for {}: its binary differs from this one", loc.root.display());
        let _ = client.call("daemon.stop", json!({})).await;
        drop(client);
        wait_until_gone(&loc).await;
    }
    spawn(&loc, ve_exe).await
}

async fn wait_until_gone(loc: &Located) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && Client::connect(loc).await.is_ok() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn spawn(loc: &Located, ve_exe: &Path) -> io::Result<Client> {
    std::fs::create_dir_all(&loc.cache)?;
    let log_path = loc.cache.join("daemon.log");
    let log = std::fs::File::create(&log_path)?;
    let mut cmd = Command::new(ve_exe);
    cmd.arg("--project")
        .arg(&loc.root)
        .args(["daemon", "run"])
        // Not the project folder: a daemon's working directory would block deleting it on Windows.
        .current_dir(&loc.cache)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    detach(&mut cmd);
    let mut child = cmd.spawn()?;

    let start = Instant::now();
    let mut exited_at = None;
    loop {
        if let Ok(client) = Client::connect(loc).await {
            return Ok(client);
        }
        if exited_at.is_none() && child.try_wait()?.is_some() {
            exited_at = Some(Instant::now());
        }
        let gave_up = match exited_at {
            // Another daemon may have won the race for the lock; give it a moment.
            Some(t) => t.elapsed() > LOSER_GRACE,
            None => start.elapsed() > START_TIMEOUT,
        };
        if gave_up {
            let log = std::fs::read_to_string(&log_path).unwrap_or_default();
            let last = log.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output");
            return Err(io::Error::other(format!("the ve daemon did not start ({last}); log: {}", log_path.display())));
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

/// Let the daemon outlive the CLI and keep it off the CLI's console.
fn detach(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
}
