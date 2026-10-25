//! Plugin secrets (BYOK API keys) in the OS keychain: Windows Credential
//! Manager, macOS Keychain, the Secret Service on Linux. Never in the project.
//!
//! The engine's environment wins: a variable of the secret's name set where
//! the daemon runs (CI, a shell profile) is used as is.

use serde::Serialize;

const SERVICE: &str = "ve-plugins";

/// Where a secret would come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretSource {
    Environment,
    Keychain,
    Missing,
}

fn entry(plugin: &str, name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("{plugin}/{name}")).map_err(|e| format!("the OS keychain is not available: {e}"))
}

/// The keychain value, if one is stored.
pub fn keychain_get(plugin: &str, name: &str) -> Option<String> {
    entry(plugin, name).ok()?.get_password().ok()
}

pub fn set(plugin: &str, name: &str, value: &str) -> Result<(), String> {
    entry(plugin, name)?.set_password(value).map_err(|e| format!("saving {name} for {plugin}: {e}"))
}

/// Remove a stored secret. `Ok(false)` if there was none.
pub fn delete(plugin: &str, name: &str) -> Result<bool, String> {
    match entry(plugin, name)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("removing {name} for {plugin}: {e}")),
    }
}

pub fn source(plugin: &str, name: &str) -> SecretSource {
    if std::env::var_os(name).is_some_and(|v| !v.is_empty()) {
        SecretSource::Environment
    } else if keychain_get(plugin, name).is_some() {
        SecretSource::Keychain
    } else {
        SecretSource::Missing
    }
}
