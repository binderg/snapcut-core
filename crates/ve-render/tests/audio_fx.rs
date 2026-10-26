//! Audio effects (`kind: audio`, `runtime: ffmpeg`) in the rendered mix:
//! an EQ cut really cuts, keyframes move the value while it plays, the
//! effect never asks for the GPU, and `render.layers` shows the values.
//! Needs ffmpeg on PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use ve_model::Id;
use ve_model::meta::{Asset, AssetKind, OutputSettings};
use ve_model::otio::{Composable, Timeline};
use ve_model::time::{Q, Rate};
use ve_ops::edit::{self, Insert};
use ve_ops::effects;
use ve_render::Span;
use ve_render::fx::Registry;
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

fn fr(n: i64) -> Q {
    Q::new(n, FPS)
}

/// A 2-second 1 kHz tone alone on A1.
fn timeline(work: &Path) -> (Timeline, Id) {
    let tone = work.join("tone.wav");
    ffmpeg(&["-f", "lavfi", "-i", &format!("sine=f=1000:r={SR}:d=2"), tone.to_str().unwrap()]);
    let output = OutputSettings { width: 64, height: 64, rate: Rate::integer(FPS), sample_rate: SR, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("afx", output);
    let probe = ve_media::probe(&tone).unwrap();
    tl.metadata.ve.assets.push(Asset {
        id: Id::from_raw("asset_tone"),
        url: ve_media::file_url(&tone).unwrap(),
        rel: None,
        kind: AssetKind::Source,
        hash: None,
        provenance: None,
        media: Some(probe.media),
    });
    let ins = Insert {
        asset: Id::from_raw("asset_tone"),
        track: None,
        audio_track: Some("A1".into()),
        at: None,
        source_in: None,
        source_out: None,
        duration: None,
        video: false,
        audio: true,
        overwrite: false,
    };
    let tl = edit::insert(&tl, &ins).unwrap().0;
    let clip = tl
        .tracks
        .children
        .iter()
        .find_map(|c| match c {
            Composable::Track(t) if t.name == "A1" => t.children.iter().find_map(|c| match c {
                Composable::Clip(c) => ve_ops::lane::clip_id(c).cloned(),
                _ => None,
            }),
            _ => None,
        })
        .expect("the tone's clip");
    (tl, clip)
}

/// RMS in dBFS of the mix over `[from, to)` seconds, mono.
fn rms_db(tl: &Timeline, from: Q, to: Q, work: &Path) -> f64 {
    let fx = Registry::for_project(None);
    let mut sum = 0.0f64;
    let mut n = 0usize;
    ve_render::decode_audio(&AudioSource::Timeline { tl, span: Span::new(from, to), fx: &fx }, SR, 1, work, &mut |s| {
        for x in s {
            sum += f64::from(*x) * f64::from(*x);
            n += 1;
        }
    })
    .unwrap();
    10.0 * (sum / n.max(1) as f64).log10()
}

#[test]
fn eq_cut_lowers_the_tone_and_keyframes_move_it() {
    let work = dir("afx-eq");
    let (tl, clip) = timeline(&work);
    let whole = (Q::new(1, 2), Q::new(3, 2));
    let plain = rms_db(&tl, whole.0, whole.1, &work);

    // A 12 dB cut right on the tone.
    let cut = json!({"frequency": 1000, "gain": -12, "q": 1}).as_object().cloned().unwrap();
    let (eq, r) = effects::add(&tl, &clip, "core.audio/eq@1", cut, None).unwrap();
    let cut_db = rms_db(&eq, whole.0, whole.1, &work);
    assert!((plain - cut_db - 12.0).abs() < 0.5, "plain {plain:.2} dB, cut {cut_db:.2} dB");
    assert!(!ve_render::needs_gpu(&eq, Span::new(Q::from_integer(0), Q::from_integer(2))).unwrap(), "audio effects stay off the GPU");

    // Keyframes: no cut until 1 s, then down to -24 dB by 1.5 s.
    let rate = eq.output_rate();
    let k = |t: Q, v: f64| effects::keyframe(t, json!(v), None, rate);
    let keys = [("gain".to_owned(), vec![k(Q::from_integer(1), 0.0), k(Q::new(3, 2), -24.0)])].into_iter().collect();
    let ramp = effects::set(&eq, &r.effect, None, Some(&keys), None).unwrap().0;
    let early = rms_db(&ramp, Q::new(1, 5), Q::new(4, 5), &work);
    let late = rms_db(&ramp, Q::new(8, 5), Q::new(19, 10), &work);
    assert!((plain - early).abs() < 0.5, "before the keys: {early:.2} dB vs {plain:.2} dB");
    assert!((plain - late - 24.0).abs() < 1.0, "after the keys: {late:.2} dB vs {plain:.2} dB");

    // The layer inspector shows the value at a frame, keyframes evaluated.
    let f = ve_render::layers::at(&ramp, fr(31), &Registry::for_project(None)).unwrap();
    let e = &f.audio[0].effects[0];
    assert_eq!((e.effect.as_str(), e.name.as_deref()), ("core.audio/eq@1", Some("EQ band")));
    let gain = e.params["gain"].as_f64().unwrap();
    // Frame 31 is 1.24 s: 48% of the way from 0 to -24 dB.
    assert!((gain - -11.52).abs() < 0.01, "48% through the ramp: {gain}");

    // An effect that is not installed fails the render, naming it.
    let missing = effects::add(&tl, &clip, "project.nope@1", serde_json::Map::new(), None).unwrap().0;
    let fx = Registry::for_project(None);
    let e = ve_render::decode_audio(&AudioSource::Timeline { tl: &missing, span: Span::new(Q::from_integer(0), Q::from_integer(1)), fx: &fx }, SR, 1, &work, &mut |_| {})
        .unwrap_err();
    assert!(e.to_string().contains("project.nope"), "{e}");
}

#[test]
fn every_builtin_shapes_the_sound_without_breaking_it() {
    let work = dir("afx-builtins");
    let (tl, clip) = timeline(&work);
    let span = (Q::new(1, 2), Q::new(3, 2));
    let plain = rms_db(&tl, span.0, span.1, &work);
    for (id, params) in [
        ("core.audio/highpass@1", json!({"frequency": 1000})),
        ("core.audio/compressor@1", json!({"threshold": -30, "ratio": 8})),
        ("core.audio/noise_reduction@1", json!({"amount": 20})),
        ("core.audio/deesser@1", json!({})),
    ] {
        let with = effects::add(&tl, &clip, id, params.as_object().cloned().unwrap(), None).unwrap().0;
        let db = rms_db(&with, span.0, span.1, &work);
        assert!(db.is_finite() && db > -60.0, "{id}: {db:.2} dB");
        eprintln!("{id}: {plain:.2} -> {db:.2} dB");
    }
    // A high-pass at the tone's own frequency takes about 3 dB off; a hard compressor more.
    let hp = effects::add(&tl, &clip, "core.audio/highpass@1", json!({"frequency": 1000}).as_object().cloned().unwrap(), None).unwrap().0;
    let d = plain - rms_db(&hp, span.0, span.1, &work);
    assert!((1.5..4.5).contains(&d), "high-pass at 1 kHz on a 1 kHz tone: -{d:.2} dB");
    let comp = effects::add(&tl, &clip, "core.audio/compressor@1", json!({"threshold": -30, "ratio": 8}).as_object().cloned().unwrap(), None).unwrap().0;
    assert!(plain - rms_db(&comp, span.0, span.1, &work) > 6.0, "compressor turns a loud tone down");
}
