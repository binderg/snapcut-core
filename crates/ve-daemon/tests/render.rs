//! render.* and analyze.* through the registry: the agent loop on a small
//! "talking head" (tone with pauses) plus a hard picture cut.

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

/// 6 s at 25 fps: red for 3 s then blue (one hard cut), with a tone that
/// pauses for 1 s at 1.5-2.5 s and 4-5 s.
fn make_talk(path: &Path) {
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=red:s=160x120:r=25:d=3", "-f", "lavfi", "-i", "color=blue:s=160x120:r=25:d=3"])
        .args(["-f", "lavfi", "-i", "aevalsrc='0.5*sin(2*PI*300*t)*(lt(t\\,1.5)+between(t\\,2.5\\,4)+gte(t\\,5))':s=48000:d=6"])
        .args(["-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0[v]", "-map", "[v]", "-map", "2:a"])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"])
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

#[tokio::test]
async fn analyze_cut_and_look() {
    let dir = scratch("render-loop");
    make_talk(&dir.join("talk.mp4"));
    let r = ve_daemon::build_registry();
    ok(&r, "project.init", json!({"path": dir.join("edit"), "fps": "25", "width": 160, "height": 120})).await;
    let asset = ok(&r, "media.import", json!({"paths": [dir.join("talk.mp4")], "base_rev": 0})).await["assets"][0]["id"].clone();
    ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;

    // Silences: two 1 s pauses, shrunk by 0.1 s padding each side.
    let s = ok(&r, "analyze.silence", json!({"min_duration": "0.5s", "pad": "0.1s", "base_rev": 2})).await;
    let ranges = s["ranges"].as_array().unwrap();
    assert_eq!(ranges.len(), 2, "{s}");
    assert_eq!(ranges[0]["start"], "00:00:01:15", "{s}");
    assert_eq!(ranges[0]["end"], "00:00:02:10", "{s}");
    assert_eq!(s["markers"].as_array().unwrap().len(), 2);
    assert_eq!(s["revision"], 3);

    // Re-running replaces the markers rather than adding more.
    let again = ok(&r, "analyze.silence", json!({"min_duration": "0.5s", "pad": "0.1s", "base_rev": 3})).await;
    assert_eq!(again["markers"].as_array().unwrap().len(), 2);
    let text = ok(&r, "project.summary", json!({})).await["text"].as_str().unwrap().to_owned();
    assert!(text.contains("2 markers"), "{text}");

    // Scene cut at 3 s.
    let sc = ok(&r, "analyze.scenes", json!({"base_rev": 4})).await;
    assert_eq!(sc["ranges"].as_array().unwrap().len(), 1, "{sc}");
    assert_eq!(sc["ranges"][0]["start"], "00:00:03:00");

    // Cut the silences: 150 frames minus 20 and 19 (AAC softens the second edge).
    let cut = ok(&r, "edit.ripple_delete", json!({"markers": "silence", "base_rev": 5})).await;
    assert_eq!(cut["duration"], "00:00:04:11", "{cut}");

    // Look at the result.
    let f = ok(&r, "render.frame", json!({"at": "00:00:00:10", "width": 160})).await;
    assert_eq!(f["mime"], "image/jpeg");
    let jpg = base64::engine::general_purpose::STANDARD.decode(f["data"].as_str().unwrap()).unwrap();
    assert!(jpg.starts_with(&[0xFF, 0xD8]));
    let resp = call(&r, "render.frame", json!({"at": "00:00:10:00"})).await;
    assert_eq!(resp["error"]["code"], -32602, "past the end is a bad param: {resp}");

    let sheet = ok(&r, "render.contact_sheet", json!({"count": 4, "width": 80, "format": "png"})).await;
    assert_eq!(sheet["times"].as_array().unwrap().len(), 4);
    assert_eq!((sheet["columns"].as_u64(), sheet["rows"].as_u64()), (Some(2), Some(2)));

    let prev = ok(&r, "render.range_preview", json!({"from": "1s", "to": "3s", "width": 160})).await;
    assert_eq!(prev["frames"], 50);
    let path = PathBuf::from(prev["path"].as_str().unwrap());
    assert!(path.is_file() && prev["size_bytes"].as_u64().unwrap() > 0);

    let presets = ok(&r, "render.presets", json!({})).await;
    assert!(presets["presets"].as_array().unwrap().iter().any(|p| p["name"] == "youtube-1080p"));
    let ex = ok(&r, "render.export", json!({"preset": "draft-540p"})).await;
    assert_eq!(ex["frames"], 111);
    assert_eq!((ex["width"].as_u64(), ex["height"].as_u64()), (Some(960), Some(540)));
    let resp = call(&r, "render.export", json!({"preset": "imax"})).await;
    assert!(resp["error"]["message"].as_str().unwrap().contains("presets are"), "{resp}");

    // Missing media is a render failure (exit 4), named by clip.
    fs::remove_file(dir.join("talk.mp4")).unwrap();
    let resp = call(&r, "render.frame", json!({"at": "0s"})).await;
    assert_eq!(resp["error"]["code"], -31001, "{resp}");
    assert!(resp["error"]["message"].as_str().unwrap().contains("missing"), "{resp}");
}

/// `render.frame` keeps frames by what is in them: an unrelated edit or a
/// ripple that only shifts a frame in time still hits; changing the frame misses.
/// `ui.cache_hint` then renders the rest of the timeline in the background.
#[tokio::test]
async fn frame_cache_survives_unrelated_edits() {
    // A fresh project path each run: cached frames live in the OS cache by project path.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = scratch(&format!("frame-cache-{stamp}"));
    make_talk(&dir.join("talk.mp4"));
    let r = ve_daemon::build_registry();
    ok(&r, "project.init", json!({"path": dir.join("edit"), "fps": "25", "width": 160, "height": 120})).await;
    let asset = ok(&r, "media.import", json!({"paths": [dir.join("talk.mp4")], "base_rev": 0})).await["assets"][0]["id"].clone();
    let ins = ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;
    let video = ins["created"].as_array().unwrap().iter().find(|c| c["track"] == "V1").unwrap()["id"].clone();
    let frame = |at: &str| json!({"at": at, "width": 160});
    let cached = |v: &Value| v["cached"].as_bool().unwrap_or(false);

    let first = ok(&r, "render.frame", frame("f80")).await;
    assert!(!cached(&first));
    let again = ok(&r, "render.frame", frame("f80")).await;
    assert!(cached(&again));
    assert_eq!(first["data"], again["data"]);

    // Volume does not change the picture.
    ok(&r, "edit.set_volume", json!({"clip": video, "db": -6, "base_rev": 2})).await;
    assert!(cached(&ok(&r, "render.frame", frame("f80")).await));

    // Cutting the first second moves source frame 80 to timeline frame 55: still cached.
    ok(&r, "edit.ripple_delete", json!({"ranges": [{"from": "f0", "to": "f25"}], "base_rev": 3})).await;
    let shifted = ok(&r, "render.frame", frame("f55")).await;
    assert!(cached(&shifted));
    assert_eq!(shifted["data"], first["data"]);

    // A transform changes it.
    let clip = ok(&r, "project.summary", json!({})).await["text"].as_str().unwrap().lines().find(|l| l.contains("clip_") && l.contains("talk")).unwrap().split_whitespace().find(|w| w.starts_with("clip_")).unwrap().to_owned();
    ok(&r, "edit.set_transform", json!({"clip": clip, "scale": 0.5, "base_rev": 4})).await;
    assert!(!cached(&ok(&r, "render.frame", frame("f55")).await));

    // Playback previews are kept by content too.
    let preview = json!({"from": "f0", "to": "f50", "width": 160});
    let p1 = ok(&r, "render.range_preview", preview.clone()).await;
    assert!(!cached(&p1), "{p1}");
    let p2 = ok(&r, "render.range_preview", preview.clone()).await;
    assert!(cached(&p2), "{p2}");
    assert_eq!(p1["path"], p2["path"]);
    assert!(std::path::Path::new(p2["path"].as_str().unwrap()).is_file());
    // Louder sound is a different preview.
    ok(&r, "edit.set_volume", json!({"clip": clip, "db": -3, "base_rev": 5})).await;
    assert!(!cached(&ok(&r, "render.range_preview", preview).await));

    // Render-ahead fills in the rest.
    ok(&r, "ui.cache_hint", json!({"at": "f0", "width": 160})).await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let st = ok(&r, "ui.cache_status", json!({})).await;
        let total = st["frames"].as_i64().unwrap_or(0);
        if st["revision"] == 6 && st["pending"] == 0 && total > 0 && st["cached"] == json!([[0, total]]) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "render-ahead did not finish: {st}");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(cached(&ok(&r, "render.frame", frame("f10")).await));
}

/// Keyframe curves: eases and graph-editor handles shape a transform, read
/// back as data through `render.layers`; bad shapes are refused.
#[tokio::test]
async fn keyframe_curves() {
    let dir = scratch("curves");
    make_talk(&dir.join("talk.mp4"));
    let r = ve_daemon::build_registry();
    ok(&r, "project.init", json!({"path": dir.join("edit"), "fps": "25", "width": 160, "height": 120})).await;
    let asset = ok(&r, "media.import", json!({"paths": [dir.join("talk.mp4")], "base_rev": 0})).await["assets"][0]["id"].clone();
    let ins = ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;
    let video = ins["created"].as_array().unwrap().iter().find(|c| c["track"] == "V1").unwrap()["id"].clone();
    let opacity = async |at: &str| ok(&r, "render.layers", json!({"at": at})).await["layers"][0]["opacity"].as_f64().unwrap();

    // ease-in: behind linear at the middle, exact at the ends.
    let keys = json!({"opacity": [{"at": "0s", "value": 0, "ease": "ease-in"}, {"at": "2s", "value": 1}]});
    ok(&r, "edit.set_transform", json!({"clip": video, "keyframes": keys, "base_rev": 2})).await;
    assert!(opacity("f25").await < 0.4);
    assert_eq!(opacity("f50").await, 1.0);

    // Handles: an overshooting bump between two equal values, clamped to the range at render.
    let keys = json!({"scale": [{"at": "0s", "value": 1, "out": {"x": 0.3, "y": 0.8}}, {"at": "2s", "value": 1, "in": {"x": 0.3, "y": 0.8}}]});
    ok(&r, "edit.set_transform", json!({"clip": video, "keyframes": keys, "base_rev": 3})).await;
    let w = ok(&r, "render.layers", json!({"at": "f25"})).await["layers"][0]["rect"]["w"].as_f64().unwrap();
    assert!(w > 160.0 * 1.4, "bump at the middle: {w}");

    // An ease and handles on one segment, or a bad ease, are refused.
    let both = json!({"x": [{"at": "0s", "value": 0, "ease": "ease-out", "out": {"x": 0.2, "y": 0}}, {"at": "1s", "value": 1}]});
    let resp = call(&r, "edit.set_transform", json!({"clip": video, "keyframes": both, "base_rev": 4})).await;
    assert!(resp["error"]["message"].as_str().unwrap().contains("one or the other"), "{resp}");
    let bad = json!({"x": [{"at": "0s", "value": 0, "ease": "springy"}, {"at": "1s", "value": 1}]});
    let resp = call(&r, "edit.set_transform", json!({"clip": video, "keyframes": bad, "base_rev": 4})).await;
    assert!(resp["error"]["message"].as_str().unwrap().contains("cubic-bezier"), "{resp}");

    // The eases are saved in the project as written.
    let doc = fs::read_to_string(dir.join("edit/project.otio")).unwrap();
    assert!(doc.contains("\"ease\": \"ease-in\"") && doc.contains("\"out\""), "saved");
}
