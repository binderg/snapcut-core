//! Render exactness against sources whose content encodes its position:
//! frame N has luma 2N, audio sample n is a sawtooth of n. Cuts, speed
//! changes and J cuts must land on exactly the right frames and samples.
//! Needs ffmpeg and ffprobe on PATH (Phase 1 requirement).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ve_model::Id;
use ve_model::meta::{Asset, AssetKind, OutputSettings};
use ve_model::otio::Timeline;
use ve_model::time::{Q, Rate};
use ve_ops::edit::{self, Insert, RippleDelete, SetSpeed, Split, Trim};
use ve_render::{Canvas, EncodeSettings, Fit, Graph, ImageFormat, Span};

const FPS: i64 = 25;
const SR: i64 = 48_000;
const SIDE: u32 = 64;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("ve-render-tests").join(name);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// 4 s of 25 fps 64x64 video, luma = 2 * frame index (lossless), with a
/// stereo PCM sawtooth: sample n = ((n mod 1000) - 500) / 1000.
fn make_source(path: &Path) {
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("nullsrc=s={SIDE}x{SIDE}:r={FPS},geq=lum='N*2':cb=128:cr=128"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!("aevalsrc=exprs='(mod(n\\,1000)-500)/1000|(mod(n\\,1000)-500)/1000':s={SR}"))
        .args(["-t", "4", "-c:v", "libx264", "-qp", "0", "-g", "10", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le"])
        .arg(path)
        .status()
        .expect("ffmpeg on PATH");
    assert!(st.success());
}

fn project(src: &Path) -> Timeline {
    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: SR as u32, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("t", output);
    let probe = ve_media::probe(src).unwrap();
    tl.metadata.ve.assets.push(Asset {
        id: Id::from_raw("asset_src"),
        url: ve_media::file_url(src).unwrap(),
        kind: AssetKind::Source,
        hash: None,
        provenance: None,
        rel: None,
        media: Some(probe.media),
    });
    let ins = Insert {
        asset: Id::from_raw("asset_src"),
        track: None,
        audio_track: None,
        at: None,
        source_in: None,
        source_out: None,
        duration: None,
        video: true,
        audio: true,
        streams: None,
        overwrite: false,
    };
    edit::insert(&tl, &ins).unwrap().0
}

fn fr(n: i64) -> Q {
    Q::new(n, FPS)
}

fn canvas() -> Canvas {
    Canvas { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: SR as u32, fit: Fit::Fit }
}

/// Frame indices of a video file, from its luma.
fn frame_indices(path: &Path) -> Vec<i64> {
    let out = Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(path).args(["-f", "rawvideo", "-pix_fmt", "yuv420p", "-"]).output().unwrap();
    let n = (SIDE * SIDE) as usize;
    out.stdout
        .chunks(n * 3 / 2)
        .map(|f| {
            let mean = f[..n].iter().map(|&b| f64::from(b)).sum::<f64>() / n as f64;
            (mean / 2.0).round() as i64
        })
        .collect()
}

/// Left-channel samples of a media file as i16.
fn samples(path: &Path) -> Vec<i16> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-af", "pan=mono|c0=c0", "-f", "s16le", "-ar", "48000", "-"])
        .output()
        .unwrap();
    out.stdout.chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
}

fn saw(n: i64) -> i16 {
    // The sawtooth written to s16 the way ffmpeg rounds it.
    ((((n % 1000) - 500) as f64 / 1000.0) * 32767.0).round() as i16
}

/// Render `span` of `tl` losslessly: video as FFV1, audio as PCM, in MKV.
fn render(tl: &Timeline, span: Span, xfade: Q, out: &Path) -> Vec<String> {
    let p = ve_render::plan(tl, span).unwrap();
    let mut g = Graph::new();
    let v = g.video(&p, &canvas());
    let a = g.audio(&p, &canvas(), xfade, &ve_render::fx::Registry::for_project(None)).unwrap();
    let args: Vec<String> = ["-map", &format!("[{v}]"), "-map", &format!("[{a}]"), "-c:v", "ffv1", "-c:a", "pcm_s16le", "-f", "matroska"]
        .iter()
        .map(|s| (*s).to_owned())
        .chain([out.to_string_lossy().into_owned()])
        .collect();
    ve_render::ffmpeg::run(&g, &args, out.parent().unwrap(), false).unwrap();
    p.warnings
}

/// The timeline: [media 0..10f][media 30..40f][media 60..100f], by split + ripple delete.
fn jump_cut(tl: &Timeline) -> Timeline {
    let rd = |tl: &Timeline, a, b| {
        edit::ripple_delete(tl, &RippleDelete { ranges: vec![(fr(a), fr(b))], markers: None, tracks: vec![], clips: vec![], linked: true })
            .unwrap()
            .0
    };
    let tl = rd(tl, 100, 100 * FPS);
    let tl = rd(&tl, 40, 60);
    rd(&tl, 10, 30)
}

#[test]
fn cuts_land_on_exact_frames_and_samples() {
    let d = dir("cuts");
    let src = d.join("src.mov");
    make_source(&src);
    let tl = jump_cut(&project(&src));
    assert_eq!(ve_render::timeline_end(&tl), fr(60));

    let out = d.join("out.mkv");
    render(&tl, Span::new(Q::from_integer(0), fr(60)), Q::from_integer(0), &out);
    let frames = frame_indices(&out);
    let want: Vec<i64> = (0..10).chain(30..40).chain(60..100).collect();
    assert_eq!(frames, want);

    let s = samples(&out);
    assert_eq!(s.len() as i64, 60 * SR / FPS, "exact sample count");
    // Away from cut points (hard cuts here), each output sample is the source sample it maps to.
    let spf = SR / FPS;
    for (out_frame, media_frame) in [(0, 0), (5, 5), (10, 30), (15, 35), (20, 60), (59, 99)] {
        for k in [0, 1, 7, spf - 1] {
            let j = (out_frame * spf + k) as usize;
            let want = saw(media_frame * spf + k);
            assert!((s[j] - want).abs() <= 1, "sample {j}: {} vs {want}", s[j]);
        }
    }
}

#[test]
fn speed_and_ranges() {
    let d = dir("speed");
    let src = d.join("src.mov");
    make_source(&src);
    let tl = project(&src);
    let tl = edit::ripple_delete(&tl, &RippleDelete { ranges: vec![(fr(20), fr(100))], markers: None, tracks: vec![], clips: vec![], linked: true }).unwrap().0;
    let v = tl.metadata.ve.assets.len();
    assert_eq!(v, 1);
    let clip = {
        let ve_model::otio::Composable::Track(t) = &tl.tracks.children[0] else { panic!() };
        let ve_model::otio::Composable::Clip(c) = &t.children[0] else { panic!() };
        c.metadata.ve.clip.as_ref().unwrap().id.clone()
    };
    let tl = edit::set_speed(&tl, &SetSpeed { clip, speed: Q::from_integer(2), linked: true, ripple: true }).unwrap().0;
    assert_eq!(ve_render::timeline_end(&tl), fr(10));

    let out = d.join("fast.mkv");
    render(&tl, Span::new(Q::from_integer(0), fr(10)), ve_render::default_crossfade(), &out);
    assert_eq!(frame_indices(&out), [0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
    assert_eq!(samples(&out).len() as i64, 10 * SR / FPS);

    // A sub-range starts mid-clip on the right frame.
    let out = d.join("part.mkv");
    render(&tl, Span::new(fr(3), fr(6)), Q::from_integer(0), &out);
    assert_eq!(frame_indices(&out), [6, 8, 10]);
}

#[test]
fn j_cut_moves_audio_only() {
    let d = dir("jcut");
    let src = d.join("src.mov");
    make_source(&src);
    let tl = project(&src);
    let tl = edit::split(&tl, &Split { at: fr(20), clips: vec![], tracks: vec![], linked: true }).unwrap().0;
    // Remove media 20..50 so the second shot starts at media 50.
    let tl = edit::ripple_delete(&tl, &RippleDelete { ranges: vec![(fr(20), fr(50))], markers: None, tracks: vec![], clips: vec![], linked: true }).unwrap().0;
    let tl = edit::ripple_delete(&tl, &RippleDelete { ranges: vec![(fr(40), fr(100))], markers: None, tracks: vec![], clips: vec![], linked: true }).unwrap().0;
    let audio_left = {
        let ve_model::otio::Composable::Track(t) = &tl.tracks.children[1] else { panic!() };
        let ve_model::otio::Composable::Clip(c) = &t.children[0] else { panic!() };
        c.metadata.ve.clip.as_ref().unwrap().id.clone()
    };
    // J cut: the second shot's sound starts 5 frames before its picture.
    let tl = edit::trim(&tl, &Trim { clip: audio_left, in_delta: None, out_delta: Some(fr(-5)), roll: true, linked: false }).unwrap().0;

    let out = d.join("j.mkv");
    render(&tl, Span::new(Q::from_integer(0), fr(40)), Q::from_integer(0), &out);
    let want: Vec<i64> = (0..20).chain(50..70).collect();
    assert_eq!(frame_indices(&out), want, "picture cut stays at frame 20");
    let s = samples(&out);
    let spf = SR / FPS;
    // At output frame 16 the sound is already the second shot's: media frame 50 - 4 = 46.
    let j = (16 * spf + 3) as usize;
    assert!((s[j] - saw(46 * spf + 3)).abs() <= 1);
    // Before the audio cut it is still the first shot.
    let j = (10 * spf + 3) as usize;
    assert!((s[j] - saw(10 * spf + 3)).abs() <= 1);
}

#[test]
fn frame_sheet_and_mp4_export() {
    let d = dir("outputs");
    let src = d.join("src.mov");
    make_source(&src);
    let tl = jump_cut(&project(&src));

    let img = ve_render::frame(&tl, fr(12), SIDE, ImageFormat::Png, &d).unwrap();
    let png = d.join("f.png");
    fs::write(&png, &img.bytes).unwrap();
    assert_eq!(frame_indices(&png), [32]);

    let sheet = ve_render::contact_sheet(&tl, Span::new(Q::from_integer(0), fr(60)), 6, None, 32, ImageFormat::Jpeg, &d).unwrap();
    assert_eq!((sheet.columns, sheet.rows), (3, 2));
    assert_eq!(sheet.times, [fr(5), fr(15), fr(25), fr(35), fr(45), fr(55)]);
    assert!(sheet.image.bytes.starts_with(&[0xFF, 0xD8]), "a JPEG");

    let settings = EncodeSettings {
        canvas: canvas(),
        format: ve_render::Format::Mp4,
        encoder: None,
        crf: 20,
        x264_preset: "veryfast".into(),
        bits_per_pixel: 0.1,
        audio_kbps: 128,
        crossfade: ve_render::default_crossfade(),
    };
    let mp4 = d.join("out.mp4");
    let e = ve_render::encode(&tl, Span::new(Q::from_integer(0), fr(60)), &settings, &mp4, &d, &ve_render::fx::Registry::for_project(None)).unwrap();
    assert_eq!(e.frames, 60);
    let probe = Command::new("ffprobe")
        .args(["-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0"])
        .arg(&mp4)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "60");
}

#[test]
fn crossfade_over_a_continuous_cut_is_transparent() {
    // MOV has sample-exact audio timestamps; MKV only milliseconds.
    for ext in ["mov", "mkv"] {
        crossfade_case(ext);
    }
}

fn crossfade_case(ext: &str) {
    let d = dir(&format!("xfade-{ext}"));
    let src = d.join(format!("src.{ext}"));
    make_source(&src);
    // A split with nothing removed: both sides play continuous media, so a
    // correctly aligned equal-gain crossfade must give back the source.
    let tl = project(&src);
    let tl = edit::split(&tl, &Split { at: fr(20), clips: vec![], tracks: vec![], linked: true }).unwrap().0;
    let out = d.join("x.mkv");
    render(&tl, Span::new(Q::from_integer(0), fr(40)), ve_render::default_crossfade(), &out);
    let s = samples(&out);
    let cut = 20 * SR / FPS;
    let mut worst = 0;
    for j in (cut - 1000)..(cut + 1000) {
        worst = worst.max((i32::from(s[j as usize]) - i32::from(saw(j))).abs());
    }
    assert!(worst <= 40, "{ext}: crossfade deviates by {worst} LSB around the cut");
}

