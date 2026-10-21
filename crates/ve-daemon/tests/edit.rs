//! edit.*, marker.* and history.* through the registry, against a real
//! project folder. Undo is checked across registries, the way separate CLI
//! processes see it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use serde_json::{Value, json};
use ve_protocol::Registry;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ve-daemon-tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_clip(path: &Path, seconds: u32) {
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=160x120:rate=25", "-f", "lavfi", "-i"])
        .arg("sine=frequency=440:sample_rate=48000")
        .args(["-t", &seconds.to_string(), "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(path)
        .status()
        .expect("ffmpeg must be on PATH");
    assert!(status.success());
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

async fn rev(r: &Registry) -> u64 {
    ok(r, "project.summary", json!({})).await["revision"].as_u64().unwrap()
}

async fn summary(r: &Registry) -> String {
    ok(r, "project.summary", json!({})).await["text"].as_str().unwrap().to_owned()
}

/// (track, clip ID, start) for each clip line of a summary.
fn clips(text: &str) -> Vec<(String, String, String)> {
    let mut track = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('"') {
            track = line.split('"').nth(1).unwrap_or_default().to_owned();
        } else if let Some(id) = line.split_whitespace().find(|w| w.starts_with("clip_")) {
            out.push((track.clone(), id.to_owned(), line.split_whitespace().next().unwrap().to_owned()));
        }
    }
    out
}

/// A project with a 4 s clip imported and inserted at 0. Returns the registry,
/// the project folder and the video clip ID.
async fn setup(name: &str) -> (Arc<Registry>, PathBuf, String) {
    let dir = scratch(name);
    make_clip(&dir.join("talk.mp4"), 4);
    let r = ve_daemon::build_registry();
    let project = dir.join("edit");
    ok(&r, "project.init", json!({"path": project, "fps": "25", "width": 160, "height": 120})).await;
    let imp = ok(&r, "media.import", json!({"paths": [dir.join("talk.mp4")], "base_rev": 0})).await;
    let asset = imp["assets"][0]["id"].as_str().unwrap().to_owned();
    let ins = ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;
    assert_eq!(ins["revision"], 2);
    let created = ins["created"].as_array().unwrap();
    assert_eq!(created.len(), 2, "{ins}");
    assert_eq!(created[0]["track"], "V1");
    assert_eq!(created[1]["track"], "A1");
    assert_eq!(created[0]["end"], "00:00:04:00");
    (r, project, created[0]["id"].as_str().unwrap().to_owned())
}

#[tokio::test]
async fn edits_dry_run_and_stale_revisions() {
    let (r, project, clip) = setup("edit-basics").await;
    let file = || fs::read_to_string(project.join("project.otio")).unwrap();

    // Dry run: a patch, no new revision, no write.
    let before = file();
    let dry = ok(&r, "edit.split", json!({"at": "00:00:01:00", "base_rev": 2, "dry_run": true})).await;
    assert_eq!(dry["revision"], 2);
    assert_eq!(dry["changed"], true);
    assert!(!dry["patch"].as_array().unwrap().is_empty());
    assert_eq!(file(), before);

    // Stale base_rev fails with the current revision.
    let resp = call(&r, "edit.split", json!({"at": "f25", "base_rev": 1})).await;
    assert_eq!(resp["error"]["code"], -31000);
    assert_eq!(resp["error"]["data"]["current_rev"], 2);

    let cut = ok(&r, "edit.split", json!({"at": "1s", "clips": [clip], "base_rev": 2})).await;
    assert_eq!(cut["revision"], 3);
    assert_eq!(cut["created"].as_array().unwrap().len(), 2);
    // The patch carries the new revision for clients that apply it.
    let last = cut["patch"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last, json!({"op": "replace", "path": "/metadata/ve/project/revision", "value": 3}));

    let del = ok(&r, "edit.ripple_delete", json!({"ranges": [{"from": "00:00:01:00", "to": "00:00:01:10"}], "base_rev": 3})).await;
    assert_eq!(del["duration"], "00:00:03:15");
    let speed = ok(&r, "edit.set_speed", json!({"clip": clip, "speed": "1/2", "base_rev": 4})).await;
    assert_eq!(speed["modified"][0]["end"], "00:00:02:00");
    let text = summary(&r).await;
    assert!(text.contains("speed 1/2"), "{text}");

    // Bad input names the problem.
    let resp = call(&r, "edit.trim", json!({"clip": "clip_nope", "in": "+2f", "base_rev": 5})).await;
    assert_eq!(resp["error"]["code"], -32602);
    assert!(resp["error"]["message"].as_str().unwrap().contains("clip_nope"), "{resp}");
    let resp = call(&r, "edit.split", json!({"at": "soon", "base_rev": 5})).await;
    assert!(resp["error"]["message"].as_str().unwrap().contains("not a time"), "{resp}");
}

#[tokio::test]
async fn undo_redo_and_groups_survive_new_processes() {
    let (r, project, _) = setup("edit-undo").await;
    let agent = json!({"kind": "agent", "id": "claude"});

    ok(&r, "history.begin_group", json!({"label": "tighten", "actor": agent})).await;
    ok(&r, "edit.split", json!({"at": "1s", "base_rev": 2, "actor": agent})).await;
    ok(&r, "edit.ripple_delete", json!({"ranges": [{"from": "2s", "to": "3s"}], "base_rev": 3, "actor": agent})).await;
    let ended = ok(&r, "history.end_group", json!({"actor": agent})).await;
    assert_eq!(ended["edits"], 2);
    let edited = summary(&r).await;
    assert!(edited.contains("duration 00:00:03:00"), "{edited}");

    // A new registry is a new CLI process: history comes from the cache dir.
    drop(r);
    let r = ve_daemon::build_registry();
    ok(&r, "project.open", json!({"path": project})).await;
    let list = ok(&r, "history.list", json!({})).await;
    assert_eq!(list["undo"][0]["what"], "tighten");
    assert_eq!(list["undo"][0]["actor"], "agent:claude");
    assert_eq!(list["undo"][0]["revs"], json!([3, 4]));

    let undone = ok(&r, "history.undo", json!({"base_rev": 4})).await;
    assert_eq!(undone["revision"], 5, "undo is a new revision");
    assert_eq!(undone["step"]["what"], "tighten");
    let text = summary(&r).await;
    assert!(text.contains("duration 00:00:04:00") && text.matches("clip_").count() == 2, "{text}");

    drop(r);
    let r = ve_daemon::build_registry();
    ok(&r, "project.open", json!({"path": project})).await;
    let redone = ok(&r, "history.redo", json!({"base_rev": 5})).await;
    assert_eq!(redone["revision"], 6);
    assert_eq!(summary(&r).await.replace("rev 6", "rev 4"), edited);

    // Undo the group, then the insert, then the import.
    ok(&r, "history.undo", json!({"base_rev": 6})).await;
    ok(&r, "history.undo", json!({"base_rev": 7})).await;
    let u = ok(&r, "history.undo", json!({"base_rev": 8})).await;
    assert_eq!(u["step"]["what"], "media.import");
    assert_eq!(ok(&r, "media.assets", json!({})).await["assets"], json!([]));
    let resp = call(&r, "history.undo", json!({"base_rev": 9})).await;
    assert!(resp["error"]["message"].as_str().unwrap().contains("nothing to undo"));

    // A fresh edit clears redo.
    assert!(!ok(&r, "history.list", json!({})).await["redo"].as_array().unwrap().is_empty());
    ok(&r, "marker.add", json!({"at": "0s", "name": "start", "base_rev": rev(&r).await})).await;
    assert!(ok(&r, "history.list", json!({})).await["redo"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn history_is_dropped_when_the_file_changes_elsewhere() {
    let (r, project, _) = setup("edit-stale-history").await;
    drop(r);
    // Another tool rewrites the project at a different revision.
    let path = project.join("project.otio");
    let mut doc: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    doc["metadata"]["ve"]["project"]["revision"] = json!(40);
    fs::write(&path, serde_json::to_string(&doc).unwrap()).unwrap();

    let r = ve_daemon::build_registry();
    ok(&r, "project.open", json!({"path": project})).await;
    assert_eq!(ok(&r, "history.list", json!({})).await["undo"], json!([]));
}

#[tokio::test]
async fn markers_trim_roll_move_and_volume() {
    let (r, _project, clip) = setup("edit-more").await;
    ok(&r, "edit.split", json!({"at": "2s", "base_rev": 2})).await;
    let audio_left = clips(&summary(&r).await).into_iter().find(|(t, _, at)| t == "A1" && at == "00:00:00:00").unwrap().1;

    // L cut: the outgoing audio runs 5 frames into the next shot.
    let roll = ok(&r, "edit.trim", json!({"clip": audio_left, "out": "+5f", "roll": true, "linked": false, "base_rev": 3})).await;
    assert_eq!(roll["modified"].as_array().unwrap().len(), 2, "{roll}");
    let text = summary(&r).await;
    assert!(text.contains("00:00:02:05"), "{text}");

    let vol = ok(&r, "edit.set_volume", json!({"clip": clip, "db": -6.0, "base_rev": 4})).await;
    assert_eq!(vol["modified"][0]["track"], "A1");
    assert!(summary(&r).await.contains("vol -6dB"));

    let mk = ok(&r, "marker.add", json!({"at": "00:00:03:00", "duration": "10f", "name": "um", "color": "yellow", "base_rev": 5})).await;
    let id = mk["markers"][0].as_str().unwrap().to_owned();
    let gone = ok(&r, "edit.ripple_delete", json!({"markers": "um", "base_rev": 6})).await;
    assert_eq!(gone["duration"], "00:00:03:15");
    // The marker's own range was deleted, so the marker went with it.
    let resp = call(&r, "marker.remove", json!({"ids": [id], "base_rev": 7})).await;
    assert!(resp["error"]["message"].as_str().unwrap().contains("no timeline marker"), "{resp}");

    let mv = ok(&r, "edit.move", json!({"clip": clip, "to": "00:00:05:00", "base_rev": 7})).await;
    assert_eq!(mv["modified"][0]["start"], "00:00:05:00");
}
