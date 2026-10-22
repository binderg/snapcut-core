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
        let gpu = ve_render::compose::frame(&tl, fr(n), SIDE, ImageFormat::Png, &work).unwrap();
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
