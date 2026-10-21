//! The one protocol every client speaks: JSON-RPC 2.0, self-described by OpenRPC.
//!
//! Methods are registered once in a [`Registry`]. Every transport (CLI, local
//! socket / named pipe, WebSocket, plugin stdio) hands raw JSON to
//! [`Registry::dispatch`], so there is exactly one code path per method and the
//! OpenRPC document is generated from the same registrations, so it cannot drift.

mod error;
mod openrpc;

use std::sync::Arc;

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

/// Method registry shared by every transport.
pub struct Registry {
    module: RpcModule<()>,
    specs: Vec<MethodSpec>,
    sealed: bool,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Self { module: RpcModule::new(()), specs: Vec::new(), sealed: false }
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

    /// Dispatch one raw JSON-RPC request and return the raw JSON response.
    /// Malformed JSON gets a spec-compliant parse error response.
    pub async fn dispatch(&self, raw_request: &str) -> String {
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
