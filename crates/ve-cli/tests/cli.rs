//! The `ve` binary end to end: the agent loop as an agent would type it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join("ve-cli-tests").join("loop");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn ve(cwd: &Path, args: &[&str]) -> Output {
    // Project commands start a daemon; a short idle timeout cleans it up if a test fails.
    Command::new(env!("CARGO_BIN_EXE_ve")).current_dir(cwd).env("VE_DAEMON_IDLE_SECS", "5").args(args).output().unwrap()
}

/// Run and require success; returns stdout.
fn ok(cwd: &Path, args: &[&str]) -> String {
    let o = ve(cwd, args);
    assert!(o.status.success(), "ve {args:?}: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap()
}

fn first_id(text: &str, prefix: &str) -> String {
    text.split_whitespace().find(|w| w.starts_with(prefix)).unwrap_or_else(|| panic!("no {prefix} in {text}")).to_owned()
}

#[test]
fn agent_loop() {
    let dir = scratch();
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=160x120:r=30", "-f", "lavfi", "-i"])
        .arg("aevalsrc='0.4*sin(2*PI*250*t)*(lt(t\\,2)+gte(t\\,3))':s=48000")
        .args(["-t", "5", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(dir.join("talk.mp4"))
        .status()
        .unwrap();
    assert!(st.success());

    ok(&dir, &["init", "edit", "--fps", "30", "--size", "160x120"]);
    let proj = dir.join("edit");
    let asset = first_id(&ok(&proj, &["import", "../talk.mp4"]), "asset_");
    let ins = ok(&proj, &["--actor", "agent:test", "insert", &asset]);
    assert!(ins.contains("rev 2") && ins.contains("V1") && ins.contains("A1"), "{ins}");

    // Signed deltas pass through clap.
    let clip = first_id(&ok(&proj, &["summary"]), "clip_");
    let t = ok(&proj, &["trim", &clip, "--out", "-5f"]);
    assert!(t.contains("00:00:04:25"), "{t}");

    let a = ok(&proj, &["analyze", "silence", "--min", "0.5s", "--pad", "0.1s"]);
    assert!(a.contains("1 found"), "{a}");
    let dry = ok(&proj, &["ripple-delete", "--markers", "silence", "--dry-run"]);
    assert!(dry.contains("dry run"), "{dry}");
    let cut = ok(&proj, &["ripple-delete", "--markers", "silence"]);
    assert!(cut.contains("rev 5"), "{cut}");

    // Undo / redo across processes.
    assert!(ok(&proj, &["undo"]).contains("edit.ripple_delete"));
    assert!(ok(&proj, &["redo"]).contains("edit.ripple_delete"));
    let h = ok(&proj, &["history"]);
    assert!(h.contains("agent:test") && h.contains("edit.insert"), "{h}");

    // Files land at -o.
    ok(&proj, &["frame", "--at", "1s", "--width", "160", "-o", "f.jpg"]);
    assert!(fs::read(proj.join("f.jpg")).unwrap().starts_with(&[0xFF, 0xD8]));
    let ex = ok(&proj, &["--json", "export", "--preset", "draft-540p", "-o", "out.mp4"]);
    let v: serde_json::Value = serde_json::from_str(&ex).unwrap();
    assert!(proj.join("out.mp4").is_file());
    assert!(v.get("data").is_none() && v["output"].as_str().unwrap().ends_with("out.mp4"), "{v}");

    // The export ran as a daemon job, which `ve jobs` lists.
    let jobs: serde_json::Value = serde_json::from_str(&ok(&proj, &["--json", "jobs"])).unwrap();
    let export = jobs["jobs"].as_array().unwrap().iter().find(|j| j["method"] == "render.export").expect("export job");
    assert_eq!(export["state"], "done", "{jobs}");
    let line = ok(&proj, &["jobs"]);
    assert!(line.contains("render.export") && line.contains("done"), "{line}");
    let o = ve(&proj, &["jobs", "cancel", "job_nope"]);
    assert_eq!(o.status.code(), Some(2));

    // Errors map to exit codes: stale revision is 3, bad params 2.
    let o = ve(&proj, &["split", "--at", "1s", "--base-rev", "1"]);
    assert_eq!(o.status.code(), Some(3), "{}", String::from_utf8_lossy(&o.stderr));
    let o = ve(&proj, &["split", "--at", "later"]);
    assert_eq!(o.status.code(), Some(2));
    let o = ve(&proj, &["--actor", "robot", "undo"]);
    assert_eq!(o.status.code(), Some(2));

    ok(&proj, &["daemon", "stop"]);
}
