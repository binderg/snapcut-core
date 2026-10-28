//! Plugins in the daemon: their methods join the registry (through
//! [`ve_protocol::Extension`]), slow ones run as jobs, and their calls back
//! into the engine go through the same registry as actor `plugin:<id>`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};
use ve_plugin::{CallHooks, Host};
use ve_protocol::{DynMethod, ErrorCode, Extension, Registry, RpcError};

use crate::{ENGINE_VERSION, Session};

/// The session's plugin host, made for the open project (or none).
#[derive(Default)]
pub struct PluginSlot(Mutex<Option<(Option<PathBuf>, Arc<Host>)>>);

impl PluginSlot {
    /// Stop every plugin process (the daemon is exiting).
    pub fn shutdown(&self) {
        if let Some((_, host)) = self.0.lock().unwrap_or_else(|e| e.into_inner()).take() {
            host.shutdown();
        }
    }
}

/// Namespaces plugins may never use, besides those of built-in methods.
const RESERVED: [&str; 6] = ["system", "plugin", "ui", "daemon", "rpc", "ve"];

/// The registry's extension: everything plugins add. Holds only weak
/// references, so a dropped session or registry is freed (and its plugin
/// processes stopped).
pub struct PluginMethods {
    session: Weak<Session>,
    registry: OnceLock<Weak<Registry>>,
    reserved: OnceLock<Arc<HashSet<String>>>,
}

fn gone() -> RpcError {
    RpcError::new(ErrorCode::Internal, "the engine is shutting down")
}

impl PluginMethods {
    pub fn new(session: &Arc<Session>) -> Arc<Self> {
        Arc::new(Self { session: Arc::downgrade(session), registry: OnceLock::new(), reserved: OnceLock::new() })
    }

    /// Hook into the sealed registry: unknown methods and `describe` now reach plugins.
    pub fn attach(self: &Arc<Self>, registry: &Arc<Registry>) {
        let mut reserved: HashSet<String> = registry.specs().iter().filter_map(|s| s.name.split('.').next()).map(String::from).collect();
        reserved.extend(RESERVED.iter().map(|s| (*s).to_owned()));
        let _ = self.reserved.set(Arc::new(reserved));
        let _ = self.registry.set(Arc::downgrade(registry));
        registry.set_extension(self.clone());
        // Renders reach plugin audio effects through this.
        let me: Arc<dyn ve_render::plugin_fx::Processor> = self.clone();
        ve_render::plugin_fx::register(Arc::downgrade(&me));
    }

    fn registry(&self) -> Weak<Registry> {
        self.registry.get().cloned().unwrap_or_default()
    }

    /// The host for the session's current project, made on first use.
    fn host(&self) -> Result<Arc<Host>, RpcError> {
        let session = self.session.upgrade().ok_or_else(gone)?;
        let paths = session.opened.get().cloned().or_else(|| session.paths());
        let root = paths.as_ref().map(|(r, _)| r.clone());
        let mut slot = session.plugins.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((r, h)) = slot.as_ref()
            && *r == root
        {
            return Ok(h.clone());
        }
        let logs = match &paths {
            Some((_, cache)) => cache.join("plugins"),
            None => crate::project::cache_base().map(|b| b.join("plugins")).unwrap_or_else(|_| std::env::temp_dir().join("ve-plugins")),
        };
        let reserved = self.reserved.get().cloned().unwrap_or_default();
        let registry = self.registry();
        let idle = std::env::var("VE_PLUGIN_IDLE_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .map_or(ve_plugin::host::DEFAULT_IDLE, std::time::Duration::from_secs);
        let host = Host::new(ve_plugin::Config {
            engine: semver::Version::parse(ENGINE_VERSION).expect("engine version is semver"),
            project: root.clone(),
            user_dir: ve_plugin::user_dir(),
            logs,
            reserved: Arc::new(move |ns: &str| reserved.contains(ns)),
            engine_call: Arc::new(move |plugin: &str, method: &str, params: Value| engine_call(&registry, plugin, method, params)),
            idle,
        });
        if let Some((_, old)) = slot.replace((root, host.clone())) {
            old.shutdown();
        }
        Ok(host)
    }

}

/// Plugin audio effects (`runtime: "process"`) during renders of this
/// session's project ([`ve_render::plugin_fx`]).
impl ve_render::plugin_fx::Processor for PluginMethods {
    fn serves(&self, root: &std::path::Path) -> bool {
        self.session.upgrade().and_then(|s| s.opened.get().map(|(r, _)| r == root)).unwrap_or(false)
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        self.session.upgrade().and_then(|s| s.opened.get().map(|(_, c)| c.join(crate::cache::AUDIO_FX)))
    }

    fn version(&self, method: &str) -> Option<String> {
        let host = self.host().ok()?;
        let (p, _) = host.command(method).or_else(|| {
            host.rescan();
            host.command(method)
        })?;
        Some(format!("{}@{}", p.manifest.id, p.manifest.version))
    }

    fn process(&self, method: &str, params: Value) -> Result<Value, String> {
        let host = self.host().map_err(|e| e.message)?;
        if host.command(method).is_none() {
            host.rescan();
        }
        if host.command(method).is_none() {
            return Err(format!("no installed plugin provides `{method}`; `ve plugin list` shows what is installed"));
        }
        // Progress belongs to the render (frames done); only cancel is passed on.
        let hooks = CallHooks { on_progress: &|_, _| {}, cancelled: &ve_render::control::cancelled };
        host.call(method, params, &hooks).map_err(|e| e.message)
    }
}

impl Extension for PluginMethods {
    fn methods(&self) -> Vec<DynMethod> {
        let Ok(host) = self.host() else { return Vec::new() };
        let mut out = Vec::new();
        for p in host.plugins() {
            for c in &p.manifest.contributes.commands {
                out.push(DynMethod {
                    name: c.method.clone(),
                    summary: c.summary.clone(),
                    mutates: c.mutates,
                    params: params_schema(c),
                    result: c.result_schema(),
                    plugin: p.manifest.id.clone(),
                });
            }
        }
        out
    }

    fn call(&self, method: &str, mut params: Value) -> Option<Result<Value, RpcError>> {
        let host = self.host().ok()?;
        let (plugin, cmd) = match host.command(method) {
            Some(found) => found,
            None => {
                // Installed while the engine was running?
                host.rescan();
                host.command(method)?
            }
        };
        let background = cmd.job && params.get("background").and_then(Value::as_bool) == Some(true);
        if cmd.job
            && let Some(o) = params.as_object_mut()
        {
            o.remove("background");
        }
        let registry = self.registry();
        let plugin_id = plugin.manifest.id.clone();
        let name = method.to_owned();
        let work = move || -> Result<Value, RpcError> {
            let actor = json!({ "kind": "plugin", "id": plugin_id });
            // The plugin's edits during this call undo as one step.
            let grouped = cmd.mutates
                && dispatch(&registry, "history.begin_group", json!({ "label": format!("{plugin_id}: {name}"), "actor": actor })).is_ok();
            let hooks = CallHooks {
                on_progress: &|p, _| ve_render::control::report_progress(p),
                cancelled: &ve_render::control::cancelled,
            };
            let result = host.call(&name, params, &hooks);
            if grouped {
                let _ = dispatch(&registry, "history.end_group", json!({ "actor": actor }));
            }
            result
        };
        if background {
            // Progress is reported as a fraction, so the job's "length" is 1.
            let Some(session) = self.session.upgrade() else { return Some(Err(gone())) };
            let started = session.jobs.start(method, Some(1.0), work);
            return Some(serde_json::to_value(started).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string())));
        }
        Some(work())
    }
}

/// The command's params schema, plus `background` for job commands and the
/// usual `base_rev` / `dry_run` / `actor` for commands that edit (the plugin
/// reads them; clients fill them in as for any edit).
fn params_schema(c: &ve_plugin::manifest::CommandSpec) -> Value {
    let mut s = c.params_schema();
    let Some(props) = s.as_object_mut().map(|o| o.entry("properties").or_insert_with(|| json!({}))).and_then(Value::as_object_mut) else {
        return s;
    };
    let mut add = |name: &str, schema: Value| {
        props.entry(name).or_insert(schema);
    };
    if c.job {
        add("background", json!({ "type": "boolean", "description": "Answer `{job_id}` at once and run as a background job" }));
    }
    if c.mutates {
        add("base_rev", json!({ "type": "integer", "minimum": 0, "description": "The revision this call was planned against" }));
        add("dry_run", json!({ "type": "boolean", "description": "Report what would change without changing anything (if the plugin supports it)" }));
        add("actor", json!({ "type": "object", "description": "Who asked for the edit" }));
    }
    s
}

/// A call from plugin `plugin` into the engine. Methods that record who
/// acted get `actor: plugin:<id>`, whatever the plugin sent.
fn engine_call(registry: &Weak<Registry>, plugin: &str, method: &str, mut params: Value) -> Result<Value, RpcError> {
    let r = registry.upgrade().ok_or_else(gone)?;
    let takes_actor = r.specs().iter().find(|s| s.name == method).is_some_and(|s| {
        s.params.get("properties").and_then(Value::as_object).is_some_and(|p| p.contains_key("actor"))
    });
    if takes_actor && let Some(o) = params.as_object_mut() {
        o.insert("actor".into(), json!({ "kind": "plugin", "id": plugin }));
    }
    dispatch_on(&r, method, params)
}

fn dispatch(registry: &Weak<Registry>, method: &str, params: Value) -> Result<Value, RpcError> {
    let r = registry.upgrade().ok_or_else(gone)?;
    dispatch_on(&r, method, params)
}

/// Run one call through the registry from a plain thread. Methods are
/// synchronous, so no async runtime is needed to drive the dispatch.
fn dispatch_on(r: &Registry, method: &str, params: Value) -> Result<Value, RpcError> {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let resp: Value = serde_json::from_str(&futures::executor::block_on(r.dispatch(&req)))
        .map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
    match resp.get("error") {
        Some(e) => Err(RpcError {
            code: ErrorCode::from_rpc_code(e["code"].as_i64().unwrap_or(0)),
            message: e["message"].as_str().unwrap_or("error").to_owned(),
            data: e.get("data").cloned().filter(|d| !d.is_null()),
        }),
        None => Ok(resp["result"].clone()),
    }
}

// --- plugin.list ---

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PluginList {
    pub plugins: Vec<PluginInfo>,
    /// Plugin folders that could not be loaded, and why.
    pub issues: Vec<IssueInfo>,
    /// Where `ve plugin install` puts plugins.
    pub user_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `project` (the project's plugins/ folder) or `user`.
    pub source: String,
    pub dir: PathBuf,
    /// Its process is running now.
    pub running: bool,
    pub commands: Vec<CommandInfo>,
    /// How many effects and transitions it contributes (an effect pack);
    /// `fx.list` lists them.
    pub effects: usize,
    pub secrets: Vec<SecretInfo>,
    /// Its SKILL.md, for agents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CommandInfo {
    pub method: String,
    pub summary: String,
    pub mutates: bool,
    /// Can run as a background job.
    pub job: bool,
    /// `ve <name>` runs it; `args` are its positional params, in order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<CliInfo>,
    /// JSON Schema of its params.
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CliInfo {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SecretInfo {
    pub name: String,
    /// `environment`, `keychain` or `missing`.
    pub source: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct IssueInfo {
    pub dir: PathBuf,
    pub message: String,
}

fn label<T: Serialize>(v: T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

impl PluginMethods {
    /// `plugin.list`: rescans the plugin folders first.
    pub fn list(&self) -> Result<PluginList, RpcError> {
    let host = self.host()?;
    host.rescan();
    let running = host.running();
    let plugins = host
        .plugins()
        .into_iter()
        .map(|p| {
            let m = &p.manifest;
            PluginInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.to_string(),
                description: m.description.clone(),
                source: label(p.source),
                running: running.contains_key(&m.id),
                commands: m
                    .contributes
                    .commands
                    .iter()
                    .map(|c| CommandInfo {
                        method: c.method.clone(),
                        summary: c.summary.clone(),
                        mutates: c.mutates,
                        job: c.job,
                        cli: c.cli.as_ref().map(|cli| CliInfo { name: cli.name.clone(), args: cli.args.clone() }),
                        params: params_schema(c),
                    })
                    .collect(),
                effects: m.effect_dirs(&p.dir).len(),
                secrets: m.secrets.iter().map(|s| SecretInfo { name: s.clone(), source: label(ve_plugin::secrets::source(&m.id, s)) }).collect(),
                skill: m.skill_path(&p.dir),
                dir: p.dir,
            }
        })
        .collect();
    let issues = host.issues().into_iter().map(|i| IssueInfo { dir: i.dir, message: i.message }).collect();
    Ok(PluginList { plugins, issues, user_dir: ve_plugin::user_dir() })
    }
}
