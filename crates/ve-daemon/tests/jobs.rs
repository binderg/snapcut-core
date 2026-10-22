//! Background jobs and renders that run without the project lock: progress,
//! cancel, the queue limit, and edits that keep working during a render.
//! Needs ffmpeg on PATH.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use ve_daemon::Session;
use ve_protocol::Registry;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ve-daemon-tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `secs` of a moving 1280x720 test picture with a tone, slow enough to
/// encode that a job is still running while the test pokes at it.
fn make_media(path: &Path, secs: u32) {
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("testsrc2=s=1280x720:r=30:d={secs}")])
        .args(["-f", "lavfi", "-i", &format!("sine=frequency=330:sample_rate=48000:d={secs}")])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
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

/// A project at rev 2 with `secs` of media on the timeline.
async fn project(name: &str, secs: u32) -> (Arc<Session>, Arc<Registry>, PathBuf) {
    let dir = scratch(name);
    make_media(&dir.join("clip.mp4"), secs);
    let session = Arc::new(Session::default());
    let r = ve_daemon::registry(session.clone());
    ok(&r, "project.init", json!({"path": dir.join("edit"), "fps": "30", "width": 1280, "height": 720})).await;
    let asset = ok(&r, "media.import", json!({"paths": [dir.join("clip.mp4")], "base_rev": 0})).await["assets"][0]["id"].clone();
    ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;
    (session, r, dir)
}

async fn job(r: &Registry, id: &Value) -> Value {
    ok(r, "jobs.get", json!({"job_id": id})).await
}

/// Poll until the job has finished.
async fn wait(r: &Registry, id: &Value) -> Value {
    let start = Instant::now();
    loop {
        let j = job(r, id).await;
        if ["done", "failed", "cancelled"].contains(&j["state"].as_str().unwrap()) {
            return j;
        }
        assert!(start.elapsed() < Duration::from_secs(300), "job never finished: {j}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Poll until the job is running.
async fn wait_running(r: &Registry, id: &Value) -> Value {
    let start = Instant::now();
    loop {
        let j = job(r, id).await;
        if j["state"] != "queued" {
            return j;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "job never started: {j}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn background_export_reports_progress_and_finishes() {
    let (session, r, _dir) = project("jobs-export", 20).await;
    let mut events = session.events.subscribe();

    let started = ok(&r, "render.export", json!({"preset": "draft-540p", "background": true})).await;
    let id = started["job_id"].clone();
    assert!(id.as_str().unwrap().starts_with("job_"), "{started}");
    assert_eq!(started["state"], "queued");

    let j = wait(&r, &id).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(j["progress"], 1.0);
    let path = PathBuf::from(j["result"]["path"].as_str().unwrap());
    assert!(path.is_file(), "{j}");
    assert_eq!(j["result"]["frames"], 600);

    // Subscribers heard about it: progress along the way, then one finish with the result.
    let mut progress = Vec::new();
    let mut finished = None;
    while let Ok(msg) = events.try_recv() {
        let v: Value = serde_json::from_str(&msg).unwrap();
        if v["params"]["job_id"] != id {
            continue;
        }
        match v["method"].as_str().unwrap() {
            "job.progress" => progress.push(v["params"]["progress"].as_f64().unwrap()),
            "job.finished" => finished = Some(v["params"].clone()),
            other => panic!("unexpected {other}"),
        }
    }
    assert!(progress.len() >= 2, "{progress:?}");
    assert!(progress.windows(2).all(|w| w[0] <= w[1]), "progress went backwards: {progress:?}");
    assert!(progress.iter().all(|p| *p < 1.0), "{progress:?}");
    let finished = finished.expect("job.finished");
    assert_eq!(finished["state"], "done");
    assert_eq!(finished["result"]["path"], j["result"]["path"]);

    // The list has it, without the result.
    let list = ok(&r, "jobs.list", json!({})).await;
    let listed = list["jobs"].as_array().unwrap().iter().find(|x| x["job_id"] == id).unwrap().clone();
    assert_eq!(listed["state"], "done");
    assert!(listed.get("result").is_none(), "{listed}");
}

#[tokio::test]
async fn cancel_stops_a_running_export() {
    let (_session, r, _dir) = project("jobs-cancel", 60).await;
    let id = ok(&r, "render.export", json!({"preset": "master", "background": true})).await["job_id"].clone();
    wait_running(&r, &id).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let asked = Instant::now();
    ok(&r, "jobs.cancel", json!({"job_id": id})).await;
    let j = wait(&r, &id).await;
    assert_eq!(j["state"], "cancelled", "{j}");
    assert_eq!(j["error"]["code"], -31004, "{j}");
    assert!(asked.elapsed() < Duration::from_secs(5), "cancel took {:?}", asked.elapsed());

    // Cancelling again, or an unknown job, is answered plainly.
    ok(&r, "jobs.cancel", json!({"job_id": id})).await;
    assert_eq!(call(&r, "jobs.get", json!({"job_id": "job_nope"})).await["error"]["code"], -32602);
}

#[tokio::test]
async fn edits_and_frames_go_on_during_a_render() {
    let (_session, r, _dir) = project("jobs-concurrent", 60).await;
    let id = ok(&r, "render.export", json!({"preset": "master", "background": true})).await["job_id"].clone();
    wait_running(&r, &id).await;

    // While it renders: a frame, and an edit that bumps the revision.
    let t = Instant::now();
    let f = ok(&r, "render.frame", json!({"at": "00:00:30:00", "width": 320})).await;
    assert_eq!(f["mime"], "image/jpeg");
    let m = ok(&r, "marker.add", json!({"at": "00:00:10:00", "name": "during", "base_rev": 2})).await;
    assert_eq!(m["revision"], 3);
    let quick = t.elapsed();
    let state = job(&r, &id).await["state"].clone();

    // The export renders the timeline as it was when it started, and still finishes.
    let j = wait(&r, &id).await;
    assert_eq!(j["state"], "done", "{j}");
    assert!(j["result"]["path"].as_str().unwrap().contains("rev2"), "{j}");
    if state == "running" {
        // Both calls answered while the export was still going: nothing waited for the lock.
        assert!(quick < Duration::from_secs(10), "{quick:?}");
    } else {
        eprintln!("export finished before the calls; concurrency not exercised ({quick:?})");
    }
}

#[tokio::test]
async fn analysis_fails_if_the_project_changed_meanwhile() {
    let (_session, r, _dir) = project("jobs-stale", 60).await;
    let id = ok(&r, "analyze.silence", json!({"base_rev": 2, "background": true})).await["job_id"].clone();
    // Edit while the analyzer reads the old timeline.
    let m = call(&r, "marker.add", json!({"at": "00:00:01:00", "base_rev": 2})).await;
    let j = wait(&r, &id).await;
    if m.get("error").is_some() {
        // The analysis committed first (rev 3), so the edit was the stale one.
        assert_eq!(j["state"], "done", "{j}");
        assert_eq!(m["error"]["code"], -31000);
    } else {
        assert_eq!(j["state"], "failed", "{j}");
        assert_eq!(j["error"]["code"], -31000, "{j}");
    }
}

#[tokio::test]
async fn at_most_two_jobs_run_at_once() {
    let (_session, r, _dir) = project("jobs-queue", 60).await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(ok(&r, "render.export", json!({"preset": "master", "background": true})).await["job_id"].clone());
    }
    // Once two run, the third waits its turn.
    let start = Instant::now();
    let states = loop {
        let list = ok(&r, "jobs.list", json!({})).await;
        let states: Vec<String> = ids
            .iter()
            .map(|id| list["jobs"].as_array().unwrap().iter().find(|j| &j["job_id"] == id).unwrap()["state"].as_str().unwrap().to_owned())
            .collect();
        if states.iter().filter(|s| *s == "running").count() == 2 {
            break states;
        }
        assert!(start.elapsed() < Duration::from_secs(30), "{states:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(states.iter().filter(|s| *s == "queued").count(), 1, "{states:?}");

    // Cancelling a queued job ends it at once; the rest are cancelled too.
    let queued = &ids[states.iter().position(|s| s == "queued").unwrap()];
    ok(&r, "jobs.cancel", json!({"job_id": queued})).await;
    assert_eq!(wait(&r, queued).await["state"], "cancelled");
    for id in &ids {
        ok(&r, "jobs.cancel", json!({"job_id": id})).await;
        assert_eq!(wait(&r, id).await["state"], "cancelled");
    }
}
