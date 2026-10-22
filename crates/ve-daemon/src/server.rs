//! The daemon: one process per project, serving the shared registry over a
//! local socket (a named pipe on Windows) and a loopback WebSocket.
//!
//! - Local socket: newline-delimited JSON-RPC, one message per line. This is
//!   what the CLI uses. Its name comes from the project's key, so clients find
//!   it without a lookup file (`ve-<key>` pipe on Windows, `daemon.sock` in the
//!   project's cache folder elsewhere, which only the user can reach).
//! - WebSocket: `ws://127.0.0.1:<port>/rpc?token=<token>`, one JSON-RPC message
//!   per text frame. The token is random per daemon and only readable through
//!   `daemon.info` on the local socket. A browser `Origin` header must be a
//!   loopback origin, so a web page elsewhere cannot drive the engine.
//!
//! Both transports call [`Registry::dispatch`]. Calls run on the blocking
//! pool, so a long render does not stall other connections' I/O (they still
//! wait for the project lock until jobs exist). A connection that calls
//! `events.subscribe` also gets `project.changed` notifications.
//!
//! The daemon exits after `idle` with no open connections, or on `daemon.stop`.

use std::io;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::{ListenerOptions, Name};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use ve_protocol::Registry;

use crate::project::{self, Located};
use crate::{DaemonInfo, ENGINE_VERSION, ExeIdentity, Session};

/// Default time without connections before the daemon exits.
pub const DEFAULT_IDLE: Duration = Duration::from_secs(600);

/// Messages queued per connection before a slow reader holds up its own responses.
const OUTBOX: usize = 256;

pub struct Options {
    pub project: PathBuf,
    pub idle: Duration,
    /// 0 picks a free port.
    pub ws_port: u16,
}

/// The local socket address for a project, as text (`daemon.info` shows it).
pub fn socket_address(loc: &Located) -> String {
    if cfg!(windows) { format!("ve-{}", loc.key) } else { loc.cache.join("daemon.sock").to_string_lossy().into_owned() }
}

/// The local socket name for an address from [`socket_address`].
pub fn socket_name(address: &str) -> io::Result<Name<'_>> {
    #[cfg(windows)]
    {
        address.to_ns_name::<interprocess::local_socket::GenericNamespaced>()
    }
    #[cfg(not(windows))]
    {
        address.to_fs_name::<interprocess::local_socket::GenericFilePath>()
    }
}

/// Run the daemon for `opts.project` until it goes idle or is stopped.
pub async fn run(opts: Options) -> io::Result<()> {
    let session = Arc::new(Session::default());
    let registry = crate::registry(session.clone());

    // Take the project lock first: a second daemon for the same project stops
    // here. A daemon that was just told to stop may still hold it briefly.
    let loc = project::locate(&opts.project).map_err(io::Error::other)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let open = call(&registry, "project.open", json!({ "path": loc.root })).await;
        match open {
            Ok(_) => break,
            Err(e) if e["code"] == -31003 && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => return Err(io::Error::other(e["message"].as_str().unwrap_or("could not open the project").to_owned())),
        }
    }
    std::fs::create_dir_all(&loc.cache)?;

    let address = socket_address(&loc);
    // Holding the project lock means no live daemon owns this name, so a
    // socket file left by a crashed one can be replaced.
    let listener = ListenerOptions::new().name(socket_name(&address)?).try_overwrite(true).create_tokio()?;
    let tcp = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, opts.ws_port)).await?;
    let port = tcp.local_addr()?.port();
    let token = nanoid::nanoid!(32);

    let info = DaemonInfo {
        pid: std::process::id(),
        engine: ENGINE_VERSION.into(),
        // The binary the CLI asked for, even when the daemon runs from a staged copy.
        exe: std::env::var_os(crate::client::ORIGIN_ENV).map_or_else(ExeIdentity::current, |p| ExeIdentity::of(std::path::Path::new(&p))),
        project: loc.root.clone(),
        socket: address,
        websocket: format!("ws://127.0.0.1:{port}/rpc?token={token}"),
        idle_timeout_secs: opts.idle.as_secs(),
    };
    tracing::info!("serving {} on {} and {}", info.project.display(), info.socket, info.websocket);
    let _ = session.daemon.set(info);

    let ctx = Arc::new(Ctx { registry, session: session.clone(), token, active: AtomicUsize::new(0), last: Mutex::new(Instant::now()) });

    let pipe = {
        let ctx = ctx.clone();
        async move {
            loop {
                match listener.accept().await {
                    Ok(stream) => {
                        tokio::spawn(pipe_connection(ctx.clone(), stream));
                    }
                    Err(e) => tracing::warn!("local socket accept: {e}"),
                }
            }
        }
    };
    let app = axum::Router::new().route("/rpc", get(ws_upgrade)).with_state(ctx.clone());
    let ws = axum::serve(tcp, app);

    tokio::select! {
        () = pipe => {}
        r = ws => r?,
        () = idle(ctx.clone(), opts.idle) => tracing::info!("idle for {}s; exiting", opts.idle.as_secs()),
        () = session.shutdown.notified() => {
            // Let the `daemon.stop` response reach the caller.
            tokio::time::sleep(Duration::from_millis(200)).await;
            tracing::info!("stopped by daemon.stop");
        }
    }
    Ok(())
}

async fn call(registry: &Registry, method: &str, params: Value) -> Result<Value, Value> {
    let req = json!({ "jsonrpc": "2.0", "id": 0, "method": method, "params": params }).to_string();
    let resp: Value = serde_json::from_str(&registry.dispatch(&req).await).unwrap_or(Value::Null);
    match resp.get("error") {
        Some(e) => Err(e.clone()),
        None => Ok(resp["result"].clone()),
    }
}

struct Ctx {
    registry: Arc<Registry>,
    session: Arc<Session>,
    token: String,
    /// Open connections, on both transports.
    active: AtomicUsize,
    /// Last request or disconnect.
    last: Mutex<Instant>,
}

impl Ctx {
    fn touch(&self) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }
}

async fn idle(ctx: Arc<Ctx>, timeout: Duration) {
    let tick = Duration::from_millis(250).min(timeout);
    loop {
        tokio::time::sleep(tick).await;
        let since = ctx.last.lock().unwrap_or_else(|e| e.into_inner()).elapsed();
        if ctx.active.load(Ordering::SeqCst) == 0 && since >= timeout {
            return;
        }
    }
}

/// One client connection, whatever the transport. Messages to the client go
/// through `out`; dropping the connection stops its event forwarding.
struct Connection {
    ctx: Arc<Ctx>,
    out: mpsc::Sender<String>,
    subscribed: AtomicBool,
    events: Mutex<Option<JoinHandle<()>>>,
}

impl Connection {
    fn new(ctx: Arc<Ctx>, out: mpsc::Sender<String>) -> Self {
        ctx.active.fetch_add(1, Ordering::SeqCst);
        ctx.touch();
        Self { ctx, out, subscribed: AtomicBool::new(false), events: Mutex::new(None) }
    }

    /// Handle one incoming message. Requests run concurrently; responses carry their ids.
    fn handle(&self, raw: String) {
        self.ctx.touch();
        // Subscribe before dispatching, so no change between the revision the
        // call returns and the first notification is lost.
        if is_call_to(&raw, "events.subscribe") && !self.subscribed.swap(true, Ordering::SeqCst) {
            let rx = self.ctx.session.events.subscribe();
            let task = tokio::spawn(forward_events(rx, self.out.clone()));
            *self.events.lock().unwrap_or_else(|e| e.into_inner()) = Some(task);
        }
        let (ctx, out) = (self.ctx.clone(), self.out.clone());
        tokio::spawn(async move {
            let registry = ctx.registry.clone();
            let handle = tokio::runtime::Handle::current();
            let resp = tokio::task::spawn_blocking(move || handle.block_on(registry.dispatch(&raw)))
                .await
                .unwrap_or_else(|e| {
                    json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32603, "message": format!("call panicked: {e}") } })
                        .to_string()
                });
            ctx.touch();
            // Notifications (no id) get no response.
            if !resp.trim().is_empty() {
                let _ = out.send(resp).await;
            }
        });
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(task) = self.events.lock().unwrap_or_else(|e| e.into_inner()).take() {
            task.abort();
        }
        self.ctx.active.fetch_sub(1, Ordering::SeqCst);
        self.ctx.touch();
    }
}

/// Cheap check of a message's method without a full parse of large params.
fn is_call_to(raw: &str, method: &str) -> bool {
    #[derive(Deserialize)]
    struct Head<'a> {
        #[serde(borrow)]
        method: Option<std::borrow::Cow<'a, str>>,
    }
    serde_json::from_str::<Head<'_>>(raw).ok().and_then(|h| h.method).is_some_and(|m| m == method)
}

async fn forward_events(mut rx: broadcast::Receiver<Arc<str>>, out: mpsc::Sender<String>) {
    loop {
        let msg = match rx.recv().await {
            Ok(m) => m.to_string(),
            Err(broadcast::error::RecvError::Lagged(n)) => crate::events::lagged(n),
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if out.send(msg).await.is_err() {
            return;
        }
    }
}

async fn pipe_connection(ctx: Arc<Ctx>, stream: interprocess::local_socket::tokio::Stream) {
    let (read, mut write) = tokio::io::split(stream);
    let (out, mut outbox) = mpsc::channel::<String>(OUTBOX);
    let writer = tokio::spawn(async move {
        while let Some(mut msg) = outbox.recv().await {
            msg.push('\n');
            if write.write_all(msg.as_bytes()).await.is_err() {
                return;
            }
        }
    });
    let conn = Connection::new(ctx, out);
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if !line.trim().is_empty() {
            conn.handle(line);
        }
    }
    drop(conn);
    let _ = writer.await;
}

#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

async fn ws_upgrade(State(ctx): State<Arc<Ctx>>, Query(q): Query<WsQuery>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    if q.token.as_deref() != Some(ctx.token.as_str()) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong token (see `ve daemon status`)").into_response();
    }
    if let Some(origin) = headers.get(header::ORIGIN)
        && !is_loopback_origin(origin.to_str().unwrap_or(""))
    {
        return (StatusCode::FORBIDDEN, "only loopback origins may connect").into_response();
    }
    ws.on_upgrade(move |socket| ws_connection(ctx, socket))
}

/// `http://localhost:5173`, `http://127.0.0.1`, `https://[::1]:8443`: yes. Anything else, or `null`: no.
fn is_loopback_origin(origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://")) else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        authority.split_inclusive(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "[::1]")
}

async fn ws_connection(ctx: Arc<Ctx>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (out, mut outbox) = mpsc::channel::<String>(OUTBOX);
    let writer = tokio::spawn(async move {
        while let Some(msg) = outbox.recv().await {
            if sink.send(Message::Text(msg.into())).await.is_err() {
                return;
            }
        }
    });
    let conn = Connection::new(ctx, out);
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(text) => conn.handle(text.to_string()),
            Message::Close(_) => break,
            // Pings are answered by the WebSocket layer; binary frames are not part of the protocol.
            _ => {}
        }
    }
    drop(conn);
    let _ = writer.await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_origins() {
        for ok in ["http://localhost:5173", "http://127.0.0.1", "https://[::1]:8443", "http://LOCALHOST:3000/x"] {
            assert!(is_loopback_origin(ok), "{ok}");
        }
        for bad in ["null", "http://example.com", "http://localhost.evil.com", "http://127.0.0.1.nip.io", "file://", "ws://localhost"] {
            assert!(!is_loopback_origin(bad), "{bad}");
        }
    }

    #[test]
    fn spots_subscribe_calls() {
        assert!(is_call_to(r#"{"jsonrpc":"2.0","id":1,"method":"events.subscribe"}"#, "events.subscribe"));
        assert!(!is_call_to(r#"{"jsonrpc":"2.0","id":1,"method":"project.get"}"#, "events.subscribe"));
        assert!(!is_call_to("not json", "events.subscribe"));
    }
}
