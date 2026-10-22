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

#[test]
fn effects_and_transitions_on_the_timeline() {
    use ve_ops::effects::{self, Align, NewTransition};
    use ve_render::decoder::{DecoderPool, key};
    let work = dir("gpu-fx");
    let ramp_src = ramp(&work);
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
