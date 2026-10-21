//! Error codes shared by the protocol and the CLI's exit codes.

use jsonrpsee::types::ErrorObjectOwned;
use serde_json::Value;

/// Engine error codes. JSON-RPC reserves -32768..-32000 for the spec's own
/// errors; engine-specific codes live just above that range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidParams,
    StaleRevision,
    RenderFailure,
    PluginError,
    Internal,
}

impl ErrorCode {
    pub fn rpc_code(self) -> i32 {
        match self {
            Self::InvalidParams => -32602,
            Self::Internal => -32603,
            Self::StaleRevision => -31000,
            Self::RenderFailure => -31001,
            Self::PluginError => -31002,
        }
    }

    /// Process exit code for the CLI (see ARCHITECTURE.md, "CLI conventions").
    pub fn exit_code(self) -> i32 {
        match self {
            Self::InvalidParams => 2,
            Self::StaleRevision => 3,
            Self::RenderFailure => 4,
            Self::PluginError => 5,
            Self::Internal => 1,
        }
    }

    pub fn from_rpc_code(code: i64) -> Self {
        match code {
            -32602 => Self::InvalidParams,
            -31000 => Self::StaleRevision,
            -31001 => Self::RenderFailure,
            -31002 => Self::PluginError,
            _ => Self::Internal,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }
    pub fn stale_revision(current_rev: u64) -> Self {
        Self {
            code: ErrorCode::StaleRevision,
            message: "stale_revision".into(),
            data: Some(serde_json::json!({ "current_rev": current_rev })),
        }
    }
}

impl From<RpcError> for ErrorObjectOwned {
    fn from(e: RpcError) -> Self {
        ErrorObjectOwned::owned(e.code.rpc_code(), e.message, e.data)
    }
}
