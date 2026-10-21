//! Project folders on disk and the project.* methods.
//!
//! Each test uses its own fixed folder under the temp dir (wiped at the start)
//! so repeated runs reuse the same few lock files in the cache dir.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use ve_daemon::project::{self, PROJECT_FILE, ProjectError};
use ve_model::Rate;
use ve_model::meta::OutputSettings;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ve-daemon-tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn output() -> OutputSettings {
    OutputSettings {
        width: 1280,
        height: 720,
        rate: Rate::integer(25),
        sample_rate: 48_000,
        color_space: "bt709".into(),
    }
}

fn copy_fixture(dir: &std::path::Path) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/otio/talking_head.otio");
    fs::copy(src, dir.join(PROJECT_FILE)).unwrap();
}

#[test]
fn init_creates_folder_and_reopens() {
    let dir = scratch("init").join("my-edit");
    let p = project::init(&dir, "my-edit", output()).unwrap();
    assert!(dir.join("effects").is_dir() && dir.join("plugins").is_dir());
    let tracks = p.timeline.tracks.children.len();
    drop(p);

    let p = project::open(&dir).unwrap();
    assert_eq!(p.timeline.name, "my-edit");
    assert_eq!(p.timeline.tracks.children.len(), tracks);
    assert_eq!(p.ids_added, 0);
    drop(p);

    assert!(matches!(project::init(&dir, "again", output()), Err(ProjectError::AlreadyExists(_))));
}

#[test]
fn second_open_is_locked_until_first_closes() {
    let dir = scratch("lock");
    copy_fixture(&dir);
    let first = project::open(&dir).unwrap();
    assert!(matches!(project::open(&dir), Err(ProjectError::Locked(_))));
    // Same folder by a different spelling still hits the same lock.
    let other_spelling = dir.join("effects").join("..");
    fs::create_dir_all(dir.join("effects")).unwrap();
    assert!(matches!(project::open(&other_spelling), Err(ProjectError::Locked(_))));
    drop(first);
    project::open(&dir).unwrap();
}

#[test]
fn ids_added_on_open_are_saved_and_stable() {
    let dir = scratch("ids");
    copy_fixture(&dir);
    let p = project::open(&dir).unwrap();
    assert_eq!(p.ids_added, 2, "the chapter marker and the offline clip have no ids");
    let first = ve_model::summary::summary(&p.timeline);
    drop(p);
    let p = project::open(&dir).unwrap();
    assert_eq!(p.ids_added, 0);
    assert_eq!(ve_model::summary::summary(&p.timeline), first);
}

#[test]
fn save_round_trips_and_leaves_no_temp_file() {
    let dir = scratch("save");
    copy_fixture(&dir);
    let p = project::open(&dir).unwrap();
    let before = p.timeline.clone();
    p.save().unwrap();
    drop(p);
    assert!(!dir.join(format!("{PROJECT_FILE}.tmp")).exists());
    assert_eq!(project::open(&dir).unwrap().timeline, before);
}

#[test]
fn rejects_missing_foreign_and_newer_projects() {
    let dir = scratch("reject");
    assert!(matches!(project::open(&dir), Err(ProjectError::NotFound(_))));
    assert!(matches!(project::open(&dir.join("nope")), Err(ProjectError::NotFound(_))));

    let plain = json!({
        "OTIO_SCHEMA": "Timeline.1", "name": "x",
        "tracks": {"OTIO_SCHEMA": "Stack.1", "children": []}
    });
    fs::write(dir.join(PROJECT_FILE), plain.to_string()).unwrap();
    assert!(matches!(project::open(&dir), Err(ProjectError::Invalid { .. })));

    fs::write(dir.join(PROJECT_FILE), "{ not json").unwrap();
    assert!(matches!(project::open(&dir), Err(ProjectError::Invalid { .. })));

    let mut newer: Value =
        serde_json::from_str(&fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/otio/talking_head.otio")).unwrap())
            .unwrap();
    newer["metadata"]["ve"]["project"]["schema_version"] = json!("0.2.0");
    fs::write(dir.join(PROJECT_FILE), newer.to_string()).unwrap();
    assert!(matches!(project::open(&dir), Err(ProjectError::TooNew { .. })));
}

async fn call(r: &ve_protocol::Registry, method: &str, params: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    serde_json::from_str(&r.dispatch(&req).await).unwrap()
}

#[tokio::test]
async fn project_methods_over_the_registry() {
    let dir = scratch("registry").join("cut");
    let r = ve_daemon::build_registry();

    let resp = call(&r, "project.summary", json!({})).await;
    assert_eq!(resp["error"]["code"], -32602, "nothing open yet");

    let resp = call(&r, "project.init", json!({"path": dir, "fps": "29.97", "width": 1920, "height": 1080})).await;
    assert_eq!(resp["result"]["name"], "cut");

    let resp = call(&r, "project.summary", json!({})).await;
    assert!(resp["result"]["text"].as_str().unwrap().contains("30000/1001, drop-frame"));

    let resp = call(&r, "project.get", json!({})).await;
    assert_eq!(resp["result"]["timeline"]["OTIO_SCHEMA"], "Timeline.1");

    assert!(call(&r, "project.save", json!({})).await["result"].is_object());
    // Re-opening the same folder in the same session is a no-op.
    assert!(call(&r, "project.open", json!({"path": dir})).await["result"].is_object());

    // A second session (another process, in real use) cannot open it.
    let other = ve_daemon::build_registry();
    let resp = call(&other, "project.open", json!({"path": dir})).await;
    assert_eq!(resp["error"]["code"], -31003);

    let resp = call(&r, "project.init", json!({"path": dir, "fps": "fast", "width": 1, "height": 1})).await;
    assert_eq!(resp["error"]["code"], -32602);
}
