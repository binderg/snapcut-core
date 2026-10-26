//! `fx.*`, `effect.*` and `transition.*`: the effects and transitions a
//! project can use, defining new ones, and putting them on clips and cuts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ve_model::Id;
use ve_ops::effects::{self, Align, EffectReport, NewTransition, TransitionReport};
use ve_protocol::RpcError;
use base64::Engine as _;
use ve_render::fx::{Fx, Kind, Manifest, Origin, Registry};

use crate::commit::{Committed, commit, mutation_params, parse_time};
use crate::project::OpenProject;

fn registry(p: &OpenProject) -> Registry {
    Registry::for_project(Some(p.root()))
}

/// Effects from installed packs (plugins with `contributes.effects`, in the
/// project's `plugins/` folder or the user's), and which plugin each folder
/// belongs to.
struct Packs {
    registry: Registry,
    plugin_of: HashMap<PathBuf, String>,
}

fn packs(p: &OpenProject) -> Packs {
    let engine = semver::Version::parse(crate::ENGINE_VERSION).expect("engine version is semver");
    let (plugins, _) = ve_plugin::discover(Some(p.root()), ve_plugin::user_dir().as_deref(), &engine);
    let mut dirs = Vec::new();
    let mut plugin_of = HashMap::new();
    for pl in &plugins {
        for d in pl.manifest.effect_dirs(&pl.dir) {
            plugin_of.insert(d.clone(), pl.manifest.id.clone());
            dirs.push(d);
        }
    }
    Packs { registry: Registry::packs(&dirs), plugin_of }
}

impl Packs {
    /// Pack effects the project does not already have (same `id@major`).
    fn unshadowed<'a>(&'a self, project: &'a Registry) -> impl Iterator<Item = &'a Arc<Fx>> + 'a {
        self.registry.list.iter().filter(|f| !project.list.iter().any(|x| x.reference() == f.reference()))
    }

    fn plugin(&self, f: &Fx) -> Option<String> {
        f.dir.as_ref().and_then(|d| self.plugin_of.get(d).cloned())
    }
}

/// The effect a call names: the project's own (built-ins and `effects/`)
/// first, so a project keeps the version it copied even when a pack has a
/// newer one, then the installed packs. A pack effect comes back with
/// `Origin::Pack`; [`with_fx`] copies it in before the edit commits.
fn resolve(p: &OpenProject, reference: &str, kind: Kind) -> Result<Arc<Fx>, RpcError> {
    let project = registry(p);
    if project.get(reference).is_some() {
        return project.require(reference, kind).map_err(invalid);
    }
    let packs = packs(p);
    let mut all = project.clone();
    all.list.extend(packs.unshadowed(&project).cloned());
    all.require(reference, kind).map_err(invalid)
}

/// Copy a pack effect into `<project>/effects/`, so the project renders
/// without the pack installed. Returns the new folder.
fn copy_in(p: &OpenProject, fx: &Fx) -> Result<PathBuf, RpcError> {
    let src = fx.dir.as_ref().ok_or_else(|| internal(format!("{} has no folder to copy", fx.manifest.id)))?;
    let effects = p.root().join("effects");
    let base = fx.manifest.id.replace(['/', '.'], "_");
    let major = fx.manifest.version.split('.').next().unwrap_or("0");
    let dest = std::iter::once(base.clone())
        .chain(std::iter::once(format!("{base}_v{major}")))
        .chain((2..).map(|n| format!("{base}_v{major}_{n}")))
        .map(|name| effects.join(name))
        .find(|d| !d.exists())
        .expect("an unused folder name");
    copy_dir(src, &dest).map_err(|e| {
        let _ = std::fs::remove_dir_all(&dest);
        internal(format!("copying {} into the project: {e}", src.display()))
    })?;
    Registry::forget(p.root());
    Ok(dest)
}

fn copy_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

/// Run an edit that uses `fx`, copying it in from its pack first (not on a
/// dry run). The copy is removed again if the edit fails.
fn with_fx<T>(p: &mut OpenProject, fx: &Fx, dry_run: bool, edit: impl FnOnce(&mut OpenProject) -> Result<T, RpcError>) -> Result<T, RpcError> {
    let copied = if fx.origin == Origin::Pack && !dry_run { Some(copy_in(p, fx)?) } else { None };
    let r = edit(p);
    if r.is_err()
        && let Some(d) = copied
    {
        let _ = std::fs::remove_dir_all(d);
        Registry::forget(p.root());
    }
    r
}

fn internal(m: impl Into<String>) -> RpcError {
    RpcError::new(ve_protocol::ErrorCode::Internal, m)
}

fn invalid(e: impl ToString) -> RpcError {
    RpcError::invalid_params(e.to_string())
}

// --- fx.list ---

/// One effect or transition `fx.list` shows.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FxInfo {
    /// `core.color/desaturate`.
    pub id: String,
    /// What clips store: `id@major`.
    pub reference: String,
    pub version: String,
    /// `effect` or `transition`.
    pub kind: String,
    pub runtime: String,
    pub name: String,
    pub description: String,
    /// `builtin`, `project` or `pack` (installed, not yet in this project:
    /// using it copies it into the project's `effects/`).
    pub origin: String,
    /// The plugin a `pack` effect comes from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// JSON Schema of the params (`x-animatable` marks params that take keyframes).
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FxList {
    pub effects: Vec<FxInfo>,
    /// Project and pack effects that failed to load, with the reason.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

fn info(f: &Fx, plugin: Option<String>) -> FxInfo {
    FxInfo {
        id: f.manifest.id.clone(),
        reference: f.reference(),
        version: f.manifest.version.clone(),
        kind: ve_render::fx::kind_word(f.kind()).into(),
        runtime: f.manifest.runtime.clone(),
        name: f.manifest.name.clone(),
        description: f.manifest.description.clone(),
        origin: match f.origin {
            Origin::Builtin => "builtin".into(),
            Origin::Project => "project".into(),
            Origin::Pack => "pack".into(),
        },
        plugin,
        params: f.manifest.params.clone(),
    }
}

pub fn list(p: &OpenProject) -> FxList {
    let r = registry(p);
    let packs = packs(p);
    let mut effects: Vec<FxInfo> = r.list.iter().map(|f| info(f, None)).collect();
    effects.extend(packs.unshadowed(&r).map(|f| info(f, packs.plugin(f))));
    let mut problems = r.problems;
    problems.extend(packs.registry.problems.iter().cloned());
    FxList { effects, problems }
}

// --- fx.define ---

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefineParams {
    /// The `effect.json` manifest. `id` must start with `project.`; `runtime` must be `wgsl`.
    pub manifest: Value,
    /// The WGSL source: `fn effect(uv: vec2<f32>) -> vec4<f32>` or `fn transition(uv: vec2<f32>) -> vec4<f32>`.
    pub source: String,
    /// Images for `"format": "image"` params: file name (PNG or JPEG) to base64 bytes.
    /// Saved next to the shader.
    #[serde(default)]
    pub files: std::collections::BTreeMap<String, String>,
    /// Validate and test-render only; write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Defined {
    pub effect: FxInfo,
    /// Folder written (absent on a dry run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// GPU adapter the test render ran on.
    pub tested_on: String,
}

/// Validate a new project effect (naga, then a timed GPU test render) and
/// save it to `<project>/effects/<name>/`.
pub fn define(p: &mut OpenProject, a: DefineParams) -> Result<Defined, RpcError> {
    let m: Manifest = serde_json::from_value(a.manifest).map_err(|e| invalid(format!("manifest: {e}")))?;
    if !m.id.starts_with("project.") {
        return Err(invalid(format!("id `{}`: project effects use ids starting with `project.`", m.id)));
    }
    let existing = registry(p);
    if existing.list.iter().any(|f| f.manifest.id == m.id && f.origin == Origin::Builtin) {
        return Err(invalid(format!("`{}` is a built-in id", m.id)));
    }
    let mut images = std::collections::BTreeMap::new();
    let mut raw = Vec::new();
    for (name, b64) in &a.files {
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| invalid(format!("files.{name}: not base64: {e}")))?;
        let img = ve_render::fx::decode_image(name, &bytes).map_err(invalid)?;
        images.insert(name.clone(), std::sync::Arc::new(img));
        raw.push((name.clone(), bytes));
    }
    let fx = Fx::with_images(m, a.source, Origin::Project, None, images).map_err(invalid)?;
    let gpu = ve_render::gpu::Gpu::shared().map_err(|e| RpcError::new(ve_protocol::ErrorCode::RenderFailure, e.to_string()))?;
    gpu.test_fx(&fx).map_err(invalid)?;
    let name = fx.manifest.id.trim_start_matches("project.").replace(['/', '.'], "_");
    let dir = p.root().join("effects").join(&name);
    if !a.dry_run {
        std::fs::create_dir_all(&dir).map_err(|e| internal(format!("{}: {e}", dir.display())))?;
        let json = serde_json::to_string_pretty(&fx.manifest).unwrap_or_default() + "\n";
        std::fs::write(dir.join("effect.json"), json).map_err(|e| internal(e.to_string()))?;
        std::fs::write(dir.join(&fx.manifest.shader), &fx.source).map_err(|e| internal(e.to_string()))?;
        for (name, bytes) in &raw {
            std::fs::write(dir.join(name), bytes).map_err(|e| internal(e.to_string()))?;
        }
        Registry::forget(p.root());
    }
    Ok(Defined { effect: info(&fx, None), path: (!a.dry_run).then(|| dir.display().to_string()), tested_on: gpu.adapter.clone() })
}

// --- effect.* ---

/// A keyframe as callers send it.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeArg {
    /// Time from the clip's start (`0s`, `1.5s`, `f12`, `00:00:02:00`).
    pub at: String,
    pub value: Value,
    /// Shape of the segment to the next key: `linear` (default), `hold`
    /// (keep this value until the next key), `ease`, `ease-in`, `ease-out`,
    /// `ease-in-out`, or `cubic-bezier(x1, y1, x2, y2)` as in CSS.
    #[serde(default)]
    pub ease: Option<String>,
    /// Graph-editor handle into this key: `{"x": fraction of the segment, "y": offset from this value}`.
    #[serde(default, rename = "in")]
    pub r#in: Option<ve_model::curve::Handle>,
    /// Graph-editor handle out of this key, same shape. A segment uses an ease or handles, not both.
    #[serde(default)]
    pub out: Option<ve_model::curve::Handle>,
}

pub(crate) fn keyframes(p: &OpenProject, keys: &std::collections::BTreeMap<String, Vec<KeyframeArg>>) -> Result<IndexMap<String, Vec<ve_model::meta::Keyframe>>, RpcError> {
    let rate = p.timeline.output_rate();
    keys.iter()
        .map(|(name, list)| {
            let mut ks = list
                .iter()
                .map(|k| {
                    let t = parse_time("at", &k.at, rate)?;
                    let mut kf = effects::keyframe(t, k.value.clone(), k.ease.as_ref().map(|e| e.trim().to_owned()), rate);
                    kf.r#in = k.r#in.clone();
                    kf.out = k.out.clone();
                    Ok(kf)
                })
                .collect::<Result<Vec<_>, RpcError>>()?;
            ks.sort_by_key(|k| k.t);
            ve_model::curve::check(&ks).map_err(|e| invalid(format!("keyframes for `{name}`: {e}")))?;
            Ok((name.clone(), ks))
        })
        .collect()
}

mutation_params! {
    /// `effect.add`: put an effect on a video clip.
    pub struct EffectAddParams {
        pub clip: Id,
        /// Effect id (`core.color/exposure`) or `id@major`. `fx.list` lists them.
        pub effect: String,
        /// Param values; unset params use their defaults.
        #[serde(default)]
        pub params: Map<String, Value>,
        /// Position in the clip's stack (0 runs first). Default: last.
        #[serde(default)]
        pub index: Option<usize>,
    }
}

pub fn effect_add(p: &mut OpenProject, a: EffectAddParams) -> Result<Committed<EffectReport>, RpcError> {
    let fx = resolve(p, &a.effect, Kind::Effect)?;
    fx.check_params(&a.params).map_err(invalid)?;
    with_fx(p, &fx, a.dry_run, |p| {
        commit(p, a.mutation(), "effect.add", |tl| effects::add(tl, &a.clip, &fx.reference(), a.params.clone(), a.index).map_err(invalid))
    })
}

mutation_params! {
    /// `effect.set`: change an effect's params, keyframes or place in the stack.
    pub struct EffectSetParams {
        /// The effect instance (`fx_...`).
        pub id: Id,
        /// Params to change; `null` resets one to its default.
        #[serde(default)]
        pub params: Option<Map<String, Value>>,
        /// Param -> keyframes, replacing that param's keyframes; `[]` removes them.
        #[serde(default)]
        pub keyframes: Option<std::collections::BTreeMap<String, Vec<KeyframeArg>>>,
        /// New position in the clip's stack.
        #[serde(default)]
        pub index: Option<usize>,
    }
}

pub fn effect_set(p: &mut OpenProject, a: EffectSetParams) -> Result<Committed<EffectReport>, RpcError> {
    let inst = effects::get(&p.timeline, &a.id).ok_or_else(|| invalid(format!("no effect `{}` on any clip", a.id)))?.clone();
    let fx = registry(p).require(&inst.effect, Kind::Effect).map_err(invalid)?;
    if let Some(params) = &a.params {
        let set: Map<String, Value> = params.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
        fx.check_params(&set).map_err(invalid)?;
        for k in params.keys() {
            if !fx.params.iter().any(|x| &x.name == k) {
                return Err(invalid(format!("{} has no param `{k}`", fx.manifest.id)));
            }
        }
    }
    let keys = a.keyframes.as_ref().map(|k| keyframes(p, k)).transpose()?;
    if let Some(k) = &keys {
        fx.check_keyframes(k).map_err(invalid)?;
    }
    commit(p, a.mutation(), "effect.set", |tl| effects::set(tl, &a.id, a.params.as_ref(), keys.as_ref(), a.index).map_err(invalid))
}

mutation_params! {
    /// `effect.remove`.
    pub struct EffectRemoveParams {
        pub id: Id,
    }
}

pub fn effect_remove(p: &mut OpenProject, a: EffectRemoveParams) -> Result<Committed<EffectReport>, RpcError> {
    commit(p, a.mutation(), "effect.remove", |tl| effects::remove(tl, &a.id).map_err(invalid))
}

// --- transition.* ---

fn default_transition() -> String {
    "core.transition/dissolve".into()
}

fn default_duration() -> String {
    "0.5s".into()
}

mutation_params! {
    /// `transition.add`: a transition on a cut between two touching clips on a video track.
    pub struct TransitionAddParams {
        /// The outgoing clip: the transition goes on the cut at its end.
        #[serde(default)]
        pub clip: Option<Id>,
        /// Or the time of the cut.
        #[serde(default)]
        pub at: Option<String>,
        /// With `at`: the track (default: the first video track with a cut there).
        #[serde(default)]
        pub track: Option<String>,
        /// Transition id; `fx.list` lists them. Default `core.transition/dissolve`.
        #[serde(default = "default_transition")]
        pub transition: String,
        /// Default `0.5s`.
        #[serde(default = "default_duration")]
        pub duration: String,
        /// `center` (default; half before the cut), `start` (begins at the cut) or `end` (ends at it).
        #[serde(default)]
        pub align: Align,
        #[serde(default)]
        pub params: Map<String, Value>,
    }
}

pub fn transition_add(p: &mut OpenProject, a: TransitionAddParams) -> Result<Committed<TransitionReport>, RpcError> {
    let fx = resolve(p, &a.transition, Kind::Transition)?;
    fx.check_params(&a.params).map_err(invalid)?;
    let rate = p.timeline.output_rate();
    let at = a.at.as_ref().map(|s| parse_time("at", s, rate)).transpose()?;
    if a.clip.is_none() && at.is_none() {
        return Err(invalid("give `clip` (the outgoing clip) or `at` (the time of a cut)"));
    }
    let new = NewTransition {
        clip: a.clip.clone(),
        at,
        track: a.track.clone(),
        duration: parse_time("duration", &a.duration, rate)?,
        align: a.align,
        transition: fx.reference(),
        params: a.params.clone(),
    };
    with_fx(p, &fx, a.dry_run, |p| commit(p, a.mutation(), "transition.add", |tl| effects::add_transition(tl, &new).map_err(invalid)))
}

mutation_params! {
    /// `transition.set`: change a transition's length, alignment, kind or params.
    pub struct TransitionSetParams {
        /// The transition (`tr_...`).
        pub id: Id,
        #[serde(default)]
        pub duration: Option<String>,
        #[serde(default)]
        pub align: Option<Align>,
        /// Switch to another transition (`core.transition/wipe`).
        #[serde(default)]
        pub transition: Option<String>,
        /// Params to change; `null` resets one to its default.
        #[serde(default)]
        pub params: Option<Map<String, Value>>,
    }
}

pub fn transition_set(p: &mut OpenProject, a: TransitionSetParams) -> Result<Committed<TransitionReport>, RpcError> {
    let current = effects::transition_meta(&p.timeline, &a.id).ok_or_else(|| invalid(format!("no transition `{}`", a.id)))?.clone();
    let fx = resolve(p, a.transition.as_deref().unwrap_or(&current.transition), Kind::Transition)?;
    if let Some(params) = &a.params {
        let set: Map<String, Value> = params.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
        fx.check_params(&set).map_err(invalid)?;
    }
    let rate = p.timeline.output_rate();
    let duration = a.duration.as_ref().map(|d| parse_time("duration", d, rate)).transpose()?;
    let kind = a.transition.as_ref().map(|_| fx.reference());
    with_fx(p, &fx, a.dry_run, |p| {
        commit(p, a.mutation(), "transition.set", |tl| {
            effects::set_transition(tl, &a.id, duration, a.align, kind.as_deref(), a.params.as_ref()).map_err(invalid)
        })
    })
}

mutation_params! {
    /// `transition.remove`: back to a plain cut.
    pub struct TransitionRemoveParams {
        pub id: Id,
    }
}

pub fn transition_remove(p: &mut OpenProject, a: TransitionRemoveParams) -> Result<Committed<TransitionReport>, RpcError> {
    commit(p, a.mutation(), "transition.remove", |tl| effects::remove_transition(tl, &a.id).map_err(invalid))
}
