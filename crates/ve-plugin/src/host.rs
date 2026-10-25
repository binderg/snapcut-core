//! The plugin host: finds plugins, starts their processes on first use,
//! routes calls to them and stops them when idle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use ve_protocol::{PROTOCOL_VERSION, RpcError};

use crate::manifest::{CommandSpec, Manifest};
use crate::process::{CallHooks, Callback, Process, Spawn, plugin_error};
use crate::secrets::{self, SecretSource};

/// How long a plugin process may sit unused before it is stopped.
pub const DEFAULT_IDLE: Duration = Duration::from_secs(300);

/// Where a plugin was found. Earlier sources win when two have the same id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The project's `plugins/` folder.
    Project,
    /// The user's plugin folder (`ve plugin install`).
    User,
}

#[derive(Debug, Clone)]
pub struct Plugin {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub source: Source,
}

/// A plugin folder that could not be used, and why.
#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub dir: PathBuf,
    pub message: String,
}

/// The engine's answer to a call a plugin makes: `(plugin id, method, params)`.
pub type EngineCall = Arc<dyn Fn(&str, &str, Value) -> Result<Value, RpcError> + Send + Sync>;

pub struct Config {
    pub engine: semver::Version,
    /// The open project, if any; its `plugins/` folder is searched first.
    pub project: Option<PathBuf>,
    /// The user plugin folder; `None` searches only the project.
    pub user_dir: Option<PathBuf>,
    /// Plugin logs go to `<logs>/<id>.log`.
    pub logs: PathBuf,
    /// Method namespaces the engine owns; plugins may not use them.
    pub reserved: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    pub engine_call: EngineCall,
    pub idle: Duration,
}

#[derive(Default)]
struct State {
    plugins: Vec<Plugin>,
    issues: Vec<Issue>,
    /// method -> (plugin index, command index)
    commands: HashMap<String, (usize, usize)>,
}

/// A plugin's process, if started. Locked while it starts.
type Slot = Arc<Mutex<Option<Arc<Process>>>>;

pub struct Host {
    cfg: Config,
    state: RwLock<State>,
    /// One slot per plugin id, so a slow start blocks only that plugin.
    procs: Mutex<HashMap<String, Slot>>,
}

impl Host {
    pub fn new(cfg: Config) -> Arc<Self> {
        let host = Arc::new(Self { cfg, state: RwLock::default(), procs: Mutex::default() });
        host.rescan();
        let weak = Arc::downgrade(&host);
        let _ = std::thread::Builder::new().name("ve-plugin-reaper".into()).spawn(move || reaper(&weak));
        host
    }

    /// Read every plugin folder again (a plugin installed while the engine runs).
    pub fn rescan(&self) {
        let mut st = State::default();
        let mut roots: Vec<(Source, PathBuf)> = Vec::new();
        if let Some(p) = &self.cfg.project {
            roots.push((Source::Project, p.join("plugins")));
        }
        if let Some(u) = &self.cfg.user_dir {
            roots.push((Source::User, u.clone()));
        }
        for (source, root) in roots {
            let Ok(entries) = std::fs::read_dir(&root) else { continue };
            let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join("plugin.json").is_file()).collect();
            dirs.sort();
            for dir in dirs {
                if let Err(message) = self.add(&mut st, source, &dir) {
                    st.issues.push(Issue { dir, message });
                }
            }
        }
        *self.state.write().unwrap_or_else(|e| e.into_inner()) = st;
    }

    fn add(&self, st: &mut State, source: Source, dir: &Path) -> Result<(), String> {
        let m = Manifest::load(dir).map_err(|e| e.to_string())?;
        if let Some(other) = st.plugins.iter().find(|p| p.manifest.id == m.id) {
            return Err(format!("plugin {} is also in {}, which takes priority", m.id, other.dir.display()));
        }
        if !m.supports(&self.cfg.engine) {
            return Err(format!("plugin {} {} wants engine {}, this is {}", m.id, m.version, m.engine, self.cfg.engine));
        }
        if m.protocol.split('.').next() != Some(PROTOCOL_VERSION.to_string().as_str()) {
            return Err(format!("plugin {} speaks protocol {}, this engine speaks {PROTOCOL_VERSION}", m.id, m.protocol));
        }
        for c in &m.contributes.commands {
            let ns = c.method.split('.').next().unwrap_or_default();
            if (self.cfg.reserved)(ns) {
                return Err(format!("plugin {}: method {} uses the engine's `{ns}` namespace", m.id, c.method));
            }
            if let Some((pi, _)) = st.commands.get(&c.method) {
                return Err(format!("plugin {}: method {} is already provided by {}", m.id, c.method, st.plugins[*pi].manifest.id));
            }
        }
        let pi = st.plugins.len();
        for (ci, c) in m.contributes.commands.iter().enumerate() {
            st.commands.insert(c.method.clone(), (pi, ci));
        }
        st.plugins.push(Plugin { manifest: m, dir: dir.to_owned(), source });
        Ok(())
    }

    pub fn plugins(&self) -> Vec<Plugin> {
        self.state.read().unwrap_or_else(|e| e.into_inner()).plugins.clone()
    }

    pub fn issues(&self) -> Vec<Issue> {
        self.state.read().unwrap_or_else(|e| e.into_inner()).issues.clone()
    }

    /// The plugin and command behind `method`.
    pub fn command(&self, method: &str) -> Option<(Plugin, CommandSpec)> {
        let st = self.state.read().unwrap_or_else(|e| e.into_inner());
        let (pi, ci) = *st.commands.get(method)?;
        let p = st.plugins[pi].clone();
        let c = p.manifest.contributes.commands[ci].clone();
        Some((p, c))
    }

    /// Process ids of running plugins.
    pub fn running(&self) -> HashMap<String, u32> {
        let procs = self.procs.lock().unwrap_or_else(|e| e.into_inner()).clone();
        procs
            .into_iter()
            .filter_map(|(id, slot)| {
                let slot = slot.try_lock().ok()?;
                let p = slot.as_ref().filter(|p| p.alive())?;
                Some((id, p.pid()))
            })
            .collect()
    }

    /// Run `method` in its plugin, starting the process if needed.
    pub fn call(&self, method: &str, params: Value, hooks: &CallHooks<'_>) -> Result<Value, RpcError> {
        let (plugin, _) = self.command(method).ok_or_else(|| RpcError::invalid_params(format!("no plugin provides `{method}`")))?;
        let process = self.process(&plugin)?;
        process.call(method, params, hooks)
    }

    fn process(&self, plugin: &Plugin) -> Result<Arc<Process>, RpcError> {
        let id = &plugin.manifest.id;
        let slot = self.procs.lock().unwrap_or_else(|e| e.into_inner()).entry(id.clone()).or_default().clone();
        let mut slot = slot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = slot.as_ref().filter(|p| p.alive()) {
            return Ok(p.clone());
        }
        let run = plugin.manifest.run.as_ref().ok_or_else(|| plugin_error(id, "has no `run` in its manifest"))?;
        let (command, args) = run.for_this_os();
        let mut env = vec![
            ("VE_PLUGIN_ID".to_owned(), id.clone()),
            ("VE_PLUGIN_DIR".to_owned(), plugin.dir.to_string_lossy().into_owned()),
            ("VE_ENGINE_VERSION".to_owned(), self.cfg.engine.to_string()),
            // Plugins written in Python print UTF-8 and do not buffer stdout.
            ("PYTHONIOENCODING".to_owned(), "utf-8".to_owned()),
            ("PYTHONUNBUFFERED".to_owned(), "1".to_owned()),
        ];
        for name in &plugin.manifest.secrets {
            match secrets::source(id, name) {
                SecretSource::Environment => {}
                SecretSource::Keychain => env.extend(secrets::keychain_get(id, name).map(|v| (name.clone(), v))),
                SecretSource::Missing => {
                    return Err(plugin_error(
                        id,
                        format!("needs {name}: run `ve plugin secret set {id} {name}` (or set {name} in the environment)"),
                    ));
                }
            }
        }
        let init = json!({
            "engine": self.cfg.engine.to_string(),
            "protocol": PROTOCOL_VERSION,
            "plugin": { "id": id, "version": plugin.manifest.version.to_string(), "dir": plugin.dir },
            "project": self.cfg.project,
        });
        let engine_call = self.cfg.engine_call.clone();
        let plugin_id = id.clone();
        let callback: Callback = Arc::new(move |method: &str, params: Value| engine_call(&plugin_id, method, params));
        let spawn = Spawn {
            plugin_id: id.clone(),
            command,
            args,
            dir: plugin.dir.clone(),
            env,
            log: self.cfg.logs.join(format!("{id}.log")),
            init,
        };
        tracing::info!("starting plugin {id} from {}", plugin.dir.display());
        let p = Process::start(spawn, callback)?;
        *slot = Some(p.clone());
        Ok(p)
    }

    /// Stop plugins unused for longer than the idle timeout.
    pub fn stop_idle(&self) {
        let procs = self.procs.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for (id, slot) in procs {
            let Ok(mut slot) = slot.try_lock() else { continue };
            let idle = slot.as_ref().and_then(|p| p.idle_for());
            if idle.is_some_and(|d| d >= self.cfg.idle) {
                tracing::info!("stopping idle plugin {id}");
                if let Some(p) = slot.take() {
                    p.shutdown(Duration::from_secs(2));
                }
            }
        }
    }

    /// Stop every plugin process (the engine is exiting).
    pub fn shutdown(&self) {
        let procs = std::mem::take(&mut *self.procs.lock().unwrap_or_else(|e| e.into_inner()));
        for (_, slot) in procs {
            if let Some(p) = slot.lock().unwrap_or_else(|e| e.into_inner()).take() {
                p.shutdown(Duration::from_secs(2));
            }
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn reaper(host: &Weak<Host>) {
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let Some(h) = host.upgrade() else { return };
        h.stop_idle();
    }
}
