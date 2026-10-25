//! One running plugin process, spoken to over stdio.
//!
//! Newline-delimited JSON-RPC 2.0 both ways, like a language server:
//!
//! - The engine sends `initialize` once, then method calls. While a call
//!   runs it may send `$/cancel` `{id}`. Before stopping the process it sends
//!   the `shutdown` notification and closes stdin.
//! - The plugin answers each call, and may meanwhile call any engine method
//!   (its own requests, own ids) and send `$/progress` `{id, progress,
//!   message?}` for the call `id` it is working on, or `$/log` `{level,
//!   message}`.
//!
//! stderr goes to a log file. Everything here is plain threads: calls arrive
//! on the daemon's blocking pool or a job thread and wait for their answer.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use ve_protocol::{ErrorCode, RpcError};

/// How long a plugin gets to answer `initialize`.
const INIT_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a cancelled call gets to wind down before the process is killed.
pub const CANCEL_GRACE: Duration = Duration::from_secs(5);

/// The engine's side of calls the plugin makes back into it.
pub type Callback = Arc<dyn Fn(&str, Value) -> Result<Value, RpcError> + Send + Sync>;

/// How the process is started.
pub struct Spawn {
    pub plugin_id: String,
    pub command: String,
    pub args: Vec<String>,
    /// Working directory: the plugin's folder.
    pub dir: PathBuf,
    pub env: Vec<(String, String)>,
    /// stderr is appended here.
    pub log: PathBuf,
    /// Sent as the `initialize` params.
    pub init: Value,
}

/// What a waiting call can receive.
enum Incoming {
    Response(Value),
    Progress(f64, Option<String>),
    Exited,
}

/// Hooks a call reports to.
pub struct CallHooks<'a> {
    pub on_progress: &'a dyn Fn(f64, Option<&str>),
    /// Polled about ten times a second; once true, the call is cancelled.
    pub cancelled: &'a dyn Fn() -> bool,
}

impl CallHooks<'_> {
    pub fn none() -> CallHooks<'static> {
        CallHooks { on_progress: &|_, _| {}, cancelled: &|| false }
    }
}

pub struct Process {
    plugin_id: String,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    pending: Arc<Mutex<HashMap<u64, Sender<Incoming>>>>,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    last_used: Mutex<Instant>,
    /// Calls in flight, so an idle sweep never stops a busy plugin.
    busy: AtomicU64,
    log: PathBuf,
}

impl Process {
    /// Start the process and run the `initialize` handshake.
    pub fn start(spawn: Spawn, callback: Callback) -> Result<Arc<Self>, RpcError> {
        let program = resolve_program(&spawn.command, &spawn.dir);
        if let Some(parent) = spawn.log.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spawn.log)
            .map_err(|e| plugin_error(&spawn.plugin_id, format!("opening its log {}: {e}", spawn.log.display())))?;
        let mut cmd = Command::new(&program);
        cmd.args(&spawn.args)
            .current_dir(&spawn.dir)
            .envs(spawn.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log));
        hide_window(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| {
            plugin_error(&spawn.plugin_id, format!("could not start `{}`: {e}", program.display()))
        })?;
        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let process = Arc::new(Self {
            plugin_id: spawn.plugin_id.clone(),
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(child),
            pending: Arc::default(),
            next_id: AtomicU64::new(1),
            alive: Arc::new(AtomicBool::new(true)),
            last_used: Mutex::new(Instant::now()),
            busy: AtomicU64::new(0),
            log: spawn.log.clone(),
        });
        let reader = process.clone();
        std::thread::Builder::new()
            .name(format!("ve-plugin-{}", spawn.plugin_id))
            .spawn(move || reader.read_loop(stdout, callback))
            .map_err(|e| plugin_error(&spawn.plugin_id, e.to_string()))?;

        let hooks = CallHooks::none();
        match process.call_with_timeout("initialize", spawn.init, &hooks, Some(INIT_TIMEOUT)) {
            Ok(_) => Ok(process),
            Err(e) => {
                process.kill();
                Err(RpcError { message: format!("{} (during initialize)", e.message), ..e })
            }
        }
    }

    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub fn pid(&self) -> u32 {
        self.child.lock().unwrap_or_else(|e| e.into_inner()).id()
    }

    /// Time since the last call finished; `None` while a call runs.
    pub fn idle_for(&self) -> Option<Duration> {
        if self.busy.load(Ordering::SeqCst) > 0 {
            return None;
        }
        Some(self.last_used.lock().unwrap_or_else(|e| e.into_inner()).elapsed())
    }

    /// Call `method` and wait for the answer, reporting progress and
    /// honouring cancel through `hooks`.
    pub fn call(&self, method: &str, params: Value, hooks: &CallHooks<'_>) -> Result<Value, RpcError> {
        self.call_with_timeout(method, params, hooks, None)
    }

    fn call_with_timeout(&self, method: &str, params: Value, hooks: &CallHooks<'_>, timeout: Option<Duration>) -> Result<Value, RpcError> {
        if !self.alive() {
            return Err(self.exited_error());
        }
        self.busy.fetch_add(1, Ordering::SeqCst);
        let _done = Done(self);
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(id, tx);
        if let Err(e) = self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })) {
            self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
            return Err(e);
        }
        let started = Instant::now();
        let mut cancel_sent: Option<Instant> = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Incoming::Response(resp)) => {
                    if cancel_sent.is_some() {
                        return Err(RpcError::new(ErrorCode::Cancelled, "cancelled"));
                    }
                    return response(&self.plugin_id, resp);
                }
                Ok(Incoming::Progress(p, msg)) => (hooks.on_progress)(p, msg.as_deref()),
                Ok(Incoming::Exited) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(if cancel_sent.is_some() { RpcError::new(ErrorCode::Cancelled, "cancelled") } else { self.exited_error() });
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            match cancel_sent {
                None if (hooks.cancelled)() => {
                    let _ = self.send(&json!({ "jsonrpc": "2.0", "method": "$/cancel", "params": { "id": id } }));
                    cancel_sent = Some(Instant::now());
                }
                Some(t) if t.elapsed() > CANCEL_GRACE => {
                    tracing::warn!("plugin {} ignored $/cancel for {}s; killing it", self.plugin_id, CANCEL_GRACE.as_secs());
                    self.kill();
                    self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                    return Err(RpcError::new(ErrorCode::Cancelled, "cancelled"));
                }
                _ => {}
            }
            if let Some(limit) = timeout
                && started.elapsed() > limit
            {
                self.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
                return Err(plugin_error(&self.plugin_id, format!("no answer to `{method}` within {}s", limit.as_secs())));
            }
        }
    }

    /// Ask the process to exit, and kill it if it has not after `grace`.
    pub fn shutdown(&self, grace: Duration) {
        if self.alive() {
            let _ = self.send(&json!({ "jsonrpc": "2.0", "method": "shutdown", "params": {} }));
        }
        drop(self.stdin.lock().unwrap_or_else(|e| e.into_inner()).take());
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if matches!(self.child.lock().unwrap_or_else(|e| e.into_inner()).try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.kill();
    }

    pub fn kill(&self) {
        self.alive.store(false, Ordering::SeqCst);
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        let _ = child.wait();
    }

    fn send(&self, msg: &Value) -> Result<(), RpcError> {
        let mut line = msg.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().unwrap_or_else(|e| e.into_inner());
        let Some(pipe) = stdin.as_mut() else { return Err(self.exited_error()) };
        pipe.write_all(line.as_bytes()).and_then(|()| pipe.flush()).map_err(|_| self.exited_error())
    }

    fn exited_error(&self) -> RpcError {
        let mut e = plugin_error(&self.plugin_id, format!("the plugin process exited (log: {})", self.log.display()));
        let tail = log_tail(&self.log, 20);
        if !tail.is_empty() {
            e.message = format!("{}; last lines:\n{tail}", e.message);
        }
        e
    }

    /// The reader thread: route responses and progress to waiting calls,
    /// run the plugin's own requests against the engine.
    fn read_loop(self: Arc<Self>, stdout: std::process::ChildStdout, callback: Callback) {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let msg: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!("plugin {}: not JSON on stdout (log to stderr instead): {line}", self.plugin_id);
                    continue;
                }
            };
            match (msg.get("method").and_then(Value::as_str), msg.get("id")) {
                (Some(method), Some(id)) => self.serve(method.to_owned(), id.clone(), msg.get("params").cloned(), callback.clone()),
                (Some("$/progress"), None) => {
                    let p = &msg["params"];
                    if let (Some(id), Some(progress)) = (p["id"].as_u64(), p["progress"].as_f64()) {
                        let message = p["message"].as_str().map(String::from);
                        self.route(id, Incoming::Progress(progress.clamp(0.0, 1.0), message), false);
                    }
                }
                (Some("$/log"), None) => {
                    let text = msg["params"]["message"].as_str().unwrap_or_default();
                    match msg["params"]["level"].as_str() {
                        Some("error") => tracing::error!("plugin {}: {text}", self.plugin_id),
                        Some("warn") => tracing::warn!("plugin {}: {text}", self.plugin_id),
                        _ => tracing::info!("plugin {}: {text}", self.plugin_id),
                    }
                }
                (Some(_), None) => {}
                (None, Some(id)) => {
                    if let Some(id) = id.as_u64() {
                        self.route(id, Incoming::Response(msg), true);
                    }
                }
                (None, None) => {}
            }
        }
        self.alive.store(false, Ordering::SeqCst);
        for (_, tx) in self.pending.lock().unwrap_or_else(|e| e.into_inner()).drain() {
            let _ = tx.send(Incoming::Exited);
        }
        // Reap it so it does not linger as a zombie.
        let _ = self.child.lock().unwrap_or_else(|e| e.into_inner()).wait();
    }

    fn route(&self, id: u64, msg: Incoming, last: bool) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let tx = if last { pending.remove(&id) } else { pending.get(&id).cloned() };
        if let Some(tx) = tx {
            let _ = tx.send(msg);
        }
    }

    /// A request from the plugin: run it on its own thread (it may take a
    /// while, and the plugin may send more meanwhile) and answer.
    fn serve(self: &Arc<Self>, method: String, id: Value, params: Option<Value>, callback: Callback) {
        let me = self.clone();
        let spawned = std::thread::Builder::new().name(format!("ve-plugin-{}-call", self.plugin_id)).spawn(move || {
            let params = params.unwrap_or_else(|| json!({}));
            let resp = match callback(&method, params) {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": e.code.rpc_code(), "message": e.message, "data": e.data } }),
            };
            let _ = me.send(&resp);
        });
        if let Err(e) = spawned {
            tracing::error!("plugin {}: could not serve a call: {e}", self.plugin_id);
        }
    }
}

/// Marks a call finished when dropped.
struct Done<'a>(&'a Process);

impl Drop for Done<'_> {
    fn drop(&mut self) {
        *self.0.last_used.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
        self.0.busy.fetch_sub(1, Ordering::SeqCst);
    }
}

fn response(plugin: &str, resp: Value) -> Result<Value, RpcError> {
    if let Some(err) = resp.get("error") {
        let code = err["code"].as_i64().map_or(ErrorCode::PluginError, ErrorCode::from_rpc_code);
        // Engine errors the plugin passed on keep their code; anything else is the plugin's.
        let code = if code == ErrorCode::Internal { ErrorCode::PluginError } else { code };
        let message = err["message"].as_str().unwrap_or("the plugin reported an error");
        return Err(RpcError { code, message: format!("{plugin}: {message}"), data: err.get("data").cloned().filter(|d| !d.is_null()) });
    }
    Ok(resp.get("result").cloned().unwrap_or(Value::Null))
}

pub(crate) fn plugin_error(plugin: &str, message: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::PluginError, format!("plugin {plugin}: {message}"))
}

/// A command with a path separator is relative to the plugin folder.
fn resolve_program(command: &str, dir: &Path) -> PathBuf {
    let p = Path::new(command);
    if p.is_absolute() || !(command.contains('/') || command.contains('\\')) { p.to_owned() } else { dir.join(p) }
}

/// The last `n` lines of a log file.
fn log_tail(path: &Path, n: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else { return String::new() };
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(windows)]
fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: the daemon has no console, so a console program
    // (python) would otherwise open a window of its own.
    cmd.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn hide_window(_: &mut Command) {}
