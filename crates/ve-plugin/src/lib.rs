//! Plugins.
//!
//! A plugin is a folder with `plugin.json` ([`manifest`]). Process plugins
//! are started on first use and spoken to over stdio as newline-delimited
//! JSON-RPC ([`process`]); they call back into the engine as actor
//! `plugin:<id>`. Plugins are found in the project's `plugins/` folder, then
//! the user's plugin folder ([`user_dir`]). Secrets (BYOK keys) come from the
//! OS keychain at spawn time, never from the project file ([`secrets`]).

pub mod host;
pub mod manifest;
pub mod process;
pub mod secrets;

use std::path::{Path, PathBuf};

pub use host::{Config, Host, Issue, Plugin, Source};
pub use manifest::Manifest;
pub use process::CallHooks;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path}: {message}")]
    Manifest { path: PathBuf, message: String },
    #[error("{0}")]
    Other(String),
}

/// The user's plugin folder: `VE_PLUGINS_DIR`, else `<OS data dir>/ve/plugins`.
pub fn user_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("VE_PLUGINS_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d));
    }
    let base = etcetera::choose_base_strategy().ok()?;
    Some(etcetera::BaseStrategy::data_dir(&base).join("ve").join("plugins"))
}

/// Copy a plugin folder into the user's plugin folder (replacing an older
/// copy of the same plugin). Returns where it went.
pub fn install(src: &Path) -> Result<(Manifest, PathBuf), PluginError> {
    let manifest = Manifest::load(src)?;
    let root = user_dir().ok_or_else(|| PluginError::Other("no user data folder on this system; set VE_PLUGINS_DIR".into()))?;
    let dest = root.join(&manifest.id);
    let same = std::fs::canonicalize(src).ok().zip(std::fs::canonicalize(&dest).ok()).is_some_and(|(a, b)| a == b);
    if same {
        return Ok((manifest, dest));
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| PluginError::Io { path: dest.clone(), source: e })?;
    }
    copy_dir(src, &dest)?;
    Ok((manifest, dest))
}

fn copy_dir(src: &Path, dest: &Path) -> Result<(), PluginError> {
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| PluginError::Io { path, source }
    };
    std::fs::create_dir_all(dest).map_err(io(dest))?;
    for entry in std::fs::read_dir(src).map_err(io(src))? {
        let entry = entry.map_err(io(src))?;
        let name = entry.file_name();
        if [".git", "__pycache__", "node_modules", ".venv"].iter().any(|n| name == *n) {
            continue;
        }
        let (from, to) = (entry.path(), dest.join(&name));
        if entry.file_type().map_err(io(&from))?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(io(&from))?;
        }
    }
    Ok(())
}
