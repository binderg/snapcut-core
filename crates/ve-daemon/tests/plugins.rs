//! The plugin host end to end, with a Python test plugin
//! (`tests/fixtures/plugins/testplug`): discovery, `describe`, calls back
//! into the engine, transcripts, undo groups, jobs with progress and cancel,
//! crashes, missing secrets and refused manifests.
//! Needs ffmpeg and Python 3 on PATH; skips when Python is missing.
//! Plugins in the user's own plugin folder may show up too, so tests look
//! plugins up by id.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use ve_protocol::Registry;

fn python_available() -> bool {
    let exe = if cfg!(windows) { "python" } else { "python3" };
    Command::new(exe).arg("--version").output().is_ok_and(|o| o.status.success())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ve-daemon-tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_media(path: &Path, secs: u32) {
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("testsrc2=s=320x240:r=30:d={secs}")])
        .args(["-f", "lavfi", "-i", &format!("sine=frequency=330:sample_rate=48000:d={secs}")])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(path)
        .status()
        .expect("ffmpeg on PATH");
    assert!(st.success());
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
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

async fn err(r: &Registry, method: &str, params: Value) -> Value {
    let resp = call(r, method, params.clone()).await;
    resp.get("error").cloned().unwrap_or_else(|| panic!("{method} {params} should fail: {resp}"))
}

/// A project with 3 s of A/V on the timeline and the test plugin in its plugins/ folder.
async fn project(name: &str) -> Option<(Arc<Registry>, PathBuf, Value)> {
    if !python_available() {
        eprintln!("skipping: no Python 3 on PATH");
        return None;
    }
    let dir = scratch(name);
    make_media(&dir.join("clip.mp4"), 3);
    let r = ve_daemon::registry(Arc::new(ve_daemon::Session::default()));
    let root = dir.join("edit");
    ok(&r, "project.init", json!({"path": root, "fps": "30", "width": 320, "height": 240})).await;
    let asset = ok(&r, "media.import", json!({"paths": [dir.join("clip.mp4")], "base_rev": 0})).await["assets"][0]["id"].clone();
    ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/testplug");
    copy_dir(&fixture, &root.join("plugins/testplug"));
    Some((r, root, asset))
}

fn find<'a>(list: &'a Value, key: &str, value: &str) -> &'a Value {
    list.as_array().unwrap().iter().find(|v| v[key] == value).unwrap_or_else(|| panic!("no {key}={value} in {list}"))
}

async fn wait_job(r: &Registry, id: &Value, until: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let j = ok(r, "jobs.get", json!({"job_id": id})).await;
        if until(&j) {
            return j;
        }
        assert!(Instant::now() < deadline, "job never got there: {j}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn discovery_describe_and_calls() {
    let Some((r, root, _)) = project("plugins-basic").await else { return };
    let list = ok(&r, "plugin.list", json!({})).await;
    let p = find(&list["plugins"], "id", "testplug");
    assert_eq!(p["source"], "project");
    assert_eq!(p["running"], false);
    assert_eq!(find(&p["commands"], "method", "testplug.echo")["cli"]["name"], "echo-test");
    assert_eq!(find(&p["commands"], "method", "testplug.slow")["params"]["properties"]["background"]["type"], "boolean");

    let doc = ok(&r, "system.describe", json!({})).await;
    assert_eq!(find(&doc["methods"], "name", "testplug.echo")["x-plugin"], "testplug");

    let out = ok(&r, "testplug.echo", json!({"text": "hello"})).await;
    assert_eq!(out["text"], "hello");
    assert_eq!(out["env_id"], "testplug");
    assert_eq!(out["init"]["plugin"]["id"], "testplug");
    assert_eq!(out["init"]["protocol"], 1);
    // Compare real paths: CI temp dirs differ in spelling only (RUNNER~1 on
    // Windows, /var vs /private/var on macOS).
    let real = |p: &std::path::Path| std::fs::canonicalize(p).unwrap();
    let project_dir = PathBuf::from(out["init"]["project"].as_str().unwrap());
    assert_eq!(real(&project_dir), real(&root));
    let list = ok(&r, "plugin.list", json!({})).await;
    assert_eq!(find(&list["plugins"], "id", "testplug")["running"], true);

    // Unknown methods are still unknown.
    assert_eq!(err(&r, "testplug.nope", json!({})).await["code"], -32601);
}

#[tokio::test]
async fn plugin_transcribes_through_engine_calls() {
    let Some((r, root, asset)) = project("plugins-transcribe").await else { return };
    let out = ok(&r, "testplug.transcribe", json!({"asset": asset})).await;
    assert_eq!(out["sample_rate"], 16000);
    // 3 s of 16-bit mono at 16 kHz, plus the WAV header.
    assert!(out["bytes"].as_u64().unwrap() > 90_000, "{out}");
    assert_eq!(out["words"], 6);
    assert_eq!(fs::read_dir(root.join("transcripts")).unwrap().count(), 1);

    let list = ok(&r, "transcript.list", json!({})).await;
    let t = &list["transcripts"][0];
    assert_eq!(t["asset"], asset);
    assert_eq!(t["provenance"]["actor"], "plugin:testplug");
    assert_eq!(t["provenance"]["model"], "fake");

    // On the timeline: cut the first second away and the first two words go with it.
    let before = ok(&r, "transcript.get", json!({})).await;
    assert_eq!(before["words"].as_array().unwrap().len(), 6);
    assert_eq!(before["words"][1]["start"], "00:00:00:15");
    assert_eq!(before["words"][1]["end"], "00:00:00:27");
    let rev = ok(&r, "project.summary", json!({})).await["revision"].clone();
    ok(&r, "edit.ripple_delete", json!({"ranges": [{"from": "0s", "to": "1s"}], "base_rev": rev})).await;
    let after = ok(&r, "transcript.get", json!({})).await;
    let words = after["words"].as_array().unwrap();
    assert_eq!(words.len(), 4, "{after}");
    assert_eq!(words[0]["text"], "w2");
    assert_eq!(words[0]["start"], "00:00:00:00");
    assert_eq!(words[0]["source_start"], "1.000s");
    let text = ok(&r, "transcript.get", json!({"format": "text"})).await["text"].as_str().unwrap().to_owned();
    assert!(text.starts_with("[00:00:00:00] S0: w2 w3"), "{text}");
    // Captions follow the edit: timeline times, not media times.
    let srt = ok(&r, "transcript.get", json!({"format": "srt"})).await["text"].as_str().unwrap().to_owned();
    assert!(srt.starts_with("1\n00:00:00,000 --> "), "{srt}");
    let vtt = ok(&r, "transcript.get", json!({"format": "vtt"})).await["text"].as_str().unwrap().to_owned();
    assert!(vtt.starts_with("WEBVTT\n\n00:00:00.000 --> ") && vtt.contains("<v S0>w2"), "{vtt}");
    let own = ok(&r, "transcript.get", json!({"asset": asset})).await;
    assert_eq!(own["words"].as_array().unwrap().len(), 6);
    assert_eq!(own["language"], "en");

    assert_eq!(ok(&r, "transcript.remove", json!({"asset": asset})).await["removed"], true);
    let missing = ok(&r, "transcript.get", json!({})).await;
    assert_eq!(missing["missing"][0], asset);
    assert!(err(&r, "transcript.get", json!({"asset": asset})).await["message"].as_str().unwrap().contains("no transcript"));
}

#[tokio::test]
async fn transcript_set_checks_words() {
    let Some((r, _, asset)) = project("plugins-transcript-checks").await else { return };
    let w = |s: &str, e: &str| json!({"start": s, "end": e, "text": "x"});
    let e = err(&r, "transcript.set", json!({"asset": asset, "words": [w("2s", "1s")]})).await;
    assert!(e["message"].as_str().unwrap().contains("ends before it starts"), "{e}");
    let e = err(&r, "transcript.set", json!({"asset": asset, "words": [w("9s", "10s")]})).await;
    assert!(e["message"].as_str().unwrap().contains("past the end"), "{e}");
    let e = err(&r, "transcript.set", json!({"asset": asset, "words": [{"start": 1.0, "end": "2s", "text": "x"}]})).await;
    assert_eq!(e["code"], -32602);
    let set = ok(&r, "transcript.set", json!({"asset": asset, "words": [w("1.5s", "2s"), w("0.25s", "0.5s")]})).await;
    assert_eq!(set["end"], "2.000s");
    assert_eq!(set["replaced"], false);
    let got = ok(&r, "transcript.get", json!({"asset": asset})).await;
    assert_eq!(got["words"][0]["start"], "0.250s");
}

#[tokio::test]
async fn plugin_edits_undo_as_one_group() {
    let Some((r, _, _)) = project("plugins-group").await else { return };
    let rev = ok(&r, "project.summary", json!({})).await["revision"].as_u64().unwrap();
    let out = ok(&r, "testplug.mark", json!({"base_rev": rev})).await;
    assert_eq!(out["revision"].as_u64(), Some(rev + 2));
    let history = ok(&r, "history.list", json!({})).await;
    let text = history.to_string();
    assert!(text.contains("plugin:testplug"), "{history}");
    // One undo removes both markers.
    ok(&r, "history.undo", json!({"base_rev": rev + 2, "actor": {"kind": "plugin", "id": "testplug"}})).await;
    let summary = ok(&r, "project.summary", json!({})).await["text"].as_str().unwrap().to_owned();
    assert!(!summary.contains("one") && !summary.contains("two"), "{summary}");
}

#[tokio::test]
async fn jobs_report_progress_and_cancel() {
    let Some((r, _, _)) = project("plugins-jobs").await else { return };
    let started = ok(&r, "testplug.slow", json!({"background": true, "steps": 40})).await;
    let done = wait_job(&r, &started["job_id"], |j| j["state"] == "done").await;
    assert_eq!(done["result"]["done"], true);

    let started = ok(&r, "testplug.slow", json!({"background": true, "steps": 2000})).await;
    let id = started["job_id"].clone();
    wait_job(&r, &id, |j| j["progress"].as_f64().is_some_and(|p| p > 0.0)).await;
    ok(&r, "jobs.cancel", json!({"job_id": id})).await;
    let t = Instant::now();
    wait_job(&r, &id, |j| j["state"] == "cancelled").await;
    assert!(t.elapsed() < Duration::from_secs(3), "a cooperative cancel took {:?}", t.elapsed());

    // A plugin that ignores $/cancel is killed after the grace period, and restarts on the next call.
    let started = ok(&r, "testplug.stubborn", json!({"background": true})).await;
    let id = started["job_id"].clone();
    wait_job(&r, &id, |j| j["state"] == "running").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    ok(&r, "jobs.cancel", json!({"job_id": id})).await;
    wait_job(&r, &id, |j| j["state"] == "cancelled").await;
    assert_eq!(ok(&r, "testplug.echo", json!({"text": "back"})).await["text"], "back");
}

#[tokio::test]
async fn crashes_and_refusals_are_reported() {
    let Some((r, root, _)) = project("plugins-errors").await else { return };
    let e = err(&r, "testplug.crash", json!({})).await;
    assert_eq!(e["code"], -31002, "{e}");
    assert!(e["message"].as_str().unwrap().contains("about to crash on purpose"), "{e}");
    // The next call starts a fresh process.
    assert_eq!(ok(&r, "testplug.echo", json!({"text": "again"})).await["text"], "again");

    // A plugin needing a secret that is nowhere.
    let keyed = root.join("plugins/keyed");
    fs::create_dir_all(&keyed).unwrap();
    fs::write(
        keyed.join("plugin.json"),
        r#"{"id": "keyed", "version": "0.1.0", "engine": "*", "protocol": "1", "name": "Keyed",
            "contributes": {"commands": [{"method": "keyed.go", "summary": "Go"}]},
            "run": {"command": "python3", "args": ["main.py"], "windows": {"command": "python"}},
            "secrets": ["VE_TEST_SECRET_THAT_IS_NEVER_SET"]}"#,
    )
    .unwrap();
    // A plugin trying to take an engine namespace, and one with a broken manifest.
    let thief = root.join("plugins/thief");
    fs::create_dir_all(&thief).unwrap();
    fs::write(
        thief.join("plugin.json"),
        r#"{"id": "thief", "version": "0.1.0", "engine": "*", "protocol": "1", "name": "Thief",
            "contributes": {"commands": [{"method": "edit.split", "summary": "Mine now"}]}, "run": {"command": "x"}}"#,
    )
    .unwrap();
    let broken = root.join("plugins/broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("plugin.json"), "{ not json").unwrap();

    let list = ok(&r, "plugin.list", json!({})).await;
    let keyed_info = find(&list["plugins"], "id", "keyed");
    assert_eq!(keyed_info["secrets"][0]["source"], "missing");
    let issues = list["issues"].to_string();
    assert!(issues.contains("engine's `edit` namespace"), "{issues}");
    assert!(issues.contains("broken"), "{issues}");
    let e = err(&r, "keyed.go", json!({})).await;
    assert!(e["message"].as_str().unwrap().contains("ve plugin secret set keyed VE_TEST_SECRET_THAT_IS_NEVER_SET"), "{e}");
    // Built-ins are never shadowed.
    let rev = ok(&r, "project.summary", json!({})).await["revision"].clone();
    ok(&r, "edit.split", json!({"at": "00:00:01:00", "base_rev": rev})).await;
}
