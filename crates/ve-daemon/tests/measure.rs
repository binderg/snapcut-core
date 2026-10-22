//! measure.* and edit.normalize through the registry, on a file whose level
//! and pitch are known: a 220 Hz stereo sine at -20 dBFS peak, PCM so no codec
//! changes the numbers.

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

/// 4 s of 160x120 test picture with a 220 Hz sine at amplitude 0.1 on both channels.
fn make_tone(path: &Path) {
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=160x120:r=25:d=4"])
        .args(["-f", "lavfi", "-i", "aevalsrc=0.1*sin(2*PI*220*t)|0.1*sin(2*PI*220*t):s=48000:d=4"])
        .args(["-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le"])
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

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn near(v: &Value, want: f64, tol: f64) {
    assert!((f(v) - want).abs() <= tol, "{v} is not {want} ± {tol}");
}

#[tokio::test]
async fn measure_a_known_tone() {
    let dir = scratch("measure");
    let file = dir.join("tone.mkv");
    make_tone(&file);
    let r = ve_daemon::build_registry();

    // Color description comes with the probe.
    let probe = ok(&r, "media.probe", json!({"path": file})).await;
    let color = &probe["media"]["video"]["color"];
    assert_eq!((color["pixel_format"].as_str(), color["bit_depth"].as_u64()), (Some("yuv420p"), Some(8)), "{probe}");
    assert_eq!((color["chroma"].as_str(), color["dynamic_range"].as_str()), (Some("4:2:0"), Some("sdr")));

    ok(&r, "project.init", json!({"path": dir.join("edit"), "fps": "25", "width": 160, "height": 120})).await;
    let asset = ok(&r, "media.import", json!({"paths": [file], "base_rev": 0})).await["assets"][0]["id"].clone();

    // Levels of the file: -20 dBFS peak, -23 dBFS RMS (a sine), identical channels.
    let lv = ok(&r, "measure.levels", json!({"asset": asset, "resolution": "1s"})).await;
    for ch in lv["channels"].as_array().unwrap() {
        near(&ch["peak_db"], -20.0, 0.2);
        near(&ch["rms_db"], -23.0, 0.2);
        assert_eq!(ch["clipped_samples"], 0);
    }
    near(&lv["balance_db"], 0.0, 0.1);
    near(&lv["correlation"], 1.0, 0.01);
    assert_eq!(lv["series"]["rms_db"].as_array().unwrap().len(), 4, "{lv}");
    assert_eq!(lv["series"]["step"], "00:00:01:00");

    // The same call again comes from the cache.
    assert_eq!(ok(&r, "measure.levels", json!({"asset": asset, "resolution": "1s"})).await["cached"], true);

    // Loudness: two channels at -23 dBFS RMS sum to about -20.7 LUFS.
    let ld = ok(&r, "measure.loudness", json!({"asset": asset, "resolution": "1s"})).await;
    near(&ld["integrated_lufs"], -20.7, 0.8);
    near(&ld["true_peak_dbtp"], -20.0, 0.3);
    assert_eq!(ld["series"]["lufs"].as_array().unwrap().len(), 4, "{ld}");

    let pitch = ok(&r, "measure.pitch", json!({"asset": asset})).await;
    near(&pitch["median_hz"], 220.0, 2.0);
    assert_eq!(pitch["note"], "A3");
    assert!(f(&pitch["voiced"]) > 0.9, "{pitch}");

    let wf = ok(&r, "measure.waveform", json!({"asset": asset, "buckets": 50})).await;
    assert_eq!(wf["max"].as_array().unwrap().len(), 50);
    near(&wf["max"][10], 0.1, 0.005);
    near(&wf["min"][10], -0.1, 0.005);

    // Timeline measurements need no asset and are never cached.
    ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;
    let tl = ok(&r, "measure.loudness", json!({})).await;
    near(&tl["integrated_lufs"], f(&ld["integrated_lufs"]), 0.3);
    assert_eq!(tl["cached"], false);
    assert!(call(&r, "measure.loudness", json!({"asset": asset, "from": "1s"})).await["error"]["message"].as_str().unwrap().contains("whole"));

    // Normalize to -14: a dry run changes nothing, the real one adds the gain to the clip.
    let dry = ok(&r, "edit.normalize", json!({"target": -14, "base_rev": 2, "dry_run": true})).await;
    assert_eq!((dry["dry_run"].as_bool(), dry["revision"].as_u64()), (Some(true), Some(2)));
    let n = ok(&r, "edit.normalize", json!({"target": -14, "base_rev": 2})).await;
    let gain = f(&n["gain_db"]);
    near(&n["gain_db"], -14.0 - f(&tl["integrated_lufs"]), 0.1);
    assert_eq!(n["clips"].as_array().unwrap().len(), 1, "{n}");
    let after = ok(&r, "measure.loudness", json!({})).await;
    near(&after["integrated_lufs"], -14.0, 0.3);
    let doc = ok(&r, "project.get", json!({})).await;
    let text = doc["timeline"].to_string();
    assert!(text.contains(&format!("\"volume_db\":{gain}")), "{gain} not in the project");

    // A ceiling holds the gain back and says so.
    let capped = ok(&r, "edit.normalize", json!({"target": 0, "ceiling": -6, "base_rev": 3, "dry_run": true})).await;
    near(&capped["expected_true_peak_dbtp"], -6.0, 0.1);
    assert!(capped["warnings"][0].as_str().unwrap().contains("limiter"), "{capped}");
}
