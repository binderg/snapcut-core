//! `plugin.json`: what a plugin is and what it adds.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PluginError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Unique id, lowercase: `deepgram`, `community.jumpcut`.
    pub id: String,
    pub version: semver::Version,
    /// Engine versions this plugin works with, e.g. `">=0.1.0-rc.1, <0.3"`.
    pub engine: semver::VersionReq,
    /// Protocol major version it speaks (`"1"`).
    pub protocol: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub contributes: Contributes,
    /// How to start the plugin's process. Absent for plugins with no code.
    #[serde(default)]
    pub run: Option<Run>,
    /// Keys the plugin needs (API keys). Read from the OS keychain
    /// (`ve plugin secret set`) or the engine's environment, and handed to
    /// the process as environment variables of the same name.
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub requires: Requires,
    /// A SKILL.md teaching agents when and how to use the plugin.
    #[serde(default)]
    pub skill: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contributes {
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    /// Effect and transition folders (an effect pack), relative to the
    /// plugin folder: each is one effect (it holds an `effect.json`) or a
    /// folder of them. They need no process; the engine copies one into the
    /// project's `effects/` the first time the project uses it.
    #[serde(default)]
    pub effects: Vec<String>,
}

/// One protocol method the plugin adds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSpec {
    /// `namespace.name`. The namespace must not be one the engine uses.
    pub method: String,
    pub summary: String,
    /// JSON Schema of the params object: inline, or a path relative to the plugin folder.
    #[serde(default)]
    pub params: Option<Value>,
    /// JSON Schema of the result: inline or a path.
    #[serde(default)]
    pub result: Option<Value>,
    /// Edits the project (through the engine's methods). The engine groups
    /// the plugin's edits during one call into one undo step.
    #[serde(default)]
    pub mutates: bool,
    /// Slow work: callers may pass `background: true` to run it as a job,
    /// and the CLI always does.
    #[serde(default)]
    pub job: bool,
    /// A `ve <name>` command for this method.
    #[serde(default)]
    pub cli: Option<CliSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliSpec {
    /// The subcommand: `ve transcribe ...`.
    pub name: String,
    /// Params filled from positional arguments, in order. Every other param
    /// is a `--flag` (snake_case params also answer to kebab-case flags).
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    /// Program to start. A relative path with a `/` resolves against the
    /// plugin folder; a bare name is looked up on PATH.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Per-OS replacements (`python` on Windows, `python3` elsewhere).
    #[serde(default)]
    pub windows: Option<RunOverride>,
    #[serde(default)]
    pub macos: Option<RunOverride>,
    #[serde(default)]
    pub linux: Option<RunOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunOverride {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

impl Run {
    /// Command and args for this OS.
    pub fn for_this_os(&self) -> (String, Vec<String>) {
        let o = if cfg!(windows) {
            self.windows.as_ref()
        } else if cfg!(target_os = "macos") {
            self.macos.as_ref()
        } else {
            self.linux.as_ref()
        };
        let command = o.and_then(|o| o.command.clone()).unwrap_or_else(|| self.command.clone());
        let args = o.and_then(|o| o.args.clone()).unwrap_or_else(|| self.args.clone());
        (command, args)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    #[serde(default)]
    pub gpu: bool,
    /// Things the plugin downloads on first use (models), for the user to know.
    #[serde(default)]
    pub downloads: Vec<Value>,
}

impl Manifest {
    /// Whether this engine version is in the plugin's range. A prerelease
    /// engine (`0.1.0-rc.1`) also counts as its release (`0.1.0`), so ranges
    /// like `*` or `>=0.1` work on release candidates too.
    pub fn supports(&self, engine: &semver::Version) -> bool {
        let mut release = engine.clone();
        release.pre = semver::Prerelease::EMPTY;
        self.engine.matches(engine) || self.engine.matches(&release)
    }

    /// Read and check `<dir>/plugin.json`. Schema files named by commands
    /// are read here too, so a broken plugin fails at load, not at call time.
    pub fn load(dir: &Path) -> Result<Self, PluginError> {
        let path = dir.join("plugin.json");
        let text = std::fs::read_to_string(&path).map_err(|e| PluginError::Io { path: path.clone(), source: e })?;
        let mut m: Manifest =
            serde_json::from_str(&text).map_err(|e| PluginError::Manifest { path: path.clone(), message: e.to_string() })?;
        let bad = |message: String| PluginError::Manifest { path: path.clone(), message };
        if !valid_id(&m.id) {
            return Err(bad(format!("id `{}`: use lowercase letters, digits, `.`, `-` and `_`", m.id)));
        }
        for c in &mut m.contributes.commands {
            let Some((ns, name)) = c.method.split_once('.') else {
                return Err(bad(format!("method `{}`: name it `namespace.name`", c.method)));
            };
            if !valid_id(ns) || name.is_empty() || !name.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || "._".contains(ch)) {
                return Err(bad(format!("method `{}`: use lowercase letters, digits, `.` and `_`", c.method)));
            }
            for (field, schema) in [("params", &mut c.params), ("result", &mut c.result)] {
                if let Some(Value::String(file)) = schema {
                    let p = dir.join(file.as_str());
                    let text = std::fs::read_to_string(&p).map_err(|e| PluginError::Io { path: p.clone(), source: e })?;
                    *schema = Some(
                        serde_json::from_str(&text)
                            .map_err(|e| bad(format!("{} {field} schema {}: {e}", c.method, p.display())))?,
                    );
                }
                if let Some(s) = schema.as_ref()
                    && !s.is_object()
                {
                    return Err(bad(format!("{} {field}: a JSON Schema object or a path to one", c.method)));
                }
            }
        }
        for e in &m.contributes.effects {
            let rel = Path::new(e);
            if e.is_empty() || rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir)) {
                return Err(bad(format!("effects `{e}`: give a folder inside the plugin folder")));
            }
            if effect_dirs_in(&dir.join(rel)).is_empty() {
                return Err(bad(format!("effects `{e}`: no effect.json in {} or the folders inside it", dir.join(rel).display())));
            }
        }
        if m.run.is_none() && !m.contributes.commands.is_empty() {
            return Err(bad("commands need a process, but the manifest has no `run`".into()));
        }
        for s in &m.secrets {
            if s.is_empty() || !s.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_') {
                return Err(bad(format!("secret `{s}`: name it like an environment variable (`DEEPGRAM_API_KEY`)")));
            }
        }
        Ok(m)
    }

    /// Every effect folder the plugin contributes, in manifest order (each
    /// holds an `effect.json`).
    pub fn effect_dirs(&self, dir: &Path) -> Vec<PathBuf> {
        self.contributes.effects.iter().flat_map(|e| effect_dirs_in(&dir.join(e))).collect()
    }

    /// The SKILL.md path, if the plugin has one.
    pub fn skill_path(&self, dir: &Path) -> Option<PathBuf> {
        self.skill.as_ref().map(|s| dir.join(s))
    }
}

/// `dir` itself when it holds an `effect.json`, else its subfolders that do (sorted).
fn effect_dirs_in(dir: &Path) -> Vec<PathBuf> {
    if dir.join("effect.json").is_file() {
        return vec![dir.to_owned()];
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join("effect.json").is_file()).collect();
    dirs.sort();
    dirs
}

impl CommandSpec {
    /// The params schema, `{"type": "object"}` when the manifest gives none.
    pub fn params_schema(&self) -> Value {
        self.params.clone().unwrap_or_else(|| serde_json::json!({ "type": "object" }))
    }

    pub fn result_schema(&self) -> Value {
        self.result.clone().unwrap_or_else(|| serde_json::json!({ "type": "object" }))
    }
}

fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
        && s.starts_with(|c: char| c.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, manifest: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("plugin.json"), manifest).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join("ve-plugin-tests").join(name);
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn loads_schema_files_and_checks_names() {
        let d = scratch("manifest-ok");
        write(
            &d,
            r#"{"id": "acme.stt", "version": "0.1.0", "engine": ">=0.1.0-rc.1", "protocol": "1", "name": "STT",
                "contributes": {"commands": [{"method": "acme.transcribe", "summary": "Transcribe", "params": "p.json",
                  "job": true, "cli": {"name": "transcribe", "args": ["asset"]}}]},
                "run": {"command": "python3", "args": ["main.py"], "windows": {"command": "python"}},
                "secrets": ["ACME_KEY"]}"#,
        );
        std::fs::write(d.join("p.json"), r#"{"type": "object", "properties": {"asset": {"type": "string"}}}"#).unwrap();
        let m = Manifest::load(&d).unwrap();
        let c = &m.contributes.commands[0];
        assert_eq!(c.params_schema()["properties"]["asset"]["type"], "string");
        let (cmd, args) = m.run.as_ref().unwrap().for_this_os();
        assert_eq!(cmd, if cfg!(windows) { "python" } else { "python3" });
        assert_eq!(args, ["main.py"]);
        assert!(m.supports(&semver::Version::parse("0.1.0").unwrap()));
        let star: semver::VersionReq = "*".parse().unwrap();
        let m2 = Manifest { engine: star, ..m.clone() };
        assert!(m2.supports(&semver::Version::parse("0.1.0-rc.1").unwrap()));
    }

    #[test]
    fn rejects_bad_names() {
        let base = |extra: &str| {
            format!(r#"{{"id": "acme", "version": "0.1.0", "engine": "*", "protocol": "1", "name": "x", "run": {{"command": "x"}}, {extra}}}"#)
        };
        for (i, bad) in [
            r#""contributes": {"commands": [{"method": "transcribe", "summary": "s"}]}"#,
            r#""contributes": {"commands": [{"method": "Acme.Go", "summary": "s"}]}"#,
            r#""secrets": ["lower"]"#,
            r#""unknown": 1"#,
        ]
        .iter()
        .enumerate()
        {
            let d = scratch(&format!("manifest-bad-{i}"));
            write(&d, &base(bad));
            assert!(Manifest::load(&d).is_err(), "{bad}");
        }
    }
    #[test]
    fn effect_packs_need_no_process() {
        let d = scratch("manifest-pack");
        for e in ["looks/grain", "looks/vhs", "single"] {
            std::fs::create_dir_all(d.join(e)).unwrap();
            std::fs::write(d.join(e).join("effect.json"), "{}").unwrap();
        }
        let pack = |effects: &str| {
            format!(r#"{{"id": "acme.looks", "version": "1.0.0", "engine": "*", "protocol": "1", "name": "Looks", "contributes": {{"effects": {effects}}}}}"#)
        };
        write(&d, &pack(r#"["looks", "single"]"#));
        let m = Manifest::load(&d).unwrap();
        assert!(m.run.is_none());
        let names: Vec<String> = m.effect_dirs(&d).iter().map(|p| p.strip_prefix(&d).unwrap().to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(names, ["looks/grain", "looks/vhs", "single"]);
        for bad in [r#"["../x"]"#, r#"["missing"]"#, r#"[""]"#] {
            write(&d, &pack(bad));
            assert!(Manifest::load(&d).is_err(), "{bad} should be refused");
        }
    }
}
