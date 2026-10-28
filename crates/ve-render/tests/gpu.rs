//! Golden test for the Phase 2 spike: frames composited on the GPU match the
//! Phase 1 ffmpeg renderer on the same timelines (cuts, a speed change, a
//! second video track with a pillarboxed clip whose bars are transparent).
//! Needs ffmpeg on PATH and a GPU or software adapter.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ve_model::Id;
use ve_model::meta::{Asset, AssetKind, OutputSettings};
use ve_model::otio::{Timeline, TrackKind};
use ve_model::time::{Q, Rate};
use ve_ops::edit::{self, Insert, RippleDelete, SetSpeed};
use ve_render::ImageFormat;

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

fn insert(asset: &str, track: Option<&str>, at: Option<Q>, duration: Option<Q>, overwrite: bool) -> Insert {
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
        streams: None,
        overwrite,
    }
}

/// V1: a gray ramp (luma 2N at frame N) cut and partly sped up. V2: a color
/// test pattern, 50x64, pillarboxed on the 64x64 canvas, over frames 10-29.
fn timeline(work: &Path) -> Timeline {
    let ramp = work.join("ramp.mkv");
    ffmpeg(&[
        "-f", "lavfi", "-i", &format!("nullsrc=s={SIDE}x{SIDE}:r={FPS},geq=lum='N*2':cb=128:cr=128"),
        "-t", "4", "-c:v", "libx264", "-qp", "0", "-g", "10", "-pix_fmt", "yuv420p", ramp.to_str().unwrap(),
    ]);
    let bars = work.join("bars.mkv");
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=50x64:r={FPS}"), "-t", "2", "-c:v", "ffv1", "-pix_fmt", "yuv420p", bars.to_str().unwrap()]);

    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("t", output);
    add_asset(&mut tl, "asset_ramp", &ramp);
    add_asset(&mut tl, "asset_bars", &bars);
    tl = edit::insert(&tl, &insert("asset_ramp", None, None, None, false)).unwrap().0;
    // Cut frames 40-49, so timeline frame 40 shows source frame 50.
    let rd = RippleDelete { ranges: vec![(fr(40), fr(50))], markers: None, tracks: vec![], clips: vec![], linked: true };
    tl = edit::ripple_delete(&tl, &rd).unwrap().0;
    // Double the speed of the clip after the cut (frame 40 on): V1's second clip.
    let ve_model::otio::Composable::Track(v1) = &tl.tracks.children[0] else { panic!("V1") };
    let id = v1
        .children
        .iter()
        .filter_map(|c| match c {
            ve_model::otio::Composable::Clip(c) => ve_ops::lane::clip_id(c).cloned(),
            _ => None,
        })
        .nth(1)
        .expect("two clips after the cut");
    tl = edit::set_speed(&tl, &SetSpeed { clip: id, speed: Q::from_integer(2), ripple: true, linked: true }).unwrap().0;
    tl = ve_ops::tracks::add(&tl, TrackKind::Video, None, None).unwrap().0;
    edit::insert(&tl, &insert("asset_bars", Some("V2"), Some(fr(10)), Some(fr(20)), true)).unwrap().0
}

fn fx() -> ve_render::fx::Registry {
    ve_render::fx::Registry::for_project(None)
}

fn rgb(png: &[u8]) -> image::RgbImage {
    image::load_from_memory(png).unwrap().to_rgb8()
}

/// (mean, max) absolute channel difference.
fn diff(a: &image::RgbImage, b: &image::RgbImage) -> (f64, u8) {
    assert_eq!(a.dimensions(), b.dimensions());
    let (mut sum, mut max) = (0u64, 0u8);
    for (x, y) in a.as_raw().iter().zip(b.as_raw()) {
        let d = x.abs_diff(*y);
        sum += u64::from(d);
        max = max.max(d);
    }
    (sum as f64 / a.as_raw().len() as f64, max)
}

#[test]
fn gpu_frames_match_the_ffmpeg_renderer() {
    let work = dir("gpu-golden");
    let tl = timeline(&work);
    eprintln!("adapter: {}", ve_render::gpu::Gpu::shared().unwrap().adapter);
    // V1 alone (0, 39), V2 over V1 (10, 20, 29), after the cut (40), sped up (45, 60).
    for n in [0, 10, 20, 29, 30, 39, 40, 45, 60] {
        let reference = ve_render::frame(&tl, fr(n), SIDE, ImageFormat::Png, &work).unwrap();
        let gpu = ve_render::compose::frame(&tl, fr(n), SIDE, ImageFormat::Png, &fx()).unwrap();
        let (a, b) = (rgb(&reference.bytes), rgb(&gpu.bytes));
        let (mean, max) = diff(&a, &b);
        let ssim = image_compare::rgb_hybrid_compare(&a, &b).unwrap().score;
        eprintln!("frame {n:>2}: mean diff {mean:.3}, max {max}, hybrid similarity {ssim:.4}");
        if n == 20 {
            let _ = fs::write(work.join("gpu-frame-20.png"), &gpu.bytes);
        }
        let v2 = (10..30).contains(&n);
        // Guard against a vacuous pass: V2's color pattern must be on screen
        // where expected, and the gray ramp must show the right frame.
        let center = b.get_pixel(32, 32).0;
        let colorful = center.iter().max().unwrap() - center.iter().min().unwrap() > 30;
        assert_eq!(colorful, v2, "frame {n}: V2 visible = {colorful}, center {center:?}");
        if v2 {
            // Color edges differ slightly: ffmpeg overlays in 4:2:0 YUV, the GPU in RGB.
            assert!(mean < 1.5 && ssim > 0.97, "frame {n}: mean {mean:.3}, similarity {ssim:.4}");
            // The pillarbox bars (x < 6) are transparent: V1 shows through on both.
            assert!(a.get_pixel(1, 32).0.abs_diff_max(b.get_pixel(1, 32).0) <= 1, "frame {n}: bar pixel");
        } else {
            // Gray ramp: one source frame apart is about 2 levels, so this pins the exact frame.
            assert!(max <= 1, "frame {n}: max diff {max}, mean {mean:.3}");
            // Luma 2N in limited range: which source frame is showing.
            let source = match n {
                0..40 => n,
                _ => 50 + (n - 40) * 2,
            };
            let level = f64::from(b.get_pixel(32, 32).0[0]);
            let expected = ((2.0 * source as f64) - 16.0).max(0.0) * 255.0 / 219.0;
            assert!((level - expected).abs() <= 2.5, "frame {n}: level {level}, expected source frame {source} ({expected:.1})");
        }
    }
}

trait AbsDiffMax {
    fn abs_diff_max(&self, other: [u8; 3]) -> u8;
}

impl AbsDiffMax for [u8; 3] {
    fn abs_diff_max(&self, other: [u8; 3]) -> u8 {
        (0..3).map(|i| self[i].abs_diff(other[i])).max().unwrap_or(0)
    }
}

fn ramp(work: &Path) -> PathBuf {
    let p = work.join("ramp.mkv");
    ffmpeg(&[
        "-f", "lavfi", "-i", &format!("nullsrc=s={SIDE}x{SIDE}:r={FPS},geq=lum='N*2':cb=128:cr=128"),
        "-t", "4", "-c:v", "libx264", "-qp", "0", "-g", "10", "-pix_fmt", "yuv420p", p.to_str().unwrap(),
    ]);
    p
}

/// Source frame index from a decoded gray ramp frame (luma 2N, limited range).
fn ramp_index(rgba: &[u8]) -> i64 {
    let v = f64::from(rgba[(32 * SIDE as usize + 32) * 4]);
    ((v * 219.0 / 255.0 + 16.0) / 2.0).round() as i64
}

#[test]
fn warm_decoders_reuse_streams_and_restart_on_jumps() {
    use ve_render::decoder::{DecoderPool, SKIP_LIMIT, key};
    let work = dir("gpu-warm");
    let src = ramp(&work);
    let k = key(&src, SIDE, SIDE, Rate::integer(FPS), 0);
    let mut pool = DecoderPool::default();
    // Frames below 8 are too dark to read back (luma under 16 clips to black).
    for n in [10, 11, 12, 15, 16] {
        assert_eq!(ramp_index(&pool.frame(&k, n).unwrap()), n);
    }
    assert_eq!(pool.starts, 1, "forward steps reuse one process");
    assert_eq!(ramp_index(&pool.frame(&k, 13).unwrap()), 13);
    assert_eq!(pool.starts, 2, "going back restarts");
    let far = 14 + SKIP_LIMIT + 1;
    assert_eq!(ramp_index(&pool.frame(&k, far).unwrap()), far);
    assert_eq!(pool.starts, 3, "a far jump restarts");
    // The same frame again (already read) restarts at that frame.
    assert_eq!(ramp_index(&pool.frame(&k, far).unwrap()), far);
    assert_eq!(pool.starts, 4);
    // Past the end of the 100-frame file: the last frame.
    assert_eq!(ramp_index(&pool.frame(&k, 150).unwrap()), 99);
    // A second layer on the same file keeps its own stream: interleaved
    // requests do not restart either one.
    let k2 = key(&src, SIDE, SIDE, Rate::integer(FPS), 1);
    assert_eq!(ramp_index(&pool.frame(&k, 20).unwrap()), 20);
    assert_eq!(ramp_index(&pool.frame(&k2, 60).unwrap()), 60);
    let before = pool.starts;
    for i in 1..5 {
        assert_eq!(ramp_index(&pool.frame(&k, 20 + i).unwrap()), 20 + i);
        assert_eq!(ramp_index(&pool.frame(&k2, 60 + i).unwrap()), 60 + i);
    }
    assert_eq!(pool.starts, before);
    assert_eq!(pool.open_count(), 2);
}

/// A 1080p 30 fps project with one 20 s clip of a color test pattern.
fn hd_project(work: &Path) -> Timeline {
    let src = work.join("hd.mp4");
    ffmpeg(&["-f", "lavfi", "-i", "testsrc2=s=1920x1080:r=30", "-t", "20", "-c:v", "libx264", "-preset", "veryfast", "-g", "60", "-pix_fmt", "yuv420p", src.to_str().unwrap()]);
    let output = OutputSettings { width: 1920, height: 1080, rate: Rate::integer(30), sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("hd", output);
    add_asset(&mut tl, "asset_hd", &src);
    edit::insert(&tl, &insert("asset_hd", None, None, None, false)).unwrap().0
}

#[test]
fn scaled_hd_frames_match() {
    let work = dir("gpu-hd");
    let tl = hd_project(&work);
    for n in [0, 31, 299] {
        let t = Q::new(n, 30);
        let a = rgb(&ve_render::frame(&tl, t, 640, ImageFormat::Png, &work).unwrap().bytes);
        let b = rgb(&ve_render::compose::frame(&tl, t, 640, ImageFormat::Png, &fx()).unwrap().bytes);
        let (mean, max) = diff(&a, &b);
        eprintln!("hd frame {n}: mean {mean:.3} max {max}");
        assert!(mean < 0.5, "hd frame {n}: mean {mean:.3}, max {max}");
    }
}

/// `cargo test -p ve-render --test gpu bench -- --ignored --nocapture`
#[test]
#[ignore = "benchmark; run by hand"]
fn bench_gpu_vs_ffmpeg() {
    use std::time::Instant;
    let work = dir("gpu-bench");
    let tl = hd_project(&work);
    let _ = ve_render::compose::frame(&tl, Q::from_integer(0), 640, ImageFormat::Jpeg, &fx()).unwrap(); // GPU setup
    for width in [640, 1920] {
        for (label, frames) in [("30 frames in a row", (300..330).collect::<Vec<i64>>()), ("10 random frames", vec![17, 412, 95, 333, 501, 8, 260, 577, 140, 455])] {
            for (name, gpu) in [("ffmpeg", false), ("gpu   ", true)] {
                let start = Instant::now();
                for &n in &frames {
                    let t = Q::new(n, 30);
                    if gpu {
                        ve_render::compose::frame(&tl, t, width, ImageFormat::Jpeg, &fx()).unwrap();
                    } else {
                        ve_render::frame(&tl, t, width, ImageFormat::Jpeg, &work).unwrap();
                    }
                }
                let per = start.elapsed().as_secs_f64() * 1000.0 / frames.len() as f64;
                eprintln!("{width:>4}px  {label:<20} {name} {per:6.1} ms/frame");
            }
        }
    }
}

fn lin(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn enc(l: f64) -> f64 {
    let l = l.clamp(0.0, 1.0);
    255.0 * if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }
}

/// Ramp frames 0-49 with +1 stop of exposure, a 20-frame dissolve at 50 into
/// a color clip. Returns the timeline and both sources.
fn fx_timeline(work: &Path) -> (Timeline, PathBuf, PathBuf) {
    use ve_ops::effects::{self, Align, NewTransition};
    let ramp_src = ramp(work);
    let color_src = work.join("color.mkv");
    ffmpeg(&["-f", "lavfi", "-i", &format!("color=c=0xC04020:s={SIDE}x{SIDE}:r={FPS}"), "-t", "2", "-c:v", "ffv1", "-pix_fmt", "yuv420p", color_src.to_str().unwrap()]);

    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("fx", output);
    add_asset(&mut tl, "asset_ramp", &ramp_src);
    add_asset(&mut tl, "asset_color", &color_src);
    // Ramp frames 0-49, then the color clip from its frame 25: both have handles.
    let mut a = insert("asset_ramp", None, None, None, false);
    a.source_out = Some(fr(50));
    tl = edit::insert(&tl, &a).unwrap().0;
    let mut b = insert("asset_color", None, Some(fr(50)), None, false);
    b.source_in = Some(fr(25));
    tl = edit::insert(&tl, &b).unwrap().0;
    let ve_model::otio::Composable::Track(v1) = &tl.tracks.children[0] else { panic!() };
    let ve_model::otio::Composable::Clip(first) = &v1.children[0] else { panic!() };
    let first = ve_ops::lane::clip_id(first).unwrap().clone();
    let stops = serde_json::json!({"stops": 1.0}).as_object().cloned().unwrap();
    tl = effects::add(&tl, &first, "core.color/exposure@1", stops, None).unwrap().0;
    let t = NewTransition {
        clip: None,
        at: Some(fr(50)),
        track: None,
        duration: fr(20),
        align: Align::Center,
        transition: "core.transition/dissolve@1".into(),
        params: serde_json::Map::new(),
    };
    tl = effects::add_transition(&tl, &t).unwrap().0;
    (tl, ramp_src, color_src)
}

#[test]
fn effects_and_transitions_on_the_timeline() {
    use ve_render::decoder::{DecoderPool, key};
    let work = dir("gpu-fx");
    let (tl, ramp_src, color_src) = fx_timeline(&work);

    // Source pixels, decoded the way the compositor decodes them.
    let mut pool = DecoderPool::default();
    let mut px = |src: &Path, n: i64| -> [u8; 3] {
        let f = pool.frame(&key(src, SIDE, SIDE, Rate::integer(FPS), 9), n).unwrap();
        let i = (32 * SIDE as usize + 32) * 4;
        [f[i], f[i + 1], f[i + 2]]
    };
    // (timeline frame, ramp frame, color frame, share of the color clip)
    for (n, ramp_f, color_f, mix) in [(30, 30, 0, 0.0), (45, 45, 20, 0.25), (50, 50, 25, 0.5), (59, 59, 34, 0.95), (60, 0, 35, 1.0), (70, 0, 45, 1.0)] {
        let (r, c) = (px(&ramp_src, ramp_f), px(&color_src, color_f));
        let expected: Vec<f64> = (0..3).map(|ch| enc((1.0 - mix) * 2.0 * lin(r[ch]) + mix * lin(c[ch]))).collect();
        let img = rgb(&ve_render::compose::frame(&tl, fr(n), SIDE, ImageFormat::Png, &fx()).unwrap().bytes);
        let got = img.get_pixel(32, 32).0;
        eprintln!("frame {n}: got {got:?}, expected {expected:.1?}");
        for ch in 0..3 {
            assert!((f64::from(got[ch]) - expected[ch]).abs() <= 2.0, "frame {n} channel {ch}: got {got:?}, expected {expected:.1?}");
        }
    }
    // The ffmpeg path still renders, as plain cuts, and says why.
    let plain = ve_render::frame(&tl, fr(45), SIDE, ImageFormat::Png, &work).unwrap();
    assert!(plain.warnings.iter().any(|w| w.contains("GPU compositor")), "{:?}", plain.warnings);
}

// ---- GPU export ----

fn settings(tl: &Timeline) -> ve_render::EncodeSettings {
    ve_render::EncodeSettings {
        canvas: ve_render::project_canvas(tl),
        format: ve_render::Format::Mp4,
        encoder: None,
        crf: 10,
        x264_preset: "veryfast".into(),
        bits_per_pixel: 0.3,
        audio_kbps: 128,
        crossfade: ve_render::default_crossfade(),
    }
}

/// Every frame of a video file as RGB.
fn decode_all(path: &Path) -> Vec<image::RgbImage> {
    let (w, h) = (SIDE, SIDE);
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("ffmpeg on PATH");
    assert!(out.status.success());
    out.stdout.chunks_exact((w * h * 3) as usize).map(|c| image::RgbImage::from_raw(w, h, c.to_vec()).unwrap()).collect()
}

/// Frame `n` of a video as raw yuv420p bytes, untouched by any color conversion.
fn yuv_frame(path: &Path, n: usize) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "yuv420p", "-"])
        .output()
        .expect("ffmpeg on PATH");
    let size = (SIDE * SIDE * 3 / 2) as usize;
    out.stdout[n * size..(n + 1) * size].to_vec()
}

/// The YUV matrix a file is tagged with (`unknown` when it says nothing).
fn matrix_tag(path: &Path) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v", "-show_entries", "stream=color_space", "-of", "csv=p=0"])
        .arg(path)
        .output()
        .expect("ffprobe on PATH");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Saturated colors survive the GPU export the same as the ffmpeg export,
/// for footage tagged BT.709 (phones, cameras) and for untagged footage.
/// Before this test, BT.709 footage lost ~20 levels of luma on the green bar.
/// The gray ramp cannot catch a matrix mistake: gray is the same in all of them.
#[test]
fn gpu_export_keeps_colors() {
    let work = dir("gpu-export-color");
    let tagged = work.join("bars709.mp4");
    ffmpeg(&[
        "-f", "lavfi", "-i", &format!("smptehdbars=s={SIDE}x{SIDE}:r={FPS}:d=1"),
        "-vf", "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv",
        "-c:v", "libx264", "-qp", "0", "-pix_fmt", "yuv420p",
        "-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", tagged.to_str().unwrap(),
    ]);
    let untagged = work.join("bars-untagged.mkv");
    ffmpeg(&[
        "-f", "lavfi", "-i", &format!("smptehdbars=s={SIDE}x{SIDE}:r={FPS}:d=1"),
        "-vf", "setparams=colorspace=unknown:color_primaries=unknown:color_trc=unknown",
        "-c:v", "ffv1", "-pix_fmt", "yuv420p", untagged.to_str().unwrap(),
    ]);
    assert_eq!(matrix_tag(&tagged), "bt709");
    assert_eq!(matrix_tag(&untagged), "unknown");
    for (name, src) in [("bt709", &tagged), ("untagged", &untagged)] {
        let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
        let mut tl = ve_model::doc::new_project("color", output);
        add_asset(&mut tl, "asset_bars", src);
        tl = edit::insert(&tl, &insert("asset_bars", None, None, None, false)).unwrap().0;
        let (a, b) = (work.join(format!("{name}-ffmpeg.mp4")), work.join(format!("{name}-gpu.mp4")));
        let s = settings(&tl);
        ve_render::encode(&tl, span_all(&tl), &s, &a, &work, &fx()).unwrap();
        ve_render::compose::encode(&tl, span_all(&tl), &s, &b, &work, &fx()).unwrap();
        assert_eq!(matrix_tag(&b), "bt709", "{name}: the GPU export says what it is");
        // Decoded by their tags (untagged: BT.601, ffmpeg's default and the engine's).
        let (fa, fb) = (decode_all(&a), decode_all(&b));
        assert_eq!(fa.len(), fb.len());
        let (mean, max) = diff(&fa[10], &fb[10]);
        eprintln!("{name}: mean {mean:.3}, max {max}");
        // A BT.601/709 mix-up moves whole saturated bars by 10-20 levels; bar
        // edges differ a little more from chroma subsampling either way.
        assert!(mean < 2.0, "{name}: mean {mean:.3}, max {max}");
        if name == "bt709" {
            // The ffmpeg path passes the source's YUV through; the GPU export
            // must land on the same numbers, whatever a player assumes.
            let (ya, yb) = (yuv_frame(&a, 10), yuv_frame(&b, 10));
            let mean = ya.iter().zip(&yb).map(|(x, y)| f64::from(x.abs_diff(*y))).sum::<f64>() / ya.len() as f64;
            eprintln!("{name}: yuv mean {mean:.3}");
            assert!(mean < 1.5, "{name}: yuv mean {mean:.3}");
        }
    }
}

fn span_all(tl: &Timeline) -> ve_render::Span {
    ve_render::span_or_all(tl, None, None).unwrap()
}

#[test]
fn gpu_export_matches_the_ffmpeg_export() {
    let work = dir("gpu-export-parity");
    let tl = timeline(&work);
    assert!(!ve_render::needs_gpu(&tl, span_all(&tl)).unwrap(), "a plain project stays on ffmpeg");
    let (a, b) = (work.join("ffmpeg.mp4"), work.join("gpu.mp4"));
    let s = settings(&tl);
    let ea = ve_render::encode(&tl, span_all(&tl), &s, &a, &work, &fx()).unwrap();
    let eb = ve_render::compose::encode(&tl, span_all(&tl), &s, &b, &work, &fx()).unwrap();
    assert_eq!(ea.frames, eb.frames);
    let (fa, fb) = (decode_all(&a), decode_all(&b));
    assert_eq!(fa.len(), fb.len(), "same frame count");
    assert_eq!(fa.len() as i64, ea.frames);
    for (n, (x, y)) in fa.iter().zip(&fb).enumerate() {
        let (mean, max) = diff(x, y);
        // V2's color edges differ between a YUV and an RGB blend (plus codec noise).
        let limit = if (10..30).contains(&n) { 3.0 } else { 1.05 };
        assert!(mean < limit, "frame {n}: mean {mean:.3}, max {max}");
        if !(10..30).contains(&n) {
            // Codec noise plus rounding: the GPU export goes back to YUV with
            // BT.709, the source came in by its own matrix.
            assert!(max <= 6, "frame {n}: max {max}");
        }
    }
}

#[test]
fn gpu_export_shows_effects_and_transitions() {
    let work = dir("gpu-export-fx");
    let (tl, _, _) = fx_timeline(&work);
    assert!(ve_render::needs_gpu(&tl, span_all(&tl)).unwrap(), "effects and transitions pick the GPU");
    let (plain, fancy) = (work.join("plain.mp4"), work.join("fancy.mp4"));
    let s = settings(&tl);
    ve_render::encode(&tl, span_all(&tl), &s, &plain, &work, &fx()).unwrap();
    ve_render::compose::encode(&tl, span_all(&tl), &s, &fancy, &work, &fx()).unwrap();
    let (fp, ff) = (decode_all(&plain), decode_all(&fancy));
    assert_eq!(fp.len(), ff.len());
    // Golden: each exported frame equals the compositor's own frame, within codec noise.
    for n in [30, 45, 50, 59, 60, 70] {
        let golden = rgb(&ve_render::compose::frame(&tl, fr(n), SIDE, ImageFormat::Png, &fx()).unwrap().bytes);
        let (mean, max) = diff(&golden, &ff[n as usize]);
        assert!(mean < 2.0, "frame {n}: mean {mean:.3}, max {max}");
    }
    // Not vacuous: +1 stop of exposure brightens frame 30, and the dissolve
    // is neither clip alone at its midpoint.
    let center = |f: &image::RgbImage| f.get_pixel(32, 32).0[0];
    assert!(i32::from(center(&ff[30])) > i32::from(center(&fp[30])) + 10, "exposure missing: {} vs {}", center(&ff[30]), center(&fp[30]));
    let mid = ff[50].get_pixel(32, 32).0;
    assert!(mid != fp[50].get_pixel(32, 32).0 && mid != ff[70].get_pixel(32, 32).0, "dissolve missing: {mid:?}");
}

#[test]
fn gpu_export_progress_and_cancel() {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use ve_render::control::{Control, scoped};

    let work = dir("gpu-export-cancel");
    let tl = hd_project(&work); // 20 s of 1080p: far too long to finish in the test
    let out = work.join("cancelled.mp4");
    let cancel = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(Vec::<f64>::new()));
    let last = Arc::new(AtomicU64::new(0));
    let ctrl = {
        let (seen, last) = (seen.clone(), last.clone());
        Control::new(cancel.clone(), move |secs| {
            seen.lock().unwrap().push(secs);
            last.store(secs.to_bits(), Ordering::SeqCst);
        })
    };
    let flag = cancel.clone();
    let seen2 = seen.clone();
    let stopper = std::thread::spawn(move || {
        // Cancel once some frames are through.
        let t = Instant::now();
        while seen2.lock().unwrap().len() < 15 && t.elapsed() < Duration::from_secs(60) {
            std::thread::sleep(Duration::from_millis(20));
        }
        flag.store(true, Ordering::SeqCst);
    });
    let s = settings(&tl);
    let started = Instant::now();
    let r = scoped(Arc::new(ctrl), || ve_render::compose::encode(&tl, span_all(&tl), &s, &out, &work, &fx()));
    stopper.join().unwrap();
    assert!(matches!(r, Err(ve_render::RenderError::Cancelled)), "{:?}", r.err());
    assert!(started.elapsed() < Duration::from_secs(30), "cancel took {:?}", started.elapsed());
    assert!(!out.exists(), "a cancelled export leaves no file");
    let seen = seen.lock().unwrap();
    assert!(seen.windows(2).all(|w| w[1] > w[0]), "progress only moves forward");
    assert!(seen.last().copied().unwrap_or(0.0) < 20.0, "stopped before the end");
}

/// 10-minute 1080p 29.97 talking-head stand-in with 160 jump cuts: ffmpeg
/// export vs GPU export (same encoder settings), then GPU with an effect.
/// `cargo test -p ve-render --test gpu bench_export -- --ignored --nocapture`
#[test]
#[ignore = "benchmark; takes many minutes"]
fn bench_export() {
    use std::time::Instant;
    let work = std::env::temp_dir().join("ve-render-tests").join("bench-export");
    fs::create_dir_all(&work).unwrap();
    let src = work.join("talk.mp4");
    if !src.exists() {
        ffmpeg(&[
            "-f", "lavfi", "-i", "testsrc2=s=1920x1080:r=30000/1001:d=600",
            "-f", "lavfi", "-i", "sine=frequency=220:sample_rate=48000:d=600",
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30", "-g", "60", "-pix_fmt", "yuv420p", "-c:a", "aac", src.to_str().unwrap(),
        ]);
    }
    let rate = Rate::new(30000, 1001);
    let output = OutputSettings { width: 1920, height: 1080, rate, sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("bench", output);
    add_asset(&mut tl, "asset_talk", &src);
    let mut a = insert("asset_talk", None, None, None, false);
    a.audio = true;
    tl = edit::insert(&tl, &a).unwrap().0;
    // 160 pauses of 15 frames, spread over the first ~9.5 minutes (cut back to front so times stay valid).
    let ranges: Vec<(Q, Q)> = (1..=160).map(|k| (Q::new(k * 1001 * 107, 30000), Q::new(k * 1001 * 107 + 1001 * 15, 30000))).collect();
    let rd = RippleDelete { ranges, markers: None, tracks: vec![], clips: vec![], linked: true };
    tl = edit::ripple_delete(&tl, &rd).unwrap().0;
    let span = span_all(&tl);
    let frames = (span.duration() * rate.as_ratio()).round().to_integer();
    eprintln!("timeline: {frames} frames ({:.1} s)", frames as f64 / rate.to_f64());

    let mut s = settings(&tl);
    s.crf = 20;
    s.x264_preset = "medium".into();
    let report = |name: &str, t: Instant, out: &Path| {
        let secs = t.elapsed().as_secs_f64();
        eprintln!("{name:<22} {secs:7.1} s  {:6.1} ms/frame  {:5.1} fps  {} MB", secs * 1000.0 / frames as f64, frames as f64 / secs, fs::metadata(out).map_or(0, |m| m.len() / 1_000_000));
    };
    let o1 = work.join("ffmpeg.mp4");
    let t = Instant::now();
    ve_render::encode(&tl, span, &s, &o1, &work, &fx()).unwrap();
    report("ffmpeg export", t, &o1);
    let o2 = work.join("gpu.mp4");
    let t = Instant::now();
    ve_render::compose::encode(&tl, span, &s, &o2, &work, &fx()).unwrap();
    report("gpu export", t, &o2);
    // Encoder alone: what the GPU path can never beat.
    let o3 = work.join("encode-only.mp4");
    let t = Instant::now();
    ffmpeg(&["-f", "lavfi", "-i", "testsrc2=s=1920x1080:r=30000/1001", "-frames:v", &frames.to_string(), "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p", o3.to_str().unwrap()]);
    report("x264 alone (floor)", t, &o3);
    let _ = (fs::remove_file(&o1), fs::remove_file(&o2), fs::remove_file(&o3));
}

/// A frame that cannot render (here an effect nobody has installed) stops the
/// export at once with that error, not ffmpeg's, and leaves no file.
#[test]
fn gpu_export_reports_render_errors() {
    use std::time::{Duration, Instant};
    let work = dir("gpu-export-error");
    let mut tl = hd_project(&work);
    let ve_model::otio::Composable::Track(v1) = &tl.tracks.children[0] else { panic!("V1") };
    let ve_model::otio::Composable::Clip(clip) = &v1.children[0] else { panic!("a clip") };
    let id = ve_ops::lane::clip_id(clip).unwrap().clone();
    tl = ve_ops::effects::add(&tl, &id, "core.color/not-installed@1", serde_json::Map::new(), None).unwrap().0;
    let out = work.join("broken.mp4");
    let started = Instant::now();
    let r = ve_render::compose::encode(&tl, span_all(&tl), &settings(&tl), &out, &work, &fx());
    let Err(ve_render::RenderError::Invalid(msg)) = r else { panic!("expected the effect error, got {:?}", r.err()) };
    assert!(msg.contains("not-installed"), "{msg}");
    assert!(started.elapsed() < Duration::from_secs(20), "took {:?}", started.elapsed());
    assert!(!out.exists(), "no file is left behind");
}

/// Picture in picture: V2 at half size in the bottom-right quarter over V1,
/// fading in with an opacity keyframe.
#[test]
fn transforms_place_and_fade_a_layer() {
    use ve_model::meta::Keyframe;
    use ve_model::time::RationalTime;
    use ve_ops::transform::{self, TransformChange};
    let work = dir("gpu-transform");
    let red = work.join("red.mkv");
    let blue = work.join("blue.mkv");
    for (path, c) in [(&red, "red"), (&blue, "blue")] {
        ffmpeg(&["-f", "lavfi", "-i", &format!("color=c={c}:s={SIDE}x{SIDE}:r={FPS}"), "-t", "1", "-c:v", "ffv1", "-pix_fmt", "yuv420p", path.to_str().unwrap()]);
    }
    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("pip", output);
    add_asset(&mut tl, "asset_red", &red);
    add_asset(&mut tl, "asset_blue", &blue);
    tl = edit::insert(&tl, &insert("asset_red", None, None, None, false)).unwrap().0;
    tl = ve_ops::tracks::add(&tl, TrackKind::Video, None, None).unwrap().0;
    let (next, r) = edit::insert(&tl, &insert("asset_blue", Some("V2"), Some(fr(0)), None, true)).unwrap();
    tl = next;
    let top = r.created[0].id.clone();
    let key = |f: i64, v: f64| Keyframe { t: RationalTime::from_frames(f, Rate::integer(FPS)), v: v.into(), ease: "linear".into(), r#in: None, out: None };
    let change = TransformChange {
        reset: false,
        values: vec![("scale".into(), Some(0.5)), ("x".into(), Some(0.25)), ("y".into(), Some(0.25))],
        keyframes: [("opacity".to_owned(), vec![key(0, 0.0), key(10, 1.0)])].into_iter().collect(),
        blend: None,
    };
    tl = transform::set(&tl, &top, &change).unwrap().0;
    assert!(ve_render::needs_gpu(&tl, ve_render::Span::new(fr(0), fr(1))).unwrap());

    let at = |n: i64| rgb(&ve_render::compose::frame(&tl, fr(n), SIDE, ImageFormat::Png, &fx()).unwrap().bytes);
    let close = |got: [u8; 3], want: [f64; 3]| (0..3).all(|c| (f64::from(got[c]) - want[c]).abs() <= 3.0);
    // Fully in: V1 shows top left, V2 fills the bottom-right quarter.
    let img = at(15);
    let (r, b) = (img.get_pixel(16, 16).0, img.get_pixel(48, 48).0);
    assert!(r[0] > 200 && r[2] < 40, "{r:?}");
    assert!(b[2] > 200 && b[0] < 40, "{b:?}");
    assert_eq!(img.get_pixel(16, 48).0, r, "outside the small picture is V1");
    // Halfway through the fade: an even mix in linear light.
    let mid = at(5).get_pixel(48, 48).0;
    let want: [f64; 3] = std::array::from_fn(|c| enc(0.5 * lin(r[c]) + 0.5 * lin(b[c])));
    assert!(close(mid, want), "got {mid:?}, want {want:.1?}");
    // At frame 0 it is invisible.
    assert_eq!(at(0).get_pixel(48, 48).0, r);
    // The ffmpeg path draws it plain and says so.
    let plain = ve_render::frame(&tl, fr(15), SIDE, ImageFormat::Png, &work).unwrap();
    assert!(plain.warnings.iter().any(|w| w.contains("transform")), "{:?}", plain.warnings);
}

/// Keying a whole range from one plan gives the same keys as keying each frame
/// alone (the frame cache depends on it), across a cut, a transition and an effect.
#[test]
fn range_keys_match_single_frame_keys() {
    let work = dir("frame-keys");
    let (tl, _, _) = fx_timeline(&work);
    let total = (ve_render::timeline_end(&tl) * Q::from_integer(FPS)).to_integer();
    let all = ve_render::compose::frame_keys(&tl, 0, total, SIDE, ImageFormat::Jpeg, &fx()).unwrap();
    assert_eq!(all.len() as i64, total);
    for f in 0..total {
        let one = ve_render::compose::frame_key(&tl, fr(f), SIDE, ImageFormat::Jpeg, &fx()).unwrap();
        assert_eq!(all[f as usize], one, "frame {f}");
    }
    // A sub-range keys the same frames the same way.
    let part = ve_render::compose::frame_keys(&tl, 40, 70, SIDE, ImageFormat::Jpeg, &fx()).unwrap();
    assert_eq!(part, all[40..70]);
    // Different frames differ (the ramp changes every frame).
    assert_ne!(all[10], all[11]);
}

/// A 64x64 PNG: left half opaque red, right half fully transparent.
fn half_red_png(work: &Path) -> PathBuf {
    let png = work.join("half red.png");
    ffmpeg(&[
        "-f", "lavfi", "-i", "color=red:s=32x64,format=rgba",
        "-f", "lavfi", "-i", "color=black@0:s=32x64,format=rgba",
        "-filter_complex", "[0][1]hstack,format=rgba", "-frames:v", "1", png.to_str().unwrap(),
    ]);
    png
}

#[test]
fn still_images_render_with_transparency_on_both_paths() {
    let work = dir("stills");
    let ramp = ramp(&work);
    let png = half_red_png(&work);
    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
    let mut tl = ve_model::doc::new_project("t", output);
    add_asset(&mut tl, "asset_ramp", &ramp);
    add_asset(&mut tl, "asset_png", &png);
    assert!(tl.metadata.ve.assets[1].media.as_ref().unwrap().video.as_ref().unwrap().still);
    tl = edit::insert(&tl, &insert("asset_ramp", None, None, None, false)).unwrap().0;
    tl = ve_ops::tracks::add(&tl, TrackKind::Video, None, None).unwrap().0;
    // The image over frames 10-59 on V2.
    tl = edit::insert(&tl, &insert("asset_png", Some("V2"), Some(fr(10)), Some(fr(50)), true)).unwrap().0;

    for n in [5, 10, 30, 59, 60] {
        let reference = rgb(&ve_render::frame(&tl, fr(n), SIDE, ImageFormat::Png, &work).unwrap().bytes);
        let gpu = rgb(&ve_render::compose::frame(&tl, fr(n), SIDE, ImageFormat::Png, &fx()).unwrap().bytes);
        let (mean, max) = diff(&reference, &gpu);
        eprintln!("frame {n}: mean diff {mean:.3}, max {max}");
        let shown = (10..60).contains(&n);
        for img in [&reference, &gpu] {
            let left = img.get_pixel(8, 32).0;
            let right = img.get_pixel(56, 32).0;
            if shown {
                assert!(left[0] > 200 && left[1] < 60 && left[2] < 60, "frame {n}: left should be red, got {left:?}");
            }
            // The transparent half (and the whole frame without the image) shows the gray ramp.
            let gray = right.iter().max().unwrap() - right.iter().min().unwrap() <= 4;
            assert!(gray, "frame {n}: right should be the gray ramp, got {right:?}");
        }
        // Opaque and fully transparent areas agree; only the red/clear edge may differ.
        assert!(mean < 2.0, "frame {n}: mean {mean:.3}");
    }
    // The still always keys the same source frame, so the frame cache can share it.
    let k = |n| ve_render::compose::frame_key(&tl, fr(n), SIDE, ImageFormat::Png, &fx()).unwrap();
    assert_ne!(k(10), k(11), "the ramp under it still changes");

    // An image alone on V1 sits on black, and exports exactly its frames.
    let output = OutputSettings { width: SIDE, height: SIDE, rate: Rate::integer(FPS), sample_rate: 48_000, color_space: "bt709".into() };
    let mut solo = ve_model::doc::new_project("t", output);
    add_asset(&mut solo, "asset_png", &png);
    solo = edit::insert(&solo, &insert("asset_png", None, None, Some(fr(30)), false)).unwrap().0;
    let f = rgb(&ve_render::frame(&solo, fr(3), SIDE, ImageFormat::Png, &work).unwrap().bytes);
    assert!(f.get_pixel(8, 32).0[0] > 200, "red on the left");
    assert!(f.get_pixel(56, 32).0.iter().all(|&c| c < 20), "black where the image is clear");
    let out = work.join("solo.mp4");
    let enc = ve_render::encode(&solo, span_all(&solo), &settings(&solo), &out, &work, &fx()).unwrap();
    assert_eq!(enc.frames, 30);
    assert_eq!(decode_all(&out).len(), 30);
}
