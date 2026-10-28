//! Plugin audio effects (`runtime: "process"`) line up with the mix: a
//! plugin that changes nothing leaves every sample where it was, across a
//! cut (crossfade handles), a trimmed in-point and a speed change. The
//! plugin here is a stand-in processor in this process. Needs ffmpeg on PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use ve_model::Id;
use ve_model::meta::{Asset, AssetKind, OutputSettings};
use ve_model::otio::{Composable, Timeline};
use ve_model::time::{Q, Rate};
use ve_ops::edit::{self, Insert, SetSpeed, Split, Trim};
use ve_ops::effects;
use ve_render::Span;
use ve_render::fx::Registry;
use ve_render::plugin_fx::{self, Processor};
use ve_render::source::AudioSource;

const FPS: i64 = 25;
const SR: u32 = 48_000;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("ve-render-tests").join(name);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn ffmpeg(args: &[&str]) {
    let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().expect("ffmpeg on PATH");
    assert!(st.success(), "ffmpeg {args:?}");
}

/// Copies input to output and remembers each call's `start_sample` and `frames`.
struct Copy {
    root: PathBuf,
    calls: Mutex<Vec<(i64, i64)>>,
}

impl Processor for Copy {
    fn serves(&self, root: &Path) -> bool {
        root == self.root
    }
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.root.join("cache"))
    }
    fn version(&self, _: &str) -> Option<String> {
        Some("copy@1.0.0".into())
    }
    fn process(&self, method: &str, p: Value) -> Result<Value, String> {
        assert_eq!(method, "copy.process");
        assert_eq!(p["channels"], 2);
        self.calls.lock().unwrap().push((p["start_sample"].as_i64().unwrap(), p["frames"].as_i64().unwrap()));
        fs::copy(p["input"].as_str().unwrap(), p["output"].as_str().unwrap()).map_err(|e| e.to_string())?;
        Ok(json!({}))
    }
}

fn clips(tl: &Timeline) -> Vec<Id> {
    tl.tracks
        .children
        .iter()
        .filter_map(|c| match c {
            Composable::Track(t) if t.name == "A1" => Some(t),
            _ => None,
        })
        .flat_map(|t| &t.children)
        .filter_map(|c| match c {
            Composable::Clip(c) => ve_ops::lane::clip_id(c).cloned(),
            _ => None,
        })
        .collect()
}

fn mix(tl: &Timeline, fx: &Registry, work: &Path) -> Vec<f32> {
    let mut out = Vec::new();
    let span = Span::new(Q::from_integer(0), ve_render::plan::timeline_end(tl));
    ve_render::decode_audio(&AudioSource::Timeline { tl, span, fx }, SR, 2, work, &mut |s| out.extend_from_slice(s)).unwrap();
    out
}

/// A project folder whose `effects/` has the `copy.copy` plugin effect,
/// served by a [`Copy`] processor, and a timeline with `source` on A1
/// from 0.5 s to 3.5 s of the file.
fn setup(name: &str, source: &[&str]) -> (PathBuf, Arc<Copy>, Timeline) {
    let root = dir(name);
    let fx_dir = root.join("effects/copy");
    fs::create_dir_all(&fx_dir).unwrap();
    let manifest = json!({"id": "copy.copy", "version": "1.0.0", "kind": "audio", "runtime": "process", "method": "copy.process",
        "name": "Copy", "params": {"type": "object", "properties": {"state": {"type": "string"}}}});
    fs::write(fx_dir.join("effect.json"), manifest.to_string()).unwrap();
    let proc = Arc::new(Copy { root: root.clone(), calls: Mutex::new(Vec::new()) });
    let as_dyn: Arc<dyn Processor> = proc.clone();
    // Registration is weak: the returned `proc` keeps it alive for the test.
    plugin_fx::register(Arc::downgrade(&as_dyn));

    let wav = root.join("source.wav");
    let mut args: Vec<&str> = source.to_vec();
    let out = wav.to_string_lossy().into_owned();
    args.push(&out);
    ffmpeg(&args);
    let output = OutputSettings { width: 64, height: 64, rate: Rate::integer(FPS), sample_rate: SR, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("pfx", output);
    tl.metadata.ve.assets.push(Asset {
        id: Id::from_raw("asset_src"),
        url: ve_media::file_url(&wav).unwrap(),
        rel: None,
        kind: AssetKind::Source,
        hash: None,
        provenance: None,
        media: Some(ve_media::probe(&wav).unwrap().media),
    });
    let ins = Insert {
        asset: Id::from_raw("asset_src"),
        track: None,
        audio_track: Some("A1".into()),
        at: None,
        source_in: Some(Q::new(1, 2)),
        source_out: Some(Q::new(7, 2)),
        duration: None,
        video: false,
        audio: true,
        streams: None,
        overwrite: false,
    };
    let tl = edit::insert(&tl, &ins).unwrap().0;
    (root, proc, tl)
}

fn with_copy(tl: &Timeline) -> Timeline {
    let mut with = tl.clone();
    for c in clips(tl) {
        with = effects::add(&with, &c, "copy.copy@1", Map::new(), None).unwrap().0;
    }
    with
}

#[test]
fn a_plugin_that_changes_nothing_changes_nothing() {
    // Two tones mixed, so a misplaced sample shows. Cut at 1 s, the second
    // part trimmed in by 0.3 s: a cut with crossfade handles on both sides.
    let tone = |f: u32| format!("sine=f={f}:r={SR}:d=4");
    let (root, proc, tl) = setup(
        "plugin-fx-identity",
        &["-f", "lavfi", "-i", &tone(440), "-f", "lavfi", "-i", &tone(1234), "-filter_complex", "amix=inputs=2"],
    );
    let tl = edit::split(&tl, &Split { at: Q::from_integer(1), clips: Vec::new(), tracks: Vec::new(), linked: true }).unwrap().0;
    let second = clips(&tl)[1].clone();
    let tl = edit::trim(&tl, &Trim { clip: second, in_delta: Some(Q::new(3, 10)), out_delta: None, roll: false, linked: true }).unwrap().0;

    let fx = Registry::for_project(Some(&root));
    let work = root.join("work");
    fs::create_dir_all(&work).unwrap();
    let plain = mix(&tl, &fx, &work);
    let with = with_copy(&tl);
    let processed = mix(&with, &fx, &work);
    let calls = proc.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2, "one call per clip");
    assert!(calls.iter().all(|&(start, _)| start < 0), "a handle before each clip: {calls:?}");
    assert_eq!(plain.len(), processed.len());
    let worst = plain.iter().zip(&processed).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(worst < 1e-5, "largest difference {worst}");

    // Rendering again reuses the processed files.
    mix(&with, &fx, &work);
    assert_eq!(proc.calls.lock().unwrap().len(), 2);
}

/// First frame at or after `from` where 10 ms of the left side fall below -60 dBFS.
fn falls_silent(s: &[f32], from: usize) -> usize {
    let w = SR as usize / 100;
    (from..s.len() / 2 - w)
        .step_by(12)
        .find(|&i| {
            let e: f32 = (i..i + w).map(|k| s[2 * k] * s[2 * k]).sum::<f32>() / w as f32;
            10.0 * e.max(1e-20).log10() < -60.0
        })
        .expect("the tone stops")
}

#[test]
fn sped_up_clips_keep_their_timing() {
    // A tone for the first 2 s of the file, then silence. Played from 0.5 s
    // at 3/2, it stops 1 s into the timeline.
    let (root, _proc, tl) = setup("plugin-fx-speed", &["-f", "lavfi", "-i", &format!("sine=f=440:r={SR}:d=2"), "-af", "apad=whole_dur=4"]);
    let clip = clips(&tl)[0].clone();
    let tl = edit::set_speed(&tl, &SetSpeed { clip, speed: Q::new(3, 2), linked: true, ripple: true }).unwrap().0;
    let fx = Registry::for_project(Some(&root));
    let work = root.join("work");
    fs::create_dir_all(&work).unwrap();
    let plain = mix(&tl, &fx, &work);
    let processed = mix(&with_copy(&tl), &fx, &work);
    assert_eq!(plain.len(), processed.len());
    let (a, b) = (falls_silent(&plain, SR as usize / 2), falls_silent(&processed, SR as usize / 2));
    let ms = |i: usize| i as f64 * 1000.0 / f64::from(SR);
    assert!((ms(a) - 1000.0).abs() < 15.0, "the plain mix stops at {:.1} ms", ms(a));
    assert!((ms(a) - ms(b)).abs() < 5.0, "plain stops at {:.1} ms, processed at {:.1} ms", ms(a), ms(b));
}
