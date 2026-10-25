//! The one protocol every client speaks: JSON-RPC 2.0, self-described by OpenRPC.
//!
//! Methods are registered once in a [`Registry`]. Every transport (CLI, local
//! socket / named pipe, WebSocket, plugin stdio) hands raw JSON to
//! [`Registry::dispatch`], so there is exactly one code path per method and the
//! OpenRPC document is generated from the same registrations, so it cannot drift.

mod error;
mod openrpc;

use std::sync::{Arc, RwLock};

use jsonrpsee::RpcModule;
use jsonrpsee::types::{ErrorObjectOwned, Params};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use error::{ErrorCode, RpcError};

/// Protocol major version. Breaking changes only in a new major.
pub const PROTOCOL_VERSION: u32 = 1;

/// Who made a call. Recorded in history so edits can be shown and undone by actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Actor {
    pub kind: ActorKind,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    User,
    Agent,
    Plugin,
}

impl Actor {
    pub fn user() -> Self {
        Self { kind: ActorKind::User, id: "local".into() }
    }
}

/// Everything `system.describe` needs to know about one method.
#[derive(Debug, Clone)]
pub struct MethodSpec {
    pub name: &'static str,
    pub summary: &'static str,
    /// Mutating methods take `base_rev`, support `dry_run`, and emit `project.changed`.
    pub mutates: bool,
    pub params: schemars::Schema,
    pub result: schemars::Schema,
}

/// A method that exists only at run time (a plugin's), as `describe` shows it.
#[derive(Debug, Clone)]
pub struct DynMethod {
    pub name: String,
    pub summary: String,
    pub mutates: bool,
    /// JSON Schema of the by-name params object.
    pub params: serde_json::Value,
    pub result: serde_json::Value,
    /// Plugin id that provides it.
    pub plugin: String,
}

/// Methods that come and go at run time (plugins). The registry asks it for
/// any method it does not have itself, and adds its methods to `describe`.
pub trait Extension: Send + Sync {
    fn methods(&self) -> Vec<DynMethod>;
    /// Run `method`; `None` when it is not one of this extension's.
    fn call(&self, method: &str, params: serde_json::Value) -> Option<Result<serde_json::Value, RpcError>>;
}

/// Method registry shared by every transport.
pub struct Registry {
    module: RpcModule<()>,
    specs: Vec<MethodSpec>,
    sealed: bool,
    extension: RwLock<Option<Arc<dyn Extension>>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self { module: RpcModule::new(()), specs: Vec::new(), sealed: false, extension: RwLock::new(None) }
    }

    /// Register a synchronous method. `P` is the by-name params object, `R` the result.
    /// Both must derive `JsonSchema`; that is what makes them appear in `describe`.
    pub fn method<P, R, F>(&mut self, name: &'static str, summary: &'static str, mutates: bool, f: F)
    where
        P: DeserializeOwned + JsonSchema + 'static,
        R: Serialize + JsonSchema + Clone + Send + 'static,
        F: Fn(P) -> Result<R, RpcError> + Send + Sync + Clone + 'static,
    {
        assert!(!self.sealed, "registry is sealed; register methods before seal()");
        self.specs.push(MethodSpec {
            name,
            summary,
            mutates,
            params: openrpc::schema_for::<P>(),
            result: openrpc::schema_for::<R>(),
        });
        self.module
            .register_method(name, move |params: Params<'_>, _, _| -> Result<R, ErrorObjectOwned> {
                let p: P = parse_params(&params)?;
                f(p).map_err(Into::into)
            })
            .unwrap_or_else(|e| panic!("duplicate method {name}: {e}"));
    }

    /// Register `system.describe` (which needs the full method list) and freeze the registry.
    pub fn seal(mut self, title: &str, version: &str) -> Arc<Self> {
        let doc = openrpc::document(title, version, &self.specs, true);
        let doc = Arc::new(serde_json::to_value(doc).expect("OpenRPC document serializes"));
        self.method("system.describe", "OpenRPC document for every method and plugin", false, {
            let doc = doc.clone();
            move |_: NoParams| Ok((*doc).clone())
        });
        self.sealed = true;
        Arc::new(self)
    }

    pub fn specs(&self) -> &[MethodSpec] {
        &self.specs
    }

    /// Whether a method is built in (not provided by an extension).
    pub fn has_method(&self, name: &str) -> bool {
        self.module.method(name).is_some()
    }

    /// Hand methods the registry does not have to `ext` (the plugin host).
    pub fn set_extension(&self, ext: Arc<dyn Extension>) {
        *self.extension.write().unwrap_or_else(|e| e.into_inner()) = Some(ext);
    }

    fn extension(&self) -> Option<Arc<dyn Extension>> {
        self.extension.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Dispatch one raw JSON-RPC request and return the raw JSON response.
    /// Malformed JSON gets a spec-compliant parse error response.
    pub async fn dispatch(&self, raw_request: &str) -> String {
        if let Some(ext) = self.extension()
            && let Some(resp) = self.dispatch_extension(&*ext, raw_request).await
        {
            return resp;
        }
        match self.module.raw_json_request(raw_request, 1).await {
            Ok((resp, _subscription)) => resp.get().to_owned(),
            Err(e) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") }
            })
            .to_string(),
        }
    }
}

impl Registry {
    /// Calls to methods only the extension has, and `system.describe` (which
    /// then lists the extension's methods too). `None`: a normal call.
    async fn dispatch_extension(&self, ext: &dyn Extension, raw: &str) -> Option<String> {
        #[derive(Deserialize)]
        struct Request {
            #[serde(default)]
            id: Option<serde_json::Value>,
            method: String,
            #[serde(default)]
            params: Option<serde_json::Value>,
        }
        let req: Request = serde_json::from_str(raw).ok()?;
        let id = req.id.unwrap_or(serde_json::Value::Null);
        if req.method == "system.describe" {
            let (raw_resp, _) = self.module.raw_json_request(raw, 1).await.ok()?;
            let mut resp: serde_json::Value = serde_json::from_str(raw_resp.get()).ok()?;
            if let Some(methods) = resp["result"]["methods"].as_array_mut() {
                methods.extend(ext.methods().iter().map(openrpc::dyn_method));
            }
            return Some(resp.to_string());
        }
        if self.has_method(&req.method) {
            return None;
        }
        let params = req.params.unwrap_or_else(|| serde_json::json!({}));
        let outcome = ext.call(&req.method, params)?;
        Some(match outcome {
            Ok(result) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => serde_json::json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": e.code.rpc_code(), "message": e.message, "data": e.data },
            }),
        }
        .to_string())
    }
}

/// Params type for methods that take none.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

fn parse_params<P: DeserializeOwned>(params: &Params<'_>) -> Result<P, ErrorObjectOwned> {
    let raw = params.as_str().unwrap_or("{}");
    serde_json::from_str(raw).map_err(|e| RpcError::invalid_params(e.to_string()).into())
}

/// Result of `system.version`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VersionInfo {
    pub engine: String,
    pub protocol: u32,
    pub schema: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Arc<Registry> {
        let mut r = Registry::new();
        r.method("system.version", "Engine and protocol versions", false, |_: NoParams| {
            Ok(VersionInfo {
                engine: "0.1.0".into(),
                protocol: PROTOCOL_VERSION,
                schema: ve_model::SCHEMA_VERSION.to_string(),
            })
        });
        r.seal("test engine", "0.1.0")
    }

    #[tokio::test]
    async fn dispatch_roundtrip() {
        let r = registry();
        let resp = r.dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"system.version"}"#).await;
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["protocol"], 1);
    }

    #[tokio::test]
    async fn bad_params_are_invalid_params() {
        let r = registry();
        let resp = r
            .dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"system.version","params":{"nope":1}}"#)
            .await;
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32602);
    }

    struct Echo;
    impl Extension for Echo {
        fn methods(&self) -> Vec<DynMethod> {
            vec![DynMethod {
                name: "echo.say".into(),
                summary: "Say it back".into(),
                mutates: false,
                params: serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}),
                result: serde_json::json!({"type": "object"}),
                plugin: "test.echo".into(),
            }]
        }
        fn call(&self, method: &str, params: serde_json::Value) -> Option<Result<serde_json::Value, RpcError>> {
            match method {
                "echo.say" => Some(Ok(params)),
                "echo.fail" => Some(Err(RpcError::new(ErrorCode::PluginError, "nope"))),
                _ => None,
            }
        }
    }

    #[tokio::test]
    async fn extension_methods_dispatch_and_describe() {
        let r = registry();
        r.set_extension(Arc::new(Echo));
        let resp: serde_json::Value =
            serde_json::from_str(&r.dispatch(r#"{"jsonrpc":"2.0","id":7,"method":"echo.say","params":{"text":"hi"}}"#).await).unwrap();
        assert_eq!(resp["id"], 7);
        assert_eq!(resp["result"]["text"], "hi");
        let resp: serde_json::Value = serde_json::from_str(&r.dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"echo.fail"}"#).await).unwrap();
        assert_eq!(resp["error"]["code"], -31002);
        // Built-ins still win, and unknown methods are still unknown.
        let resp: serde_json::Value = serde_json::from_str(&r.dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"system.version"}"#).await).unwrap();
        assert_eq!(resp["result"]["protocol"], 1);
        let resp: serde_json::Value = serde_json::from_str(&r.dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"nope.nope"}"#).await).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
        let resp: serde_json::Value = serde_json::from_str(&r.dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"system.describe"}"#).await).unwrap();
        let echo = resp["result"]["methods"].as_array().unwrap().iter().find(|m| m["name"] == "echo.say").unwrap().clone();
        assert_eq!(echo["x-plugin"], "test.echo");
        assert_eq!(echo["params"][0]["name"], "text");
    }

    #[tokio::test]
    async fn describe_lists_every_method() {
        let r = registry();
        let resp = r.dispatch(r#"{"jsonrpc":"2.0","id":1,"method":"system.describe"}"#).await;
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        let names: Vec<&str> =
            v["result"]["methods"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["system.version", "system.describe"]);
        assert_eq!(v["result"]["openrpc"], "1.3.2");
    }
}
