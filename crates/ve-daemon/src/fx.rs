//! `fx.*`, `effect.*` and `transition.*`: the effects and transitions a
//! project can use, defining new ones, and putting them on clips and cuts.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ve_model::Id;
use ve_ops::effects::{self, Align, EffectReport, NewTransition, TransitionReport};
use ve_protocol::RpcError;
use ve_render::fx::{Fx, Kind, Manifest, Origin, Registry};

use crate::commit::{Committed, commit, mutation_params, parse_time};
use crate::project::OpenProject;

fn registry(p: &OpenProject) -> Registry {
    Registry::for_project(Some(p.root()))
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
    /// `builtin` or `project`.
    pub origin: String,
    /// JSON Schema of the params (`x-animatable` marks params that take keyframes).
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FxList {
    pub effects: Vec<FxInfo>,
    /// Project effects that failed to load, with the reason.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

fn info(f: &Fx) -> FxInfo {
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
        },
        params: f.manifest.params.clone(),
    }
}

pub fn list(p: &OpenProject) -> FxList {
    let r = registry(p);
    FxList { effects: r.list.iter().map(|f| info(f)).collect(), problems: r.problems }
}

// --- fx.define ---

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DefineParams {
    /// The `effect.json` manifest. `id` must start with `project.`; `runtime` must be `wgsl`.
    pub manifest: Value,
    /// The WGSL source: `fn effect(uv: vec2<f32>) -> vec4<f32>` or `fn transition(uv: vec2<f32>) -> vec4<f32>`.
    pub source: String,
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
    let fx = Fx::new(m, a.source, Origin::Project, None).map_err(invalid)?;
    let gpu = ve_render::gpu::Gpu::shared().map_err(|e| RpcError::new(ve_protocol::ErrorCode::RenderFailure, e.to_string()))?;
    gpu.test_fx(&fx).map_err(invalid)?;
    let name = fx.manifest.id.trim_start_matches("project.").replace(['/', '.'], "_");
    let dir = p.root().join("effects").join(&name);
    if !a.dry_run {
        std::fs::create_dir_all(&dir).map_err(|e| internal(format!("{}: {e}", dir.display())))?;
        let json = serde_json::to_string_pretty(&fx.manifest).unwrap_or_default() + "\n";
        std::fs::write(dir.join("effect.json"), json).map_err(|e| internal(e.to_string()))?;
        std::fs::write(dir.join(&fx.manifest.shader), &fx.source).map_err(|e| internal(e.to_string()))?;
    }
    Ok(Defined { effect: info(&fx), path: (!a.dry_run).then(|| dir.display().to_string()), tested_on: gpu.adapter.clone() })
}

// --- effect.* ---

/// A keyframe as callers send it.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyframeArg {
    /// Time from the clip's start (`0s`, `1.5s`, `f12`, `00:00:02:00`).
    pub at: String,
    pub value: Value,
    /// `linear` (default) or `hold` (keep this value until the next key).
    #[serde(default)]
    pub ease: Option<String>,
}

fn keyframes(p: &OpenProject, keys: &std::collections::BTreeMap<String, Vec<KeyframeArg>>) -> Result<IndexMap<String, Vec<ve_model::meta::Keyframe>>, RpcError> {
    let rate = p.timeline.output_rate();
    keys.iter()
        .map(|(name, list)| {
            let ks = list
                .iter()
                .map(|k| {
                    if let Some(e) = &k.ease
                        && e != "linear"
                        && e != "hold"
                    {
                        return Err(invalid(format!("ease `{e}`: use linear or hold")));
                    }
                    let t = parse_time("at", &k.at, rate)?;
                    Ok(effects::keyframe(t, k.value.clone(), k.ease.clone(), rate))
                })
                .collect::<Result<Vec<_>, RpcError>>()?;
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
    let fx = registry(p).require(&a.effect, Kind::Effect).map_err(invalid)?;
    fx.check_params(&a.params).map_err(invalid)?;
    commit(p, a.mutation(), "effect.add", |tl| effects::add(tl, &a.clip, &fx.reference(), a.params.clone(), a.index).map_err(invalid))
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
    let fx = registry(p).require(&a.transition, Kind::Transition).map_err(invalid)?;
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
    commit(p, a.mutation(), "transition.add", |tl| effects::add_transition(tl, &new).map_err(invalid))
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
    let reg = registry(p);
    let fx = reg.require(a.transition.as_deref().unwrap_or(&current.transition), Kind::Transition).map_err(invalid)?;
    if let Some(params) = &a.params {
        let set: Map<String, Value> = params.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
        fx.check_params(&set).map_err(invalid)?;
    }
    let rate = p.timeline.output_rate();
    let duration = a.duration.as_ref().map(|d| parse_time("duration", d, rate)).transpose()?;
    let kind = a.transition.as_ref().map(|_| fx.reference());
    commit(p, a.mutation(), "transition.set", |tl| {
        effects::set_transition(tl, &a.id, duration, a.align, kind.as_deref(), a.params.as_ref()).map_err(invalid)
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
