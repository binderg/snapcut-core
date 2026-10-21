//! media.probe / media.import / media.assets against real files made with
//! ffmpeg. Phase 1 requires ffmpeg and ffprobe on PATH; these tests fail
//! (not skip) without them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use ve_protocol::Registry;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ve-daemon-tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A 1-second 25 fps clip with a tone. `tone` changes the content (and hash).
fn make_clip(path: &Path, tone: u32) {
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=160x120:rate=25", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency={tone}:sample_rate=48000"))
        .args(["-t", "1", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(path)
        .status()
        .expect("ffmpeg must be on PATH for media tests");
    assert!(status.success(), "ffmpeg failed to make {}", path.display());
}

async fn call(r: &Registry, method: &str, params: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    serde_json::from_str(&r.dispatch(&req).await).unwrap()
}

async fn new_project(dir: &Path) -> std::sync::Arc<Registry> {
    let r = ve_daemon::build_registry();
    let resp = call(&r, "project.init", json!({"path": dir.join("edit"), "fps": "25", "width": 160, "height": 120})).await;
    assert!(resp["result"].is_object(), "{resp}");
    r
}

#[tokio::test]
async fn probe_needs_no_project() {
    let dir = scratch("media-probe");
    make_clip(&dir.join("a.mp4"), 440);
    let r = ve_daemon::build_registry();
    let resp = call(&r, "media.probe", json!({"path": dir.join("a.mp4")})).await;
    let media = &resp["result"]["media"];
    assert_eq!(media["video"]["rate"], json!([25, 1]));
    assert_eq!(media["audio"]["sample_rate"], 48_000);
    assert!(resp["result"]["url"].as_str().unwrap().starts_with("file:///"));
    assert!(resp["result"]["ffprobe"]["streams"].is_array());

    fs::write(dir.join("bad.mov"), "not media").unwrap();
    let resp = call(&r, "media.probe", json!({"path": dir.join("bad.mov")})).await;
    assert_eq!(resp["error"]["code"], -32602);
    let resp = call(&r, "media.probe", json!({"path": dir.join("missing.mov")})).await;
    assert_eq!(resp["error"]["code"], -32602);
}

#[tokio::test]
async fn import_adds_assets_once_and_persists() {
    let dir = scratch("media-import");
    make_clip(&dir.join("a.mp4"), 440);
    make_clip(&dir.join("b.mp4"), 880);
    fs::copy(dir.join("a.mp4"), dir.join("a copy.mp4")).unwrap();
    let r = new_project(&dir).await;

    let resp = call(&r, "media.import", json!({"paths": [dir.join("a.mp4"), dir.join("b.mp4"), dir.join("a copy.mp4")], "base_rev": 0})).await;
    let res = &resp["result"];
    assert_eq!(res["revision"], 1, "{resp}");
    let assets = res["assets"].as_array().unwrap();
    assert_eq!(assets[0]["existing"], false);
    assert_eq!(assets[1]["existing"], false);
    // Same bytes under another name is the same asset.
    assert_eq!(assets[2]["existing"], true);
    assert_eq!(assets[2]["id"], assets[0]["id"]);

    // Importing again changes nothing and keeps the revision.
    let resp = call(&r, "media.import", json!({"paths": [dir.join("b.mp4")], "base_rev": 1})).await;
    assert_eq!(resp["result"]["revision"], 1);
    assert_eq!(resp["result"]["assets"][0]["existing"], true);

    // The table survives a reopen, with hashes and media info.
    drop(r);
    let r = ve_daemon::build_registry();
    call(&r, "project.open", json!({"path": dir.join("edit")})).await;
    let list = call(&r, "media.assets", json!({})).await;
    let table = list["result"]["assets"].as_array().unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table[0]["hash"].as_str().unwrap().len(), 64);
    assert_eq!(table[0]["media"]["duration"]["rate"], 25.0);
    let text = call(&r, "project.summary", json!({})).await["result"]["text"].as_str().unwrap().to_owned();
    assert!(text.contains("a.mp4  00:00:01:00  160x120 25fps h264  aac 48000Hz 1ch"), "{text}");
}

#[tokio::test]
async fn import_checks_revision_dry_run_and_bad_files() {
    let dir = scratch("media-rules");
    make_clip(&dir.join("a.mp4"), 440);
    fs::write(dir.join("bad.mov"), "not media").unwrap();
    let r = new_project(&dir).await;
    let file = || fs::read_to_string(dir.join("edit").join("project.otio")).unwrap();
    let before = file();

    let resp = call(&r, "media.import", json!({"paths": [dir.join("a.mp4")], "base_rev": 5})).await;
    assert_eq!(resp["error"]["code"], -31000);
    assert_eq!(resp["error"]["data"]["current_rev"], 0);

    let resp = call(&r, "media.import", json!({"paths": [dir.join("a.mp4")], "base_rev": 0, "dry_run": true})).await;
    assert_eq!(resp["result"]["dry_run"], true);
    assert_eq!(resp["result"]["assets"][0]["existing"], false);
    assert_eq!(file(), before, "dry run must not write");

    // One unreadable file fails the whole call; the good one is not added either.
    let resp = call(&r, "media.import", json!({"paths": [dir.join("a.mp4"), dir.join("bad.mov")], "base_rev": 0})).await;
    assert_eq!(resp["error"]["code"], -32602);
    assert_eq!(file(), before);
    assert_eq!(call(&r, "media.assets", json!({})).await["result"]["assets"], json!([]));
}
