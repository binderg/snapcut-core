//! The `ffmpeg` runtime for audio effects (ARCHITECTURE.md, "Audio effects").
//!
//! An audio effect is a manifest with `kind: "audio"`, `runtime: "ffmpeg"`
//! and a `filter` template: an ffmpeg audio filter chain with `{param}`
//! placeholders, e.g. `equalizer=f={freq}:t=q:w={q}:g={gain}`. The render
//! graph puts the chain on each audio clip that uses the effect (after speed,
//! before gain and edge fades), with the param values at the clip's start.
//! Keyframed params are changed while it plays with `asendcmd`, at every
//! keyframe and every [`STEP`] between them.
//!
//! Templates come from projects and from third-party effect packs, so they
//! are held to a small, safe shape: filters from [`ALLOWED`] only (none of
//! them read or write files or take extra inputs), one linear chain, and no
//! quoting, escapes, labels, instance names or `;`.

use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;
use ve_model::meta::Keyframe;
use ve_model::time::Q;

use crate::fx::{Fx, FxError, Param, ParamType, interpolate};

/// Audio filters a template may use: EQ and filters, dynamics, repair and
/// noise reduction, modulation and echo, stereo and level. Every one maps
/// one audio stream to one audio stream of the same length and touches no
/// files.
pub const ALLOWED: &[&str] = &[
    // EQ and filters
    "equalizer", "bass", "treble", "lowshelf", "highshelf", "highpass", "lowpass", "bandpass", "bandreject", "allpass", "tiltshelf",
    // Dynamics
    "acompressor", "agate", "alimiter", "compand", "dynaudnorm", "speechnorm", "asoftclip",
    // Repair and noise reduction
    "afftdn", "anlmdn", "adeclick", "adeclip", "deesser", "dcshift",
    // Modulation and echo
    "aecho", "chorus", "flanger", "aphaser", "tremolo", "vibrato", "apulsator",
    // Colour, stereo and level
    "crystalizer", "aexciter", "asubboost", "acrusher", "stereotools", "extrastereo", "stereowiden", "crossfeed", "haas", "volume",
];

/// How often keyframed values are sent between keyframes.
pub const STEP: Q = Q::new_raw(1, 100);

/// One filter of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub name: String,
    /// `(key, value)`; `key` is `None` for a positional option.
    pub options: Vec<(Option<String>, String)>,
}

/// A parsed `filter` template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub filters: Vec<Filter>,
}

fn bad(m: impl Into<String>) -> FxError {
    FxError(m.into())
}

/// Parse and check a template's shape (not its params: see [`check`]).
pub fn parse(template: &str) -> Result<Template, FxError> {
    let t = template.trim();
    if t.is_empty() {
        return Err(bad("`filter` is empty; give an ffmpeg audio filter chain like `equalizer=f={freq}:t=q:w=1:g={gain}`"));
    }
    if let Some(c) = t.chars().find(|c| matches!(c, '\'' | '"' | '\\' | ';' | '[' | ']' | '@' | '\n' | '\r' | '\t')) {
        return Err(bad(format!(
            "`filter` may not contain {c:?}: write one chain of filters joined by commas, options as key=value joined by colons"
        )));
    }
    let mut filters = Vec::new();
    for part in t.split(',') {
        let part = part.trim();
        let (name, opts) = part.split_once('=').unwrap_or((part, ""));
        let name = name.trim();
        if name.is_empty() {
            return Err(bad(format!("`filter`: empty filter in `{t}`")));
        }
        if !ALLOWED.contains(&name) {
            return Err(bad(format!("`filter`: `{name}` is not an allowed audio filter; allowed: {}", ALLOWED.join(", "))));
        }
        let mut options = Vec::new();
        if !opts.is_empty() {
            for o in opts.split(':') {
                match o.split_once('=') {
                    Some((k, v)) if !k.trim().is_empty() => options.push((Some(k.trim().to_owned()), v.trim().to_owned())),
                    Some(_) => return Err(bad(format!("`filter`: option `{o}` of {name} has no name"))),
                    None => options.push((None, o.trim().to_owned())),
                }
            }
        }
        filters.push(Filter { name: name.to_owned(), options });
    }
    Ok(Template { filters })
}

/// The `{name}` placeholders in a value, or an error for a stray brace.
fn placeholders(v: &str) -> Result<Vec<String>, FxError> {
    let mut out = Vec::new();
    let mut rest = v;
    while let Some(i) = rest.find(['{', '}']) {
        if rest.as_bytes()[i] == b'}' {
            return Err(bad(format!("`filter`: stray `}}` in `{v}`")));
        }
        let after = &rest[i + 1..];
        let end = after.find('}').ok_or_else(|| bad(format!("`filter`: unclosed `{{` in `{v}`")))?;
        out.push(after[..end].to_owned());
        rest = &after[end + 1..];
    }
    Ok(out)
}

/// Check a template against its params: every placeholder names a param,
/// params are numbers, integers or booleans, and each keyframeable param
/// fills whole `key={name}` options (so it can be changed while playing).
pub fn check(t: &Template, params: &[Param]) -> Result<(), FxError> {
    for p in params {
        if !matches!(p.ty, ParamType::Number | ParamType::Bool) {
            return Err(bad(format!("param `{}`: audio effect params are numbers, integers or booleans", p.name)));
        }
    }
    let mut used = HashSet::new();
    for f in &t.filters {
        for (k, v) in &f.options {
            for name in placeholders(v)? {
                let p = params.iter().find(|p| p.name == name).ok_or_else(|| {
                    let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                    bad(format!("`filter`: `{{{name}}}` is not a param; params are {}", if names.is_empty() { "none".into() } else { names.join(", ") }))
                })?;
                if p.animatable && (k.is_none() || placeholders(v)?.len() != 1) {
                    return Err(bad(format!(
                        "param `{name}` is x-animatable, so it must be the only placeholder in a named option (`{}=key={{{name}}}`, a unit like `{{{name}}}dB` is fine), not `{v}`",
                        f.name
                    )));
                }
                used.insert(name);
            }
        }
    }
    if let Some(p) = params.iter().find(|p| !used.contains(&p.name)) {
        return Err(bad(format!("param `{}` is not used in `filter`", p.name)));
    }
    Ok(())
}

/// Where a keyframeable param goes: `(filter index, option key, value
/// template)`, e.g. `(0, "threshold", "{threshold}dB")`.
pub fn targets(t: &Template, param: &str) -> Vec<(usize, String, String)> {
    let mark = format!("{{{param}}}");
    t.filters
        .iter()
        .enumerate()
        .flat_map(|(i, f)| {
            f.options.iter().filter(|(k, v)| k.is_some() && v.contains(&mark)).map(move |(k, v)| (i, k.clone().unwrap_or_default(), v.clone()))
        })
        .collect()
}

/// A param value as ffmpeg reads it: a plain decimal, booleans as 0 or 1.
pub fn number(p: &Param, v: &Value) -> String {
    match p.ty {
        ParamType::Bool => if v.as_bool().unwrap_or(false) { "1" } else { "0" }.to_owned(),
        _ => {
            let x = p.clamp(v.as_f64().unwrap_or(0.0));
            let s = format!("{x:.6}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s == "-0" || s.is_empty() { "0".to_owned() } else { s.to_owned() }
        }
    }
}

/// A param's value at local time `t`: keyframes, else the clip's value, else the default.
fn value_at(p: &Param, values: &serde_json::Map<String, Value>, keys: &indexmap::IndexMap<String, Vec<Keyframe>>, t: Q) -> Value {
    keys.get(&p.name)
        .filter(|k| !k.is_empty())
        .map(|k| interpolate(k, t))
        .or_else(|| values.get(&p.name).cloned())
        .unwrap_or_else(|| p.default.clone())
}

/// The filter chain for one use of `fx` on a clip: values at local time
/// `from` (seconds since the clip starts), keyframes from there until
/// `from + len` sent as commands. `name` makes the filter instances unique
/// in the graph. Times in the chain count from 0 at `from`.
pub fn chain(fx: &Fx, values: &serde_json::Map<String, Value>, keys: &indexmap::IndexMap<String, Vec<Keyframe>>, from: Q, len: Q, name: &str) -> String {
    let t = parse(&fx.source).expect("audio effects are parsed when loaded");
    let param = |n: &str| fx.params.iter().find(|p| p.name == n);
    let mut filters: Vec<String> = Vec::new();
    for (i, f) in t.filters.iter().enumerate() {
        let opts: Vec<String> = f
            .options
            .iter()
            .map(|(k, v)| {
                let mut out = v.clone();
                for n in placeholders(v).unwrap_or_default() {
                    if let Some(p) = param(&n) {
                        out = out.replace(&format!("{{{n}}}"), &number(p, &value_at(p, values, keys, from)));
                    }
                }
                match k {
                    Some(k) => format!("{k}={out}"),
                    None => out,
                }
            })
            .collect();
        let head = format!("{}@{name}_{i}", f.name);
        filters.push(if opts.is_empty() { head } else { format!("{head}={}", opts.join(":")) });
    }
    // Keyframes: (time in the chain, command) for every change inside the piece.
    let mut cmds: Vec<(Q, String)> = Vec::new();
    for p in fx.params.iter().filter(|p| p.animatable) {
        let Some(list) = keys.get(&p.name).filter(|k| !k.is_empty()) else { continue };
        let first = list.iter().map(|k| k.t.seconds()).min().expect("not empty");
        let last = list.iter().map(|k| k.t.seconds()).max().expect("not empty");
        let (lo, hi) = (first.max(from), last.min(from + len));
        if hi < lo {
            continue;
        }
        let mut times: Vec<Q> = list.iter().map(|k| k.t.seconds()).filter(|&k| k > from && k <= hi).collect();
        let mut s = (lo / STEP).ceil() * STEP;
        while s <= hi {
            if s > from {
                times.push(s);
            }
            s += STEP;
        }
        times.sort();
        times.dedup();
        let mut prev = number(p, &value_at(p, values, keys, from));
        for at in times {
            let v = number(p, &value_at(p, values, keys, at));
            if v == prev {
                continue;
            }
            for (i, key, tpl) in targets(&t, &p.name) {
                let arg = tpl.replace(&format!("{{{}}}", p.name), &v);
                cmds.push((at - from, format!("{}@{name}_{i} {key} {arg}", t.filters[i].name)));
            }
            prev = v;
        }
    }
    if cmds.is_empty() {
        return filters.join(",");
    }
    cmds.sort_by_key(|c| c.0);
    let mut spec = String::new();
    let mut k = 0;
    while k < cmds.len() {
        let at = cmds[k].0;
        let mut same = Vec::new();
        while k < cmds.len() && cmds[k].0 == at {
            same.push(cmds[k].1.clone());
            k += 1;
        }
        if !spec.is_empty() {
            spec.push(';');
        }
        spec.push_str(&format!("{} {}", seconds(at), same.join(",")));
    }
    format!("asendcmd=c='{spec}',{}", filters.join(","))
}

fn seconds(q: Q) -> String {
    let x = *q.numer() as f64 / *q.denom() as f64;
    format!("{x:.6}")
}

/// Options of an ffmpeg filter that can change while it runs (ffmpeg's `T`
/// flag in `-h filter=NAME`), asked once per filter.
fn runtime_options(filter: &str) -> Result<HashSet<String>, FxError> {
    static KEPT: OnceLock<Mutex<HashMap<String, HashSet<String>>>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some(s) = kept.lock().unwrap_or_else(|e| e.into_inner()).get(filter) {
        return Ok(s.clone());
    }
    let out = Command::new(crate::ffmpeg::ffmpeg_path())
        .args(["-hide_banner", "-h", &format!("filter={filter}")])
        .output()
        .map_err(|e| bad(format!("running ffmpeg to check `{filter}`: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout);
    if text.contains("Unknown filter") || !text.contains("Filter ") {
        return Err(bad(format!("this ffmpeg has no `{filter}` filter")));
    }
    let mut set = HashSet::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let (Some(name), Some(ty), Some(flags)) = (words.next(), words.next(), words.next()) else { continue };
        if ty.starts_with('<') && flags.len() >= 8 && flags.chars().all(|c| c == '.' || c.is_ascii_uppercase()) && flags.contains('T') {
            set.insert(name.to_owned());
        }
    }
    kept.lock().unwrap_or_else(|e| e.into_inner()).insert(filter.to_owned(), set.clone());
    Ok(set)
}

/// The test an audio effect must pass before use (`fx.define`, and the
/// built-ins' tests): every keyframeable option can change while playing,
/// and half a second of tone through the chain at its defaults, with a
/// command for each keyframeable param, gives finite samples.
pub fn test(fx: &Fx) -> Result<(), FxError> {
    let t = parse(&fx.source)?;
    let mut keys = indexmap::IndexMap::new();
    for p in fx.params.iter().filter(|p| p.animatable) {
        for (i, key, _) in targets(&t, &p.name) {
            let f = &t.filters[i].name;
            if !runtime_options(f)?.contains(&key) {
                return Err(bad(format!(
                    "param `{}` is x-animatable, but {f}'s option `{key}` cannot change while playing; drop x-animatable or use an option that can",
                    p.name
                )));
            }
        }
        // A keyframe pair, so the test sends a command for it.
        let rate = ve_model::time::Rate::integer(1000);
        let at = |ms: i64, v: Value| Keyframe { t: ve_model::RationalTime { value: Q::from_integer(ms), rate }, v, ease: "linear".into(), r#in: None, out: None };
        keys.insert(p.name.clone(), vec![at(100, p.default.clone()), at(200, p.default.clone())]);
    }
    let mut values = serde_json::Map::new();
    // Send a real change: nudge each keyframed number at its second key.
    for p in fx.params.iter().filter(|p| p.animatable && p.ty == ParamType::Number) {
        let d = p.default.as_f64().unwrap_or(0.0);
        let to = p.clamp(if p.max.is_some_and(|m| m > d) { d + (p.max.unwrap_or(d) - d) / 2.0 } else { d });
        if let Some(list) = keys.get_mut(&p.name) {
            list[1].v = Value::from(to);
        }
        values.insert(p.name.clone(), p.default.clone());
    }
    let c = chain(fx, &values, &keys, Q::from_integer(0), Q::new(1, 2), "test");
    let out = Command::new(crate::ffmpeg::ffmpeg_path())
        .args(["-v", "error", "-hide_banner", "-f", "lavfi", "-i", "sine=f=440:r=48000:d=0.5", "-af"])
        .arg(format!("aformat=sample_fmts=fltp:channel_layouts=stereo,{c}"))
        .args(["-f", "f32le", "-"])
        .output()
        .map_err(|e| bad(format!("running ffmpeg for the test: {e}")))?;
    let why = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || out.stdout.is_empty() {
        return Err(bad(format!("ffmpeg refused the chain `{c}`: {}", why.trim())));
    }
    if !why.trim().is_empty() {
        return Err(bad(format!("ffmpeg complained about the chain `{c}`: {}", why.trim())));
    }
    let finite = out.stdout.chunks_exact(4).all(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).is_finite());
    if !finite {
        return Err(bad("the test run produced NaN or infinite samples"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_are_held_to_a_safe_shape() {
        let t = parse("equalizer=f={freq}:t=q:w=1:g={gain}, highpass=f=80").unwrap();
        assert_eq!(t.filters.len(), 2);
        assert_eq!(t.filters[0].options[3], (Some("g".into()), "{gain}".into()));
        for bad in [
            "amovie=/etc/passwd",
            "ametadata=mode=print:file=out.txt",
            "volume=2[a];[a]volume=1",
            "volume='1,2'",
            "volume@x=2",
            "",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} should be refused");
        }
        assert!(placeholders("{a}+{b}").unwrap() == ["a", "b"]);
        assert!(placeholders("{a").is_err() && placeholders("a}").is_err());
    }
}
