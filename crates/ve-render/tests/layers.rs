//! `ve_render::layers`: what shows at a frame and over a range, worked out
//! from geometry and each asset's transparency. Needs ffmpeg on PATH (to
//! make and probe the sources), not a GPU.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;
use ve_model::Id;
use ve_model::meta::{Alpha, Asset, AssetKind, OutputSettings};
use ve_model::otio::{Composable, Timeline, TrackKind};
use ve_model::time::{Q, Rate};
use ve_ops::edit::{self, Insert};
use ve_ops::transform::TransformChange;
use ve_render::layers::{self, Frame, Layer, Visibility, VisibleSpan};

const FPS: i64 = 25;
const SIDE: u32 = 64;

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

fn add_asset(tl: &mut Timeline, id: &str, path: &Path) {
    let probe = ve_media::probe(path).unwrap();
    tl.metadata.ve.assets.push(Asset {
        id: Id::from_raw(id),
        url: ve_media::file_url(path).unwrap(),
        kind: AssetKind::Source,
        hash: None,
        provenance: None,
        rel: None,
        media: Some(probe.media),
    });
}

fn insert(asset: &str, track: Option<&str>, at: Option<Q>, duration: Option<Q>) -> Insert {
    Insert {
        asset: Id::from_raw(asset),
        track: track.map(str::to_owned),
        audio_track: None,
        at,
        source_in: None,
        source_out: None,
        duration,
        video: true,
        audio: false,
        overwrite: true,
    }
}

/// The first clip on a track.
fn clip_on(tl: &Timeline, track: &str) -> Id {
    tl.tracks
        .children
        .iter()
        .find_map(|c| match c {
            Composable::Track(t) if t.name == track => t.children.iter().find_map(|c| match c {
                Composable::Clip(c) => ve_ops::lane::clip_id(c).cloned(),
                _ => None,
            }),
            _ => None,
        })
        .expect("a clip on the track")
}

fn fx() -> ve_render::fx::Registry {
    ve_render::fx::Registry::for_project(None)
}

fn at(tl: &Timeline, n: i64) -> Frame {
    layers::at(tl, fr(n), &fx()).unwrap()
}

fn on<'a>(f: &'a Frame, track: &str) -> &'a Layer {
    f.layers.iter().find(|l| l.track == track).unwrap_or_else(|| panic!("no layer on {track}: {f:#?}"))
}

/// V1: a plain video, frames 0-49. V2: an opaque card (an RGBA PNG with no
/// clear pixels), frames 10-19. V3: a logo whose right half is clear,
/// frames 15-29.
fn timeline(work: &Path) -> Timeline {
    let bg = work.join("bg.mkv");
    ffmpeg(&["-f", "lavfi", "-i", &format!("color=gray:s={SIDE}x{SIDE}:r={FPS}"), "-t", "2", "-c:v", "ffv1", "-pix_fmt", "yuv420p", bg.to_str().unwrap()]);
    let card = work.join("card.png");
    ffmpeg(&["-f", "lavfi", "-i", &format!("color=blue:s={SIDE}x{SIDE},format=rgba"), "-frames:v", "1", card.to_str().unwrap()]);
    let logo = work.join("logo.png");
    ffmpeg(&[
        "-f", "lavfi", "-i", "color=red:s=32x64:d=0.04,format=rgba",
        "-f", "lavfi", "-i", "color=black@0:s=32x64:d=0.04,format=rgba",
        "-filter_complex", "[0][1]hstack,format=rgba", "-frames:v", "1", logo.to_str().unwrap(),
    ]);
    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("layers", output);
    add_asset(&mut tl, "asset_bg", &bg);
    add_asset(&mut tl, "asset_card", &card);
    add_asset(&mut tl, "asset_logo", &logo);
    tl = edit::insert(&tl, &insert("asset_bg", None, None, None)).unwrap().0;
    for _ in 0..2 {
        tl = ve_ops::tracks::add(&tl, TrackKind::Video, None, None).unwrap().0;
    }
    tl = edit::insert(&tl, &insert("asset_card", Some("V2"), Some(fr(10)), Some(fr(10)))).unwrap().0;
    edit::insert(&tl, &insert("asset_logo", Some("V3"), Some(fr(15)), Some(fr(15)))).unwrap().0
}

fn transform(tl: &Timeline, track: &str, values: &[(&str, f64)]) -> Timeline {
    let change = TransformChange { values: values.iter().map(|(k, v)| ((*k).to_owned(), Some(*v))).collect(), ..Default::default() };
    ve_ops::transform::set(tl, &clip_on(tl, track), &change).unwrap().0
}

#[test]
fn what_shows_at_a_frame() {
    let work = dir("layers-frame");
    let tl = timeline(&work);
    let alpha = |id: &str| tl.metadata.ve.assets.iter().find(|a| a.id.as_str() == id).unwrap().media.as_ref().unwrap().video.as_ref().unwrap().alpha;
    assert_eq!(alpha("asset_bg"), Some(Alpha::None));
    assert_eq!(alpha("asset_card"), Some(Alpha::Opaque), "an RGBA PNG with no clear pixel is measured opaque");
    assert_eq!(alpha("asset_logo"), Some(Alpha::Transparent));

    let f = at(&tl, 5);
    assert_eq!(f.visible, ["V1"]);
    let v1 = on(&f, "V1");
    assert_eq!((v1.visibility, v1.visible_percent, v1.asset.as_deref()), (Visibility::Full, 100, Some("asset_bg")));
    assert_eq!(v1.source_time, Some(fr(5)));
    assert_eq!((v1.rect.w, v1.rect.h), (64.0, 64.0));

    // The opaque card hides the video entirely.
    let f = at(&tl, 12);
    assert_eq!(f.visible, ["V2"]);
    let (v1, v2) = (on(&f, "V1"), on(&f, "V2"));
    assert_eq!((v1.visibility, v1.covered_by.as_slice()), (Visibility::Hidden, ["V2".to_owned()].as_slice()));
    assert!(v2.solid && v2.still && v2.source_time.is_none());

    // The logo is see-through: it overlaps, never hides.
    let f = at(&tl, 17);
    assert_eq!(f.visible, ["V2", "V3"]);
    assert_eq!(on(&f, "V2").overlapped_by, ["V3"]);
    assert_eq!(on(&f, "V2").visibility, Visibility::Full);
    assert!(!on(&f, "V3").solid);
    let f = at(&tl, 25);
    assert_eq!(f.visible, ["V1", "V3"]);
    assert_eq!((on(&f, "V1").visibility, on(&f, "V1").overlapped_by.as_slice()), (Visibility::Full, ["V3".to_owned()].as_slice()));

    // Half-size card: a quarter of the video is covered.
    let small = transform(&tl, "V2", &[("scale", 0.5)]);
    let f = at(&small, 12);
    let v1 = on(&f, "V1");
    assert_eq!((v1.visibility, v1.visible_percent), (Visibility::Partial, 75));
    assert_eq!(on(&f, "V2").rect.w, 32.0);
    assert_eq!(on(&f, "V2").transform.as_ref().unwrap().scale, 0.5);

    // Half opacity: drawn over, hides nothing.
    let faded = transform(&tl, "V2", &[("opacity", 0.5)]);
    let f = at(&faded, 12);
    assert_eq!(f.visible, ["V1", "V2"]);
    assert_eq!(on(&f, "V1").overlapped_by, ["V2"]);

    // Moved off the canvas: hidden, and says why.
    let gone = transform(&tl, "V2", &[("x", 2.0)]);
    let f = at(&gone, 12);
    assert_eq!(f.visible, ["V1"]);
    assert_eq!(on(&f, "V2").note.as_deref(), Some("off the canvas"));

    // Turned 90 degrees, a square card still covers the square canvas; at 45 it cannot be sure.
    let f = at(&transform(&tl, "V2", &[("rotation", 90.0)]), 12);
    assert_eq!(on(&f, "V1").visibility, Visibility::Hidden);
    let f = at(&transform(&tl, "V2", &[("rotation", 45.0)]), 12);
    assert_eq!(on(&f, "V1").visibility, Visibility::Full);

    // A disabled track is listed, not drawn.
    let mut off = tl.clone();
    for c in &mut off.tracks.children {
        if let Composable::Track(t) = c
            && t.name == "V2"
        {
            t.enabled = false;
        }
    }
    let f = at(&off, 12);
    assert_eq!((f.visible.as_slice(), f.disabled.as_slice()), (["V1".to_owned()].as_slice(), ["V2".to_owned()].as_slice()));
}

#[test]
fn effect_values_at_the_frame() {
    use ve_ops::effects;
    let work = dir("layers-fx");
    let tl = timeline(&work);
    let card = clip_on(&tl, "V2");
    let (tl, r) = effects::add(&tl, &card, "core.color/exposure@1", serde_json::Map::new(), None).unwrap();
    let keys = [("stops".to_owned(), vec![effects::keyframe(fr(0), json!(0.0), None, tl.output_rate()), effects::keyframe(fr(10), json!(2.0), None, tl.output_rate())])];
    let tl = effects::set(&tl, &r.effect, None, Some(&keys.into_iter().collect()), None).unwrap().0;

    // Frame 15 is half way through the card (frames 10-19).
    let f = at(&tl, 15);
    let v2 = on(&f, "V2");
    assert_eq!(v2.effects.len(), 1);
    let e = &v2.effects[0];
    assert_eq!((e.effect.as_str(), e.name.as_deref(), e.missing), ("core.color/exposure@1", Some("Exposure"), false));
    assert_eq!(e.params["stops"].as_f64(), Some(1.0));
    // An effect may change what covers what: the card no longer counts as hiding the video.
    assert!(!v2.solid);
    assert_eq!(on(&f, "V1").visibility, Visibility::Full);
}

#[test]
fn transitions_show_both_clips() {
    use ve_ops::effects::{self, Align, NewTransition};
    let work = dir("layers-transition");
    let base = timeline(&work);
    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("t", output);
    tl.metadata.ve.assets = base.metadata.ve.assets.clone();
    let mut a = insert("asset_bg", None, None, None);
    a.source_out = Some(fr(20));
    tl = edit::insert(&tl, &a).unwrap().0;
    let mut b = insert("asset_bg", None, Some(fr(20)), None);
    b.source_in = Some(fr(25));
    tl = edit::insert(&tl, &b).unwrap().0;
    let t = NewTransition {
        clip: None,
        at: Some(fr(20)),
        track: None,
        duration: fr(10),
        align: Align::Center,
        transition: "core.transition/dissolve@1".into(),
        params: serde_json::Map::new(),
    };
    tl = effects::add_transition(&tl, &t).unwrap().0;
    // The dissolve runs over frames 15-24; frame 20 is half way.
    let f = at(&tl, 20);
    assert_eq!(f.layers.len(), 2, "{f:#?}");
    let (out, inc) = (&f.layers[0], &f.layers[1]);
    let (to, ti) = (out.transition.as_ref().unwrap(), inc.transition.as_ref().unwrap());
    assert_eq!((to.role, ti.role), ("outgoing", "incoming"));
    assert_eq!((to.with.as_str(), ti.with.as_str()), (inc.clip.as_str(), out.clip.as_str()));
    assert!((to.progress - 0.5).abs() < 1e-6);
    assert_eq!(to.name.as_deref(), Some("Dissolve"));
    // Both blend: neither hides the other.
    assert!(f.layers.iter().all(|l| l.visibility == Visibility::Full && !l.solid));
    assert_eq!(f.visible, ["V1"]);
    assert!(at(&tl, 10).layers[0].transition.is_none());
}

#[test]
fn spans_over_a_range() {
    let work = dir("layers-spans");
    let tl = timeline(&work);
    let s = |a: i64, b: i64, visible: &[&str], hidden: &[&str]| VisibleSpan {
        start: fr(a),
        end: fr(b),
        visible: visible.iter().map(|t| (*t).to_owned()).collect(),
        hidden: hidden.iter().map(|t| (*t).to_owned()).collect(),
        audible: Vec::new(),
    };
    let all = ve_render::Span::new(Q::from_integer(0), ve_render::timeline_end(&tl));
    assert_eq!(
        layers::spans(&tl, all, &fx()).unwrap(),
        [
            s(0, 10, &["V1"], &[]),
            s(10, 15, &["V2"], &["V1"]),
            s(15, 20, &["V2", "V3"], &["V1"]),
            s(20, 30, &["V1", "V3"], &[]),
            s(30, 50, &["V1"], &[]),
        ]
    );
    // A part of it starts and ends where asked.
    assert_eq!(layers::spans(&tl, ve_render::Span::new(fr(12), fr(17)), &fx()).unwrap(), [s(12, 15, &["V2"], &["V1"]), s(15, 17, &["V2", "V3"], &["V1"])]);
}

/// V1+A1: a clip with picture and sound (linked), frames 0-49. A2: a music
/// bed, frames 20-74, so the end has sound and no picture.
#[test]
fn what_is_heard() {
    use ve_ops::edit::SetVolume;
    let work = dir("layers-audio");
    let talk = work.join("talk.mkv");
    ffmpeg(&[
        "-f", "lavfi", "-i", &format!("color=gray:s={SIDE}x{SIDE}:r={FPS}"),
        "-f", "lavfi", "-i", "sine=f=440:r=48000",
        "-t", "2", "-c:v", "ffv1", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le", talk.to_str().unwrap(),
    ]);
    let music = work.join("music.wav");
    ffmpeg(&["-f", "lavfi", "-i", "sine=f=220:r=48000", "-t", "3", music.to_str().unwrap()]);
    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("audio", output);
    add_asset(&mut tl, "asset_talk", &talk);
    add_asset(&mut tl, "asset_music", &music);
    let mut a = insert("asset_talk", None, None, None);
    a.audio = true;
    tl = edit::insert(&tl, &a).unwrap().0;
    tl = ve_ops::tracks::add(&tl, TrackKind::Audio, None, None).unwrap().0;
    let mut m = insert("asset_music", None, Some(fr(20)), Some(fr(55)));
    (m.video, m.audio, m.audio_track) = (false, true, Some("A2".into()));
    tl = edit::insert(&tl, &m).unwrap().0;
    let bed = clip_on(&tl, "A2");
    tl = edit::set_volume(&tl, &SetVolume { clip: bed.clone(), db: -18.0, linked: true }).unwrap().0;

    let f = at(&tl, 30);
    assert_eq!(f.audible, ["A1", "A2"]);
    let (a1, a2) = (&f.audio[0], &f.audio[1]);
    assert_eq!((a1.track.as_str(), a1.asset.as_deref(), a1.source_time, a1.gain_db), ("A1", Some("asset_talk"), fr(30), 0.0));
    assert!(a1.link.is_some(), "inserted with its picture");
    assert_eq!(a1.link, on(&f, "V1").link, "audio and picture share the link");
    assert_eq!((a2.clip.as_str(), a2.gain_db, a2.source_time, a2.link.as_deref()), (bed.as_str(), -18.0, fr(10), None));

    // Silenced at the floor: listed, not heard.
    let quiet = edit::set_volume(&tl, &SetVolume { clip: bed.clone(), db: -100.0, linked: true }).unwrap().0;
    let f = at(&quiet, 30);
    assert_eq!((f.audible.as_slice(), f.audio.len()), (["A1".to_owned()].as_slice(), 2));
    assert!(!f.audio[1].audible);

    // A muted track is listed and not mixed.
    let mut off = tl.clone();
    for c in &mut off.tracks.children {
        if let Composable::Track(t) = c
            && t.name == "A2"
        {
            t.enabled = false;
        }
    }
    let f = at(&off, 30);
    assert_eq!((f.audible.as_slice(), f.muted.as_slice()), (["A1".to_owned()].as_slice(), ["A2".to_owned()].as_slice()));

    // Over the timeline: the bed comes in at 20 and plays on past the picture.
    let names = |v: &[&str]| v.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
    let s = |a: i64, b: i64, visible: &[&str], audible: &[&str]| VisibleSpan { start: fr(a), end: fr(b), visible: names(visible), hidden: Vec::new(), audible: names(audible) };
    let all = ve_render::Span::new(Q::from_integer(0), ve_render::timeline_end(&tl));
    assert_eq!(
        layers::spans(&tl, all, &fx()).unwrap(),
        [s(0, 20, &["V1"], &["A1"]), s(20, 50, &["V1"], &["A1", "A2"]), s(50, 75, &[], &["A2"])]
    );
}
