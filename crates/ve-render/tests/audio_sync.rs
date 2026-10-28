//! Audio stays in sync over long clips: a 44.1 kHz source in a 48 kHz
//! project (resampled), as plain PCM and as AAC in MP4 (encoder delay
//! trimmed by the file's edit list), with a cut in the middle. A click every
//! second must land where it belongs at the start, after the cut and at the
//! end of a minute. Needs ffmpeg on PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ve_model::Id;
use ve_model::meta::{Asset, AssetKind, OutputSettings};
use ve_model::otio::Timeline;
use ve_model::time::{Q, Rate};
use ve_ops::edit::{self, Insert, RippleDelete};
use ve_render::Span;
use ve_render::fx::Registry;
use ve_render::source::AudioSource;

const FPS: i64 = 25;
const SR: u32 = 48_000;
const SRC_RATE: u32 = 44_100;
const SECS: i64 = 60;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("ve-render-tests").join(name);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A short click at the start of every second, stereo, at 44.1 kHz.
fn clicks(out: &Path, codec: &[&str]) {
    let expr = format!("if(lt(mod(n\\,{SRC_RATE})\\,8)\\,0.9\\,0)");
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("aevalsrc=exprs='{expr}|{expr}':s={SRC_RATE}:d={SECS}"))
        .args(codec)
        .arg(out)
        .status()
        .expect("ffmpeg on PATH");
    assert!(st.success());
}

fn project(src: &Path) -> Timeline {
    let output = OutputSettings { width: 64, height: 64, rate: Rate::integer(FPS), sample_rate: SR, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("sync", output);
    tl.metadata.ve.assets.push(Asset {
        id: Id::from_raw("asset_clicks"),
        url: ve_media::file_url(src).unwrap(),
        rel: None,
        kind: AssetKind::Source,
        hash: None,
        provenance: None,
        media: Some(ve_media::probe(src).unwrap().media),
    });
    let ins = Insert {
        asset: Id::from_raw("asset_clicks"),
        track: None,
        audio_track: Some("A1".into()),
        at: None,
        source_in: None,
        source_out: None,
        duration: None,
        video: false,
        audio: true,
        streams: None,
        overwrite: false,
    };
    edit::insert(&tl, &ins).unwrap().0
}

/// The left channel of the mix.
fn mix(tl: &Timeline, work: &Path) -> Vec<f32> {
    let fx = Registry::for_project(None);
    let span = Span::new(Q::from_integer(0), ve_render::plan::timeline_end(tl));
    let mut left = Vec::new();
    ve_render::decode_audio(&AudioSource::Timeline { tl, span, fx: &fx }, SR, 2, work, &mut |s| left.extend(s.iter().step_by(2))).unwrap();
    left
}

/// Where the click nearest `at` (a sample index) starts, searching 50 ms either side.
fn click_near(s: &[f32], at: i64) -> i64 {
    let w = i64::from(SR) / 20;
    let lo = (at - w).max(0) as usize;
    let hi = ((at + w) as usize).min(s.len());
    let peak = (lo..hi).max_by(|&a, &b| s[a].abs().total_cmp(&s[b].abs())).unwrap();
    // The click's rising edge: first sample above half the peak.
    let half = s[peak].abs() / 2.0;
    (lo..=peak).find(|&i| s[i].abs() >= half).unwrap() as i64
}

/// Every click lands within `tolerance` samples of `second * SR`, where
/// `seconds` maps a timeline second to the source second it should play.
fn check(s: &[f32], seconds: impl Iterator<Item = i64>, tolerance: i64, what: &str) {
    for t in seconds {
        let at = t * i64::from(SR);
        let got = click_near(s, at);
        assert!((got - at).abs() <= tolerance, "{what}: the click at {t} s is at sample {got}, {} off", got - at);
    }
}

fn case(name: &str, codec: &[&str], tolerance: i64) {
    let work = dir(name);
    let src = work.join(format!("clicks.{}", if codec.contains(&"aac") { "mp4" } else { "wav" }));
    clicks(&src, codec);
    let tl = project(&src);
    let whole = mix(&tl, &work);
    assert_eq!(whole.len() as i64, SECS * i64::from(SR), "{name}: a minute at 48 kHz");
    check(&whole, 1..SECS, tolerance, name);

    // Cut 10.4 s to 20 s out (on the 25 fps grid): the click at 21 s plays at 11.4 s, and so on to the end.
    let rd = RippleDelete { ranges: vec![(Q::new(52, 5), Q::from_integer(20))], markers: None, clips: Vec::new(), tracks: Vec::new(), linked: true };
    let cut = edit::ripple_delete(&tl, &rd).unwrap().0;
    let after = mix(&cut, &work);
    check(&after, 1..10, tolerance, &format!("{name}, before the cut"));
    // Every later click plays 9.6 s earlier than in the source.
    for src_second in 21..SECS {
        let at = src_second * i64::from(SR) - 48 * i64::from(SR) / 5;
        let got = click_near(&after, at);
        assert!((got - at).abs() <= tolerance, "{name}, after the cut: source second {src_second} is {} samples off", got - at);
    }
}

#[test]
fn resampled_pcm_stays_on_the_sample() {
    case("sync-pcm", &["-c:a", "pcm_s16le"], 1);
}

#[test]
fn aac_in_mp4_stays_within_a_millisecond() {
    // AAC's encoder delay is trimmed by the MP4 edit list; what is left is
    // the codec's own smearing of a click.
    case("sync-aac", &["-c:a", "aac", "-b:a", "192k"], i64::from(SR) / 1000);
}
