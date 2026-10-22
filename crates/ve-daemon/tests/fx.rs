//! `fx.define` with image files: dry run, save, reload, use on a clip.
//! Needs ffmpeg on PATH and a GPU or software adapter.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine;
use serde_json::{Value, json};
use ve_protocol::Registry;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ve-daemon-tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// 1 s of plain white at 25 fps.
fn make_white(path: &Path) {
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=white:s=160x120:r=25:d=1"])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(path)
        .status()
        .expect("ffmpeg on PATH");
    assert!(st.success());
}

async fn call(r: &Registry, method: &str, params: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    serde_json::from_str(&r.dispatch(&req).await).unwrap()
}

async fn ok(r: &Registry, method: &str, params: Value) -> Value {
    let resp = call(r, method, params.clone()).await;
    assert!(resp.get("error").is_none(), "{method} {params}: {resp}");
    resp["result"].clone()
}

async fn err(r: &Registry, method: &str, params: Value) -> String {
    let resp = call(r, method, params).await;
    assert_eq!(resp["error"]["code"], -32602, "{method}: {resp}");
    resp["error"]["message"].as_str().unwrap().to_owned()
}

/// A 2x1 PNG: black on the left, white on the right.
fn black_white_png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(2, 1, |x, _| if x == 0 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[tokio::test]
async fn define_with_an_image_saves_reloads_and_renders() {
    let dir = scratch("fx-define-image");
    make_white(&dir.join("white.mp4"));
    let r = ve_daemon::build_registry();
    let root = dir.join("edit");
    ok(&r, "project.init", json!({"path": root, "fps": "25", "width": 160, "height": 120})).await;
    let asset = ok(&r, "media.import", json!({"paths": [dir.join("white.mp4")], "base_rev": 0})).await["assets"][0]["id"].clone();
    ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;

    let png = black_white_png();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    let manifest = json!({
        "id": "project.mask", "version": "1.0.0", "kind": "effect", "name": "Mask", "shader": "effect.wgsl",
        "params": {"type": "object", "properties": {"map": {"type": "string", "format": "image", "default": "map.png"}}}
    });
    // Multiplies the clip by the map: dark left half, light right half.
    let source = "fn effect(uv: vec2<f32>) -> vec4<f32> {\n    return src(uv) * vec4<f32>(map(uv).rgb, 1.0);\n}\n";
    let fx_dir = root.join("effects").join("mask");

    // Without the file, or with a broken one, the define is refused.
    let e = err(&r, "fx.define", json!({"manifest": manifest, "source": source, "dry_run": true})).await;
    assert!(e.contains("no image `map.png`"), "{e}");
    let e = err(&r, "fx.define", json!({"manifest": manifest, "source": source, "files": {"map.png": "!!"}, "dry_run": true})).await;
    assert!(e.contains("not base64"), "{e}");
    let e = err(&r, "fx.define", json!({"manifest": manifest, "source": source, "files": {"map.png": "aGVsbG8="}, "dry_run": true})).await;
    assert!(e.contains("image `map.png`"), "{e}");
    let e = err(&r, "fx.define", json!({"manifest": manifest, "source": source, "files": {"../map.png": b64}, "dry_run": true})).await;
    assert!(e.contains("plain file name"), "{e}");

    // Dry run: validated and test-rendered, nothing written.
    let dry = ok(&r, "fx.define", json!({"manifest": manifest, "source": source, "files": {"map.png": b64}, "dry_run": true})).await;
    assert_eq!(dry["effect"]["id"], "project.mask");
    assert!(dry.get("path").is_none(), "{dry}");
    assert!(!fx_dir.exists());

    // For real: the image lands next to effect.json.
    let done = ok(&r, "fx.define", json!({"manifest": manifest, "source": source, "files": {"map.png": b64}})).await;
    assert_eq!(done["path"].as_str().map(PathBuf::from), Some(fx_dir.clone()), "{done}");
    assert!(fx_dir.join("effect.json").is_file());
    assert_eq!(fs::read_to_string(fx_dir.join("effect.wgsl")).unwrap(), source);
    assert_eq!(fs::read(fx_dir.join("map.png")).unwrap(), png);

    // The project effect reloads from disk with its image.
    let list = ok(&r, "fx.list", json!({})).await;
    assert!(list.get("problems").is_none(), "{list}");
    let mine = list["effects"].as_array().unwrap().iter().find(|f| f["id"] == "project.mask").cloned().unwrap();
    assert_eq!(mine["origin"], "project");
    let reg = ve_render::fx::Registry::for_project(Some(&root));
    let fx = reg.get("project.mask").unwrap();
    assert_eq!(fx.images["map.png"].width, 2);

    // Clips can use it; image values must name a file it has.
    let tl = ok(&r, "project.get", json!({})).await;
    let clip = tl["timeline"]["tracks"]["children"][0]["children"][0]["metadata"]["ve"]["clip"]["id"].clone();
    let e = err(&r, "effect.add", json!({"clip": clip, "effect": "project.mask", "params": {"map": "nope.png"}, "base_rev": 2})).await;
    assert!(e.contains("no image `nope.png`"), "{e}");
    ok(&r, "effect.add", json!({"clip": clip, "effect": "project.mask", "params": {"map": "map.png"}, "base_rev": 2})).await;

    // And it renders: white footage through the map is black on the left, white on the right.
    let f = ok(&r, "render.frame", json!({"at": "0s", "width": 160, "format": "png"})).await;
    let bytes = base64::engine::general_purpose::STANDARD.decode(f["data"].as_str().unwrap()).unwrap();
    let img = image::load_from_memory(&bytes).unwrap().to_rgb8();
    let (left, right) = (img.get_pixel(10, 60).0, img.get_pixel(150, 60).0);
    assert!(left.iter().all(|&c| c < 20), "left {left:?}");
    assert!(right.iter().all(|&c| c > 235), "right {right:?}");
}
