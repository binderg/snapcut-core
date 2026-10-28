//! Effects and transitions: manifests, params, the WGSL wrapper and validation.
//!
//! An effect or transition is a manifest (`effect.json`) plus code. The
//! manifest names a `runtime`; only `wgsl` exists so far (ARCHITECTURE.md,
//! "Effects and transitions"). A `wgsl` author writes one function:
//!
//! ```wgsl
//! fn effect(uv: vec2<f32>) -> vec4<f32>      // kind "effect": src(uv) is the clip
//! fn transition(uv: vec2<f32>) -> vec4<f32>  // kind "transition": outgoing(uv), incoming(uv)
//! ```
//!
//! (`from` is a reserved word in WGSL, hence `outgoing`/`incoming`.)
//!
//! The engine wraps it into a full shader with `src`/`outgoing`/`incoming` (linear
//! light, premultiplied alpha, `uv` 0..1 from the top left), `u.time`
//! (seconds since the clip or transition starts), `u.progress` (0..1 across
//! it), `u.resolution` (pixels), and `p.<param>` for each manifest param.
//!
//! Params are JSON Schema properties. Supported: `number` and `integer` (as
//! `f32`), `boolean` (as `f32`, 0 or 1), color arrays (`"format":
//! "color"`, 3 or 4 numbers 0..1 in sRGB; the shader gets linear RGB as a
//! `vec4<f32>` with straight alpha), vectors (an array of 2 to 4 numbers
//! with `minItems` = `maxItems`; `vec2<f32>` to `vec4<f32>`), and images
//! (`"type": "string", "format": "image"`: a PNG or JPEG in the effect's
//! folder, sampled with a function named after the param, e.g. `luma(uv)`).
//! `x-animatable: true` allows keyframes (not on images).
//!
//! Built-ins live in `plugins/core/effects/` and are compiled in. Project
//! effects live in `<project>/effects/<name>/effect.json`.

use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ve_model::meta::Keyframe;
use ve_model::time::Q;

/// A manifest or shader that cannot be used. The message is written for
/// the person or agent who wrote the effect, with line numbers in their file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct FxError(pub String);

fn err(m: impl Into<String>) -> FxError {
    FxError(m.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Effect,
    Transition,
    /// An audio effect: an ffmpeg filter chain on audio clips ([`crate::afx`]).
    Audio,
}

/// `effect.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// `core.color/desaturate`, `project.my_glow`.
    pub id: String,
    /// Semver. Clips may pin the major version: `core.color/desaturate@1`.
    pub version: String,
    pub kind: Kind,
    /// `wgsl` for video effects and transitions, `ffmpeg` or `process` for
    /// audio effects (`wasm` and `ofx` come later).
    #[serde(default = "wgsl")]
    pub runtime: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The code file next to the manifest (`wgsl`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shader: String,
    /// The ffmpeg filter chain template (`ffmpeg` runtime, audio effects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// The plugin command that processes the audio (`process` runtime): a
    /// method one installed plugin contributes, e.g. `vst3.process`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// JSON Schema object whose `properties` are the params.
    #[serde(default = "empty_schema")]
    pub params: Value,
    /// Optional `SKILL.md` that teaches agents when to use it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
}

fn wgsl() -> String {
    "wgsl".into()
}

fn empty_schema() -> Value {
    serde_json::json!({"type": "object", "properties": {}})
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Number,
    Bool,
    Color,
    /// `vec2<f32>` to `vec4<f32>`; the number is the size.
    Vec(u8),
    /// A picture from the effect's folder, bound as a texture.
    Image,
    /// Plain text (`process` runtime only), e.g. a plugin's saved state.
    Text,
}

/// A picture an effect samples (a luma map, a displacement map), decoded once.
/// Stored values, not linearized: images are data for the shader.
#[derive(Debug)]
pub struct FxImage {
    /// File name in the effect's folder.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Content hash: GPU textures are cached by it.
    pub hash: u64,
}

/// Largest image an effect may ship, per side.
const MAX_IMAGE_SIDE: u32 = 4096;

/// A file name an image param may use: no folders, PNG or JPEG.
pub fn is_image_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !name.starts_with('.')
        && [".png", ".jpg", ".jpeg"].iter().any(|e| lower.ends_with(e))
}

/// Decode a PNG or JPEG for an image param.
pub fn decode_image(name: &str, bytes: &[u8]) -> Result<FxImage, FxError> {
    if !is_image_name(name) {
        return Err(err(format!("image `{name}`: use a plain file name ending in .png, .jpg or .jpeg")));
    }
    let img = image::load_from_memory(bytes).map_err(|e| err(format!("image `{name}`: {e}")))?.to_rgba8();
    let (width, height) = img.dimensions();
    if width == 0 || height == 0 || width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE {
        return Err(err(format!("image `{name}` is {width}x{height}; images must be 1 to {MAX_IMAGE_SIDE} pixels per side")));
    }
    let rgba = img.into_raw();
    let mut h = DefaultHasher::new();
    rgba.hash(&mut h);
    (width, height).hash(&mut h);
    Ok(FxImage { name: name.to_owned(), width, height, rgba, hash: h.finish() })
}

/// One field of the params uniform, in layout order: a param, or padding.
struct Slot<'a> {
    param: Option<&'a Param>,
    ty: &'static str,
}

impl Slot<'_> {
    fn bytes(&self) -> usize {
        match self.ty {
            "vec4<f32>" => 16,
            "vec3<f32>" => 12,
            "vec2<f32>" => 8,
            _ => 4,
        }
    }
}

/// One param, parsed from the schema.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: ParamType,
    pub default: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub animatable: bool,
}

impl Param {
    /// `x` within the param's range (keyframe curves may overshoot it).
    pub fn clamp(&self, x: f64) -> f64 {
        let x = self.min.map_or(x, |m| x.max(m));
        self.max.map_or(x, |m| x.min(m))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Builtin,
    Project,
    /// An installed effect pack (a plugin with `contributes.effects`). Projects
    /// never render from a pack: using one copies it into `effects/`.
    Pack,
}

/// A loaded effect or transition.
#[derive(Debug, Clone)]
pub struct Fx {
    pub manifest: Manifest,
    pub params: Vec<Param>,
    /// The author's code.
    pub source: String,
    pub origin: Origin,
    /// Folder it was loaded from (project effects).
    pub dir: Option<PathBuf>,
    /// Hash of manifest, code and images: GPU pipelines are cached by it.
    pub hash: u64,
    /// Pictures image params can use, by file name.
    pub images: BTreeMap<String, Arc<FxImage>>,
}

const RESERVED: &[&str] = &["u", "p", "src", "outgoing", "incoming", "effect", "transition", "uv"];

/// Texture bindings: 0 is the clip (or the outgoing clip), 4 the incoming clip,
/// images start here in param order.
pub const FIRST_IMAGE_BINDING: u32 = 5;

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(|c| c.is_ascii_lowercase() || c == '_') && cs.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl Fx {
    /// Check a manifest and its code and build the effect. Does not touch
    /// the GPU; [`crate::gpu::Gpu::test_fx`] does the test render.
    pub fn new(manifest: Manifest, source: String, origin: Origin, dir: Option<PathBuf>) -> Result<Self, FxError> {
        Self::with_images(manifest, source, origin, dir, BTreeMap::new())
    }

    /// [`Self::new`] with the pictures its image params may use.
    pub fn with_images(
        manifest: Manifest,
        source: String,
        origin: Origin,
        dir: Option<PathBuf>,
        images: BTreeMap<String, Arc<FxImage>>,
    ) -> Result<Self, FxError> {
        match (manifest.runtime.as_str(), manifest.kind) {
            ("wgsl", Kind::Audio) => {
                return Err(err("audio effects run on the `ffmpeg` runtime: set \"runtime\": \"ffmpeg\" and give a \"filter\" chain"));
            }
            ("ffmpeg" | "process", Kind::Audio) | ("wgsl", _) => {}
            ("ffmpeg" | "process", _) => return Err(err(format!("the `{}` runtime is for audio effects (\"kind\": \"audio\")", manifest.runtime))),
            (other, _) => {
                return Err(err(format!(
                    "runtime `{other}` is not supported yet; `wgsl` (video), `ffmpeg` and `process` (audio) run today (wasm and ofx come later)"
                )));
            }
        }
        let id_ok = !manifest.id.is_empty()
            && manifest.id.split(['.', '/']).all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'));
        if !id_ok || !manifest.id.contains('.') {
            return Err(err(format!("id `{}`: use lowercase dotted names like `project.my_glow` or `core.color/desaturate`", manifest.id)));
        }
        semver::Version::parse(&manifest.version).map_err(|e| err(format!("version `{}`: {e}", manifest.version)))?;
        let params = parse_params(&manifest.params)?;
        if manifest.runtime != "process"
            && let Some(p) = params.iter().find(|p| p.ty == ParamType::Text)
        {
            return Err(err(format!("param `{}`: plain text params are for the `process` runtime only", p.name)));
        }
        let source = if manifest.runtime == "process" {
            let method = manifest.method.clone().unwrap_or_default();
            if !method.contains('.') {
                return Err(err("a `process` effect needs a \"method\": the plugin command that processes the audio, like `myplugin.process`"));
            }
            method
        } else if manifest.kind == Kind::Audio {
            let filter = manifest.filter.clone().ok_or_else(|| err("an audio effect needs a \"filter\": an ffmpeg audio filter chain with {param} placeholders"))?;
            crate::afx::check(&crate::afx::parse(&filter)?, &params)?;
            filter
        } else {
            if manifest.shader.is_empty() {
                return Err(err("a wgsl effect needs a \"shader\" file"));
            }
            source
        };
        let mut h = DefaultHasher::new();
        serde_json::to_string(&manifest).unwrap_or_default().hash(&mut h);
        source.hash(&mut h);
        for img in images.values() {
            img.hash.hash(&mut h);
        }
        let fx = Self { manifest, params, source, origin, dir, hash: h.finish(), images };
        if fx.kind() == Kind::Audio {
            return Ok(fx);
        }
        for p in fx.image_params() {
            fx.check_image(p, &p.default).map_err(|e| err(format!("default: {e}")))?;
        }
        fx.validate()?;
        Ok(fx)
    }

    /// `id@major`, the form clips store.
    pub fn reference(&self) -> String {
        let major = self.manifest.version.split('.').next().unwrap_or("0");
        format!("{}@{major}", self.manifest.id)
    }

    pub fn kind(&self) -> Kind {
        self.manifest.kind
    }

    /// The params uniform, field by field. Largest alignment first so no
    /// field needs hidden padding: vec4 (colors too), vec3 (each followed by
    /// one pad), vec2, then scalars; padded to a multiple of 16 bytes.
    fn layout(&self) -> Vec<Slot<'_>> {
        let mut v = Vec::new();
        for p in self.params.iter().filter(|p| matches!(p.ty, ParamType::Color | ParamType::Vec(4))) {
            v.push(Slot { param: Some(p), ty: "vec4<f32>" });
        }
        for p in self.params.iter().filter(|p| p.ty == ParamType::Vec(3)) {
            v.push(Slot { param: Some(p), ty: "vec3<f32>" });
            v.push(Slot { param: None, ty: "f32" });
        }
        for p in self.params.iter().filter(|p| p.ty == ParamType::Vec(2)) {
            v.push(Slot { param: Some(p), ty: "vec2<f32>" });
        }
        for p in self.params.iter().filter(|p| matches!(p.ty, ParamType::Number | ParamType::Bool)) {
            v.push(Slot { param: Some(p), ty: "f32" });
        }
        let mut size: usize = v.iter().map(Slot::bytes).sum();
        while size == 0 || !size.is_multiple_of(16) {
            v.push(Slot { param: None, ty: "f32" });
            size += 4;
        }
        v
    }

    /// Bytes of the params uniform.
    pub fn params_size(&self) -> usize {
        self.layout().iter().map(Slot::bytes).sum()
    }

    /// Image params, in binding order.
    pub fn image_params(&self) -> impl Iterator<Item = &Param> {
        self.params.iter().filter(|p| p.ty == ParamType::Image)
    }

    /// The pictures to bind for a set of param values (others use defaults).
    pub fn images_for(&self, values: &Map<String, Value>) -> Vec<Arc<FxImage>> {
        self.image_params()
            .filter_map(|p| {
                let chosen = values.get(&p.name).and_then(Value::as_str).and_then(|n| self.images.get(n));
                chosen.or_else(|| p.default.as_str().and_then(|d| self.images.get(d))).cloned()
            })
            .collect()
    }

    fn check_image(&self, p: &Param, v: &Value) -> Result<(), FxError> {
        let name = v.as_str().ok_or_else(|| err(format!("param `{}`: expected an image file name, not {v}", p.name)))?;
        if self.images.contains_key(name) {
            return Ok(());
        }
        let have: Vec<&str> = self.images.keys().map(String::as_str).collect();
        Err(err(format!(
            "param `{}`: no image `{name}` in the effect's folder{}",
            p.name,
            if have.is_empty() { String::new() } else { format!(" (it has {})", have.join(", ")) }
        )))
    }

    /// The full shader: engine prelude, then the author's code. Returns it
    /// with the number of prelude lines, to map errors back to the author's file.
    pub fn wrapped(&self) -> (String, usize) {
        let mut s = String::new();
        s.push_str("struct VeTime {\n    time: f32,\n    progress: f32,\n    resolution: vec2<f32>,\n};\n");
        s.push_str("struct VeParams {\n");
        for (i, slot) in self.layout().iter().enumerate() {
            match slot.param {
                Some(p) => s.push_str(&format!("    {}: {},\n", p.name, slot.ty)),
                None => s.push_str(&format!("    ve_pad{i}: {},\n", slot.ty)),
            }
        }
        s.push_str("};\n");
        s.push_str("@group(0) @binding(0) var ve_a: texture_2d<f32>;\n");
        s.push_str("@group(0) @binding(1) var ve_s: sampler;\n");
        s.push_str("@group(0) @binding(2) var<uniform> u: VeTime;\n");
        s.push_str("@group(0) @binding(3) var<uniform> p: VeParams;\n");
        match self.kind() {
            Kind::Effect => {
                s.push_str("fn src(uv: vec2<f32>) -> vec4<f32> { return textureSampleLevel(ve_a, ve_s, uv, 0.0); }\n");
            }
            Kind::Audio => {}
            Kind::Transition => {
                s.push_str("@group(0) @binding(4) var ve_b: texture_2d<f32>;\n");
                s.push_str("fn outgoing(uv: vec2<f32>) -> vec4<f32> { return textureSampleLevel(ve_a, ve_s, uv, 0.0); }\n");
                s.push_str("fn incoming(uv: vec2<f32>) -> vec4<f32> { return textureSampleLevel(ve_b, ve_s, uv, 0.0); }\n");
            }
        }
        for (i, p) in self.image_params().enumerate() {
            let b = FIRST_IMAGE_BINDING + i as u32;
            s.push_str(&format!("@group(0) @binding({b}) var ve_img{i}: texture_2d<f32>;\n"));
            s.push_str(&format!("fn {}(uv: vec2<f32>) -> vec4<f32> {{ return textureSampleLevel(ve_img{i}, ve_s, uv, 0.0); }}\n", p.name));
        }
        s.push_str("struct VeOut {\n    @builtin(position) pos: vec4<f32>,\n    @location(0) uv: vec2<f32>,\n};\n");
        s.push_str("@vertex fn ve_vs(@builtin(vertex_index) i: u32) -> VeOut {\n");
        s.push_str("    let xy = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));\n");
        s.push_str("    var o: VeOut;\n    o.pos = vec4<f32>(xy * 2.0 - 1.0, 0.0, 1.0);\n");
        s.push_str("    o.uv = vec2<f32>(xy.x, 1.0 - xy.y);\n    return o;\n}\n");
        let call = match self.kind() {
            Kind::Effect | Kind::Audio => "effect",
            Kind::Transition => "transition",
        };
        s.push_str(&format!("@fragment fn ve_fs(i: VeOut) -> @location(0) vec4<f32> {{ return {call}(i.uv); }}\n"));
        let lines = s.lines().count();
        s.push_str(&self.source);
        (s, lines)
    }

    /// Parse and validate the wrapped shader with naga. Errors point at the
    /// author's file and line.
    pub fn validate(&self) -> Result<(), FxError> {
        let (src, prelude) = self.wrapped();
        let file = &self.manifest.shader;
        let call = match self.kind() {
            Kind::Effect | Kind::Audio => "fn effect(uv: vec2<f32>) -> vec4<f32>",
            Kind::Transition => "fn transition(uv: vec2<f32>) -> vec4<f32>",
        };
        let place = |line: u32, col: u32| -> String {
            let line = line as usize;
            if line > prelude { format!("{file} line {}, column {col}", line - prelude) } else { format!("the engine wrapper (line {line})") }
        };
        let module = naga::front::wgsl::parse_str(&src).map_err(|e| {
            // naga's report has the position of the first label (`wgsl:LINE:COL`).
            let report = e.emit_to_string(&src);
            let at = first_position(&report);
            let msg = e.message().to_owned();
            match at {
                Some((line, col)) if line > prelude => err(format!("{}: {msg}", place(line as u32, col as u32))),
                _ => err(format!(
                    "{file}: {msg}. The code must define `{call}`, and must not redefine engine names ({}).",
                    RESERVED.join(", ")
                )),
            }
        })?;
        let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default());
        v.validate(&module).map_err(|e| {
            let at = e.spans().next().map(|(span, _)| span.location(&src));
            let inner = e.as_inner().to_string();
            err(format!("{}: {inner}", at.map_or_else(|| file.clone(), |l| place(l.line_number, l.line_position))))
        })?;
        Ok(())
    }

    /// Check a set of param values (as a clip stores them) against the schema.
    pub fn check_params(&self, values: &Map<String, Value>) -> Result<(), FxError> {
        for (k, v) in values {
            let p = self.params.iter().find(|p| &p.name == k).ok_or_else(|| {
                let names: Vec<&str> = self.params.iter().map(|p| p.name.as_str()).collect();
                err(format!(
                    "{} has no param `{k}`; its params are {}",
                    self.manifest.id,
                    if names.is_empty() { "none".to_owned() } else { names.join(", ") }
                ))
            })?;
            check_value(p, v)?;
            if p.ty == ParamType::Image {
                self.check_image(p, v)?;
            }
        }
        Ok(())
    }

    /// Keyframes are only allowed on params marked `x-animatable`.
    pub fn check_keyframes(&self, keys: &indexmap::IndexMap<String, Vec<Keyframe>>) -> Result<(), FxError> {
        for (k, list) in keys {
            let p = self.params.iter().find(|p| &p.name == k).ok_or_else(|| err(format!("{} has no param `{k}`", self.manifest.id)))?;
            if !p.animatable && !list.is_empty() {
                return Err(err(format!("param `{k}` of {} cannot be keyframed", self.manifest.id)));
            }
            for kf in list {
                check_value(p, &kf.v)?;
            }
        }
        Ok(())
    }

    /// The params uniform at local time `t` (seconds since the clip or
    /// transition starts): keyframes, then set values, then defaults.
    pub fn pack(&self, values: &Map<String, Value>, keys: &indexmap::IndexMap<String, Vec<Keyframe>>, t: Q) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.params_size());
        for slot in self.layout() {
            let Some(p) = slot.param else {
                out.extend_from_slice(&0f32.to_le_bytes());
                continue;
            };
            let v = keys
                .get(&p.name)
                .filter(|k| !k.is_empty())
                .map(|k| interpolate(k, t))
                .or_else(|| values.get(&p.name).cloned())
                .unwrap_or_else(|| p.default.clone());
            match p.ty {
                ParamType::Color => {
                    let c = color(&v);
                    for (i, x) in c.iter().enumerate() {
                        let x = x.clamp(0.0, 1.0);
                        let x = if i < 3 { srgb_to_linear(x) } else { x };
                        out.extend_from_slice(&x.to_le_bytes());
                    }
                }
                ParamType::Vec(n) => {
                    for i in 0..n as usize {
                        let x = p.clamp(v.get(i).and_then(Value::as_f64).unwrap_or(0.0)) as f32;
                        out.extend_from_slice(&x.to_le_bytes());
                    }
                }
                ParamType::Number => out.extend_from_slice(&(p.clamp(v.as_f64().unwrap_or(0.0)) as f32).to_le_bytes()),
                ParamType::Bool => out.extend_from_slice(&(if v.as_bool().unwrap_or(false) { 1.0f32 } else { 0.0 }).to_le_bytes()),
                ParamType::Image | ParamType::Text => {}
            }
        }
        out.resize(self.params_size(), 0);
        out
    }
}

fn check_value(p: &Param, v: &Value) -> Result<(), FxError> {
    let bad = |what: &str| err(format!("param `{}`: {what}, not {v}", p.name));
    match p.ty {
        ParamType::Number => {
            let x = v.as_f64().ok_or_else(|| bad("expected a number"))?;
            if p.min.is_some_and(|m| x < m) || p.max.is_some_and(|m| x > m) {
                return Err(err(format!(
                    "param `{}` is {x}; it must be within {} to {}",
                    p.name,
                    p.min.map_or("-inf".into(), |m| m.to_string()),
                    p.max.map_or("inf".into(), |m| m.to_string())
                )));
            }
        }
        ParamType::Bool => {
            v.as_bool().ok_or_else(|| bad("expected true or false"))?;
        }
        ParamType::Text => {
            v.as_str().ok_or_else(|| bad("expected a string"))?;
        }
        ParamType::Color => {
            let a = v.as_array().ok_or_else(|| bad("expected a color like [1, 0.5, 0] (sRGB, 0 to 1)"))?;
            if !(3..=4).contains(&a.len()) || a.iter().any(|x| !x.as_f64().is_some_and(|x| (0.0..=1.0).contains(&x))) {
                return Err(bad("expected 3 or 4 numbers from 0 to 1"));
            }
        }
        ParamType::Vec(n) => {
            let want = format!("expected {n} numbers");
            let a = v.as_array().filter(|a| a.len() == n as usize).ok_or_else(|| bad(&want))?;
            for x in a {
                let x = x.as_f64().ok_or_else(|| bad(&want))?;
                if p.min.is_some_and(|m| x < m) || p.max.is_some_and(|m| x > m) {
                    return Err(err(format!(
                        "param `{}` has {x}; each number must be within {} to {}",
                        p.name,
                        p.min.map_or("-inf".into(), |m| m.to_string()),
                        p.max.map_or("inf".into(), |m| m.to_string())
                    )));
                }
            }
        }
        ParamType::Image => {
            let name = v.as_str().ok_or_else(|| bad("expected an image file name"))?;
            if !is_image_name(name) {
                return Err(bad("expected a file name in the effect's folder ending in .png, .jpg or .jpeg"));
            }
        }
    }
    Ok(())
}

fn color(v: &Value) -> [f32; 4] {
    let a: Vec<f32> = v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
    [a.first().copied().unwrap_or(0.0), a.get(1).copied().unwrap_or(0.0), a.get(2).copied().unwrap_or(0.0), a.get(3).copied().unwrap_or(1.0)]
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// `(line, column)` from the first `wgsl:LINE:COL` in a naga report.
fn first_position(report: &str) -> Option<(usize, usize)> {
    let i = report.find("wgsl:")?;
    let mut parts = report[i + 5..].split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty());
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Value of a keyframed param at `t` (eases and handles: [`ve_model::curve`]).
pub fn interpolate(keys: &[Keyframe], t: Q) -> Value {
    ve_model::curve::interpolate(keys, t)
}

fn parse_params(schema: &Value) -> Result<Vec<Param>, FxError> {
    let Some(props) = schema.get("properties") else { return Ok(Vec::new()) };
    let props = props.as_object().ok_or_else(|| err("params.properties must be an object"))?;
    let mut out = Vec::new();
    for (name, s) in props {
        if !is_ident(name) || RESERVED.contains(&name.as_str()) || name.starts_with("ve_") {
            return Err(err(format!("param name `{name}`: use lowercase letters, digits and _, and not {} or a ve_ prefix", RESERVED.join(", "))));
        }
        let format = s.get("format").and_then(Value::as_str);
        let ty = match s.get("type").and_then(Value::as_str) {
            Some("number" | "integer") => ParamType::Number,
            Some("boolean") => ParamType::Bool,
            Some("array") if format == Some("color") => ParamType::Color,
            Some("array") => {
                let items = s.get("items").and_then(|i| i.get("type")).and_then(Value::as_str);
                let lo = s.get("minItems").and_then(Value::as_u64);
                let hi = s.get("maxItems").and_then(Value::as_u64);
                match (items, lo, hi) {
                    (Some("number" | "integer"), Some(n), Some(m)) if n == m && (2..=4).contains(&n) => ParamType::Vec(n as u8),
                    _ => {
                        return Err(err(format!(
                            "param `{name}`: a vector is {{\"type\": \"array\", \"items\": {{\"type\": \"number\"}}, \"minItems\": N, \"maxItems\": N}} with N from 2 to 4 (add \"format\": \"color\" for a color)"
                        )));
                    }
                }
            }
            Some("string") if format == Some("image") => ParamType::Image,
            Some("string") if format.is_none() => ParamType::Text,
            other => {
                return Err(err(format!(
                    "param `{name}`: type {other:?} is not supported; use number, integer, boolean, a color or vector array, a string with \"format\": \"image\", or (process effects) a plain string"
                )));
            }
        };
        // Vector bounds may sit on the items, where JSON Schema puts them.
        let bound = |k: &str| s.get(k).or_else(|| s.get("items").and_then(|i| i.get(k))).and_then(Value::as_f64);
        let (min, max) = (bound("minimum"), bound("maximum"));
        let default = match (s.get("default"), ty) {
            (Some(d), _) => d.clone(),
            (None, ParamType::Number) => Value::from(min.unwrap_or(0.0).max(0.0).min(max.unwrap_or(f64::MAX))),
            (None, ParamType::Bool) => Value::Bool(false),
            (None, ParamType::Color) => serde_json::json!([1.0, 1.0, 1.0]),
            (None, ParamType::Vec(n)) => Value::Array(vec![Value::from(min.unwrap_or(0.0).max(0.0)); n as usize]),
            (None, ParamType::Image) => return Err(err(format!("param `{name}`: an image param needs a \"default\" file"))),
            (None, ParamType::Text) => Value::String(String::new()),
        };
        let animatable = s.get("x-animatable").and_then(Value::as_bool).unwrap_or(false);
        if animatable && matches!(ty, ParamType::Image | ParamType::Text) {
            return Err(err(format!("param `{name}`: images and text cannot be keyframed")));
        }
        let p = Param { name: name.clone(), ty, default, min, max, animatable };
        check_value(&p, &p.default).map_err(|e| err(format!("default: {e}")))?;
        out.push(p);
    }
    Ok(out)
}

/// Built-ins, compiled in from `plugins/core/effects`.
macro_rules! builtin {
    ($dir:literal) => {
        (
            include_str!(concat!("../../../plugins/core/effects/", $dir, "/effect.json")),
            include_str!(concat!("../../../plugins/core/effects/", $dir, "/effect.wgsl")),
        )
    };
}

const BUILTINS: &[(&str, &str)] = &[
    builtin!("desaturate"),
    builtin!("exposure"),
    builtin!("tint"),
    builtin!("vignette"),
    builtin!("blur"),
    builtin!("dissolve"),
    builtin!("dip"),
    builtin!("wipe"),
    builtin!("slide"),
];

/// Built-in audio effects: a manifest only (the code is its `filter`).
const AUDIO_BUILTINS: &[&str] = &[
    include_str!("../../../plugins/core/effects/audio-eq/effect.json"),
    include_str!("../../../plugins/core/effects/audio-highpass/effect.json"),
    include_str!("../../../plugins/core/effects/audio-compressor/effect.json"),
    include_str!("../../../plugins/core/effects/audio-noise-reduction/effect.json"),
    include_str!("../../../plugins/core/effects/audio-deesser/effect.json"),
];

/// The effects and transitions a project can use.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    pub list: Vec<Arc<Fx>>,
    /// Project effects that failed to load, with the reason.
    pub problems: Vec<String>,
    /// The project folder it was loaded for: `process` effects find the
    /// plugin host that serves it ([`crate::plugin_fx`]).
    pub root: Option<PathBuf>,
}

fn builtins() -> &'static [Arc<Fx>] {
    static B: std::sync::OnceLock<Vec<Arc<Fx>>> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        BUILTINS
            .iter()
            .map(|(json, wgsl)| {
                let m: Manifest = serde_json::from_str(json).expect("built-in manifest parses");
                Arc::new(Fx::new(m, (*wgsl).to_owned(), Origin::Builtin, None).expect("built-in effect validates"))
            })
            .chain(AUDIO_BUILTINS.iter().map(|json| {
                let m: Manifest = serde_json::from_str(json).expect("built-in manifest parses");
                Arc::new(Fx::new(m, String::new(), Origin::Builtin, None).expect("built-in audio effect checks"))
            }))
            .collect()
    })
}

/// How long a kept registry is trusted before `effects/` is listed again.
const RECHECK: std::time::Duration = std::time::Duration::from_millis(200);

struct Loaded {
    signature: u64,
    checked: std::time::Instant,
    registry: Registry,
}

fn loaded() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, Loaded>> {
    static LOADED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, Loaded>>> = std::sync::OnceLock::new();
    LOADED.get_or_init(Default::default)
}

/// A hash of the names, sizes and modification times of everything one
/// level inside each effect folder: changes when any effect file does.
fn signature(effects: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    let mut dirs: Vec<std::fs::DirEntry> = std::fs::read_dir(effects).into_iter().flatten().flatten().collect();
    dirs.sort_by_key(std::fs::DirEntry::file_name);
    for d in dirs {
        d.file_name().hash(&mut h);
        hash_files(&d.path(), &mut h);
    }
    h.finish()
}

/// Hash the names, sizes and modification times of the files in one folder.
fn hash_files(dir: &Path, h: &mut DefaultHasher) {
    let mut files: Vec<std::fs::DirEntry> = std::fs::read_dir(dir).into_iter().flatten().flatten().collect();
    files.sort_by_key(std::fs::DirEntry::file_name);
    for f in files {
        f.file_name().hash(h);
        if let Ok(m) = f.metadata() {
            m.len().hash(h);
            m.modified().ok().hash(h);
        }
    }
}

/// Read `effect.json`, its code and its images (every PNG or JPEG next to it) from a folder.
pub fn load_dir(dir: &Path, origin: Origin) -> Result<Fx, FxError> {
    let mp = dir.join("effect.json");
    let text = std::fs::read_to_string(&mp).map_err(|e| err(format!("{}: {e}", mp.display())))?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| err(format!("{}: {e}", mp.display())))?;
    if m.kind == Kind::Audio {
        // The code is the manifest's `filter`.
        return Fx::new(m, String::new(), origin, Some(dir.to_path_buf()));
    }
    let sp = dir.join(&m.shader);
    let source = std::fs::read_to_string(&sp).map_err(|e| err(format!("{}: {e}", sp.display())))?;
    Fx::with_images(m, source, origin, Some(dir.to_path_buf()), read_images(dir)?)
}

/// Every PNG or JPEG in a folder, decoded, by file name.
pub fn read_images(dir: &Path) -> Result<BTreeMap<String, Arc<FxImage>>, FxError> {
    let mut images = BTreeMap::new();
    let entries = std::fs::read_dir(dir).map_err(|e| err(format!("{}: {e}", dir.display())))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if is_image_name(&name) {
            let bytes = std::fs::read(e.path()).map_err(|e| err(format!("{name}: {e}")))?;
            images.insert(name.clone(), Arc::new(decode_image(&name, &bytes)?));
        }
    }
    Ok(images)
}

impl Registry {
    /// Built-ins plus `<project>/effects/*/effect.json`.
    ///
    /// Loading validates every project effect (naga) and decodes its images,
    /// which is far too slow to do per frame, so the result is kept per
    /// project and reused while no file under `effects/` changed (names,
    /// sizes and modification times; one directory listing per effect).
    pub fn for_project(root: Option<&Path>) -> Self {
        let Some(root) = root else { return Self { list: builtins().to_vec(), problems: Vec::new(), root: None } };
        let loaded = loaded();
        if let Some(e) = loaded.lock().unwrap_or_else(|e| e.into_inner()).get(root)
            && e.checked.elapsed() < RECHECK
        {
            return e.registry.clone();
        }
        let sig = signature(&root.join("effects"));
        {
            let mut m = loaded.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(e) = m.get_mut(root)
                && e.signature == sig
            {
                e.checked = std::time::Instant::now();
                return e.registry.clone();
            }
        }
        let r = Self::load(root);
        let entry = Loaded { signature: sig, checked: std::time::Instant::now(), registry: r.clone() };
        loaded.lock().unwrap_or_else(|e| e.into_inner()).insert(root.to_owned(), entry);
        r
    }

    /// Drop the kept registry of a project, so the next [`Self::for_project`]
    /// reads `effects/` again (after writing an effect there).
    pub fn forget(root: &Path) {
        loaded().lock().unwrap_or_else(|e| e.into_inner()).remove(root);
    }

    /// [`Self::for_project`] without the reuse: read and validate everything.
    pub fn load(root: &Path) -> Self {
        let mut r = Self { list: builtins().to_vec(), problems: Vec::new(), root: Some(root.to_owned()) };
        let Ok(entries) = std::fs::read_dir(root.join("effects")) else { return r };
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join("effect.json").is_file()).collect();
        dirs.sort();
        for d in dirs {
            match load_dir(&d, Origin::Project) {
                Ok(fx) if builtins().iter().any(|b| b.manifest.id == fx.manifest.id) => {
                    r.problems.push(format!("{}: {} is a built-in id", d.display(), fx.manifest.id));
                }
                // Majors of one id may sit side by side (clips name `id@major`).
                Ok(fx) if r.list.iter().any(|x| x.reference() == fx.reference()) => {
                    r.problems.push(format!("{}: {} is already taken", d.display(), fx.reference()));
                }
                Ok(fx) => r.list.push(Arc::new(fx)),
                Err(e) => r.problems.push(format!("{}: {e}", d.display())),
            }
        }
        r
    }

    /// The effects in installed packs: one folder per effect (from
    /// `ve_plugin::Manifest::effect_dirs`), earlier folders winning a shared
    /// `id@major`. Built-in ids and `project.` ids are refused. Like
    /// [`Self::for_project`], the result is kept while no file in the
    /// folders changes.
    pub fn packs(dirs: &[PathBuf]) -> Self {
        static KEPT: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, Registry>>> = std::sync::OnceLock::new();
        let mut h = DefaultHasher::new();
        for d in dirs {
            d.hash(&mut h);
            hash_files(d, &mut h);
        }
        let key = h.finish();
        let kept = KEPT.get_or_init(Default::default);
        if let Some(r) = kept.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return r.clone();
        }
        let mut r = Self::default();
        for d in dirs {
            match load_dir(d, Origin::Pack) {
                Ok(fx) if fx.manifest.id.starts_with("project.") => {
                    r.problems.push(format!("{}: `project.` ids are for project effects", d.display()));
                }
                Ok(fx) if builtins().iter().any(|b| b.manifest.id == fx.manifest.id) => {
                    r.problems.push(format!("{}: {} is a built-in id", d.display(), fx.manifest.id));
                }
                Ok(fx) if r.list.iter().any(|x| x.reference() == fx.reference()) => {
                    r.problems.push(format!("{}: {} is already in another pack", d.display(), fx.reference()));
                }
                Ok(fx) => r.list.push(Arc::new(fx)),
                Err(e) => r.problems.push(format!("{}: {e}", d.display())),
            }
        }
        let mut kept = kept.lock().unwrap_or_else(|e| e.into_inner());
        if kept.len() >= 8 {
            kept.clear();
        }
        kept.insert(key, r.clone());
        r
    }

    /// Find by `id` or `id@major`.
    pub fn get(&self, reference: &str) -> Option<Arc<Fx>> {
        let (id, major) = match reference.split_once('@') {
            Some((id, m)) => (id, Some(m)),
            None => (reference, None),
        };
        self.list
            .iter()
            .filter(|f| f.manifest.id == id && major.is_none_or(|m| f.manifest.version.split('.').next() == Some(m)))
            .max_by(|a, b| {
                let va = semver::Version::parse(&a.manifest.version).ok();
                let vb = semver::Version::parse(&b.manifest.version).ok();
                va.cmp(&vb)
            })
            .cloned()
    }

    /// [`Self::get`] with a helpful error, checking the kind.
    pub fn require(&self, reference: &str, kind: Kind) -> Result<Arc<Fx>, FxError> {
        let fx = self.get(reference).ok_or_else(|| {
            let names: Vec<String> = self.list.iter().filter(|f| f.kind() == kind).map(|f| f.manifest.id.clone()).collect();
            err(format!("no {} `{reference}`; available: {}", kind_word(kind), names.join(", ")))
        })?;
        if fx.kind() != kind {
            return Err(err(format!("{reference} is a {}, not a {}", kind_word(fx.kind()), kind_word(kind))));
        }
        Ok(fx)
    }
}

pub fn kind_word(k: Kind) -> &'static str {
    match k {
        Kind::Effect => "effect",
        Kind::Transition => "transition",
        Kind::Audio => "audio effect",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ve_model::time::{Rate, RationalTime};

    fn manifest(kind: Kind, params: Value) -> Manifest {
        Manifest {
            id: "project.test".into(),
            version: "1.0.0".into(),
            kind,
            runtime: "wgsl".into(),
            name: "Test".into(),
            description: String::new(),
            shader: "effect.wgsl".into(),
            filter: None,
            method: None,
            params,
            skill: None,
        }
    }

    #[test]
    fn builtins_load_and_validate() {
        let r = Registry::for_project(None);
        assert_eq!(r.list.len(), 14);
        assert!(r.get("core.color/desaturate@1").is_some());
        assert!(r.get("core.color/desaturate@2").is_none());
        assert!(r.require("core.transition/wipe", Kind::Effect).is_err());
        assert!(r.require("core.audio/eq", Kind::Effect).is_err(), "an audio effect is not a video effect");
        // Every built-in audio effect runs through ffmpeg (on PATH for these tests), keyframes included.
        for fx in r.list.iter().filter(|f| f.kind() == Kind::Audio) {
            crate::afx::test(fx).unwrap_or_else(|e| panic!("{}: {e}", fx.manifest.id));
        }
    }

    #[test]
    fn project_registry_is_kept_until_effects_change() {
        let root = std::env::temp_dir().join(format!("ve-fx-reuse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("effects")).unwrap();
        assert_eq!(Registry::for_project(Some(&root)).list.len(), 14);
        // A new effect folder shows up once the recheck window has passed, or at once after `forget`.
        let dir = root.join("effects").join("tint2");
        std::fs::create_dir_all(&dir).unwrap();
        let src = "fn effect(uv: vec2<f32>) -> vec4<f32> { return src(uv); }";
        let m = serde_json::json!({"id": "project.tint2", "version": "1.0.0", "kind": "effect", "name": "Tint 2", "shader": "effect.wgsl"});
        std::fs::write(dir.join("effect.json"), m.to_string()).unwrap();
        std::fs::write(dir.join("effect.wgsl"), src).unwrap();
        Registry::forget(&root);
        let r = Registry::for_project(Some(&root));
        assert!(r.get("project.tint2").is_some(), "{:?}", r.problems);
        // Changing a file is noticed without `forget`, after the recheck window.
        std::fs::write(dir.join("effect.wgsl"), format!("{src}\n")).unwrap();
        std::thread::sleep(RECHECK + std::time::Duration::from_millis(50));
        let again = Registry::for_project(Some(&root));
        assert_ne!(again.get("project.tint2").unwrap().hash, r.get("project.tint2").unwrap().hash);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn errors_point_at_the_authors_line() {
        let src = "fn effect(uv: vec2<f32>) -> vec4<f32> {\n    let c = src(uv);\n    return c * nope;\n}\n";
        let e = Fx::new(manifest(Kind::Effect, empty_schema()), src.into(), Origin::Project, None).unwrap_err();
        assert!(e.0.contains("effect.wgsl line 3"), "{e}");
        let e = Fx::new(manifest(Kind::Effect, empty_schema()), "fn other() {}".into(), Origin::Project, None).unwrap_err();
        assert!(e.0.contains("fn effect(uv: vec2<f32>) -> vec4<f32>"), "{e}");
        let bad_type = "fn effect(uv: vec2<f32>) -> vec4<f32> {\n    return 1.0;\n}\n";
        let e = Fx::new(manifest(Kind::Effect, empty_schema()), bad_type.into(), Origin::Project, None).unwrap_err();
        assert!(e.0.contains("effect.wgsl line"), "{e}");
    }

    #[test]
    fn params_pack_in_layout_order_with_keyframes() {
        let schema = serde_json::json!({"properties": {
            "amount": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5, "x-animatable": true},
            "tint": {"type": "array", "format": "color", "default": [1, 1, 1]},
            "on": {"type": "boolean", "default": true}
        }});
        let src = "fn effect(uv: vec2<f32>) -> vec4<f32> { return src(uv) * p.amount * p.tint * p.on; }";
        let fx = Fx::new(manifest(Kind::Effect, schema), src.into(), Origin::Project, None).unwrap();
        assert_eq!(fx.params_size(), 32);
        let mut keys = indexmap::IndexMap::new();
        let r = Rate::integer(10);
        keys.insert(
            "amount".to_owned(),
            vec![
                Keyframe { t: RationalTime::from_frames(0, r), v: Value::from(0.0), ease: "linear".into(), r#in: None, out: None },
                Keyframe { t: RationalTime::from_frames(10, r), v: Value::from(1.0), ease: "linear".into(), r#in: None, out: None },
            ],
        );
        let b = fx.pack(&Map::new(), &keys, Q::new(1, 4));
        let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        assert_eq!((f(0), f(12)), (1.0, 1.0), "color first, linear white");
        assert!((f(16) - 0.25).abs() < 1e-6, "amount interpolated: {}", f(16));
        assert_eq!(f(20), 1.0, "bool");
        assert!(fx.check_params(&serde_json::from_value(serde_json::json!({"amount": 2})).unwrap()).is_err());
        assert!(fx.check_params(&serde_json::from_value(serde_json::json!({"size": 2})).unwrap()).is_err());
    }

    #[test]
    fn unsupported_runtimes_and_names_are_refused() {
        let mut m = manifest(Kind::Effect, empty_schema());
        m.runtime = "wasm".into();
        assert!(Fx::new(m, String::new(), Origin::Project, None).unwrap_err().0.contains("not supported yet"));
        let schema = serde_json::json!({"properties": {"src": {"type": "number"}}});
        assert!(Fx::new(manifest(Kind::Effect, schema), String::new(), Origin::Project, None).is_err());
    }
}
