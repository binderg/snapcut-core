//! Audio effects through the protocol: `fx.list` shows them, `effect.add`
//! puts them on audio clips only (pointing at the linked partner otherwise),
//! `fx.define` checks and test-runs an agent's own, measurements hear them,
//! and `render.layers` reports them (with `only` for one side).
//! Needs ffmpeg on PATH.

use std::fs;
use std::path::PathBuf;
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

async fn call(r: &Registry, method: &str, params: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    serde_json::from_str(&r.dispatch(&req).await).unwrap()
}

async fn ok(r: &Registry, method: &str, params: Value) -> Value {
    let resp = call(r, method, params.clone()).await;
    assert!(resp.get("error").is_none(), "{method} {params}: {resp}");
    resp["result"].clone()
}

async fn fails(r: &Registry, method: &str, params: Value) -> String {
    let resp = call(r, method, params.clone()).await;
    resp["error"]["message"].as_str().unwrap_or_else(|| panic!("{method} {params} should fail: {resp}")).to_owned()
}

async fn rev(r: &Registry) -> Value {
    ok(r, "project.summary", json!({})).await["revision"].clone()
}

/// The first clip ID on the first track of `kind` (`Video` or `Audio`).
fn first_clip(tl: &Value, kind: &str) -> Value {
    tl["timeline"]["tracks"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["kind"] == kind)
        .and_then(|t| t["children"].as_array().unwrap().iter().find(|c| c["OTIO_SCHEMA"].as_str().unwrap_or("").starts_with("Clip")))
        .map(|c| c["metadata"]["ve"]["clip"]["id"].clone())
        .unwrap_or_else(|| panic!("no {kind} clip: {tl}"))
}

#[tokio::test]
async fn audio_effects_through_the_protocol() {
    let dir = scratch("audio-fx");
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=320x240:r=30:d=3", "-f", "lavfi", "-i", "sine=f=1000:r=48000:d=3"])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-shortest"])
        .arg(dir.join("talk.mp4"))
        .status()
        .expect("ffmpeg on PATH");
    assert!(st.success());
    let r: Arc<Registry> = ve_daemon::registry(Arc::new(ve_daemon::Session::default()));
    let root = dir.join("edit");
    ok(&r, "project.init", json!({"path": root, "fps": "30", "width": 320, "height": 240})).await;
    let asset = ok(&r, "media.import", json!({"paths": [dir.join("talk.mp4")], "base_rev": 0})).await["assets"][0]["id"].clone();
    ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;
    let tl = ok(&r, "project.get", json!({})).await;
    let (video, audio) = (first_clip(&tl, "Video"), first_clip(&tl, "Audio"));

    let list = ok(&r, "fx.list", json!({})).await;
    let eq = list["effects"].as_array().unwrap().iter().find(|e| e["id"] == "core.audio/eq").expect("built-in EQ");
    assert_eq!((eq["kind"].as_str(), eq["runtime"].as_str()), (Some("audio"), Some("ffmpeg")));

    // Each kind on its own kind of clip; a mismatch names the linked partner.
    let e = fails(&r, "effect.add", json!({"clip": video, "effect": "core.audio/eq", "base_rev": rev(&r).await})).await;
    assert!(e.contains("audio effect") && e.contains(audio.as_str().unwrap()), "{e}");
    let e = fails(&r, "effect.add", json!({"clip": audio, "effect": "core.color/exposure", "base_rev": rev(&r).await})).await;
    assert!(e.contains("video effect") && e.contains(video.as_str().unwrap()), "{e}");

    // Loudness of the first two seconds, then with a 12 dB cut on the tone.
    let lufs = |v: &Value| v["integrated_lufs"].as_f64().unwrap_or_else(|| panic!("{v}"));
    let before = lufs(&ok(&r, "measure.loudness", json!({"from": "0s", "to": "2s"})).await);
    let added = ok(&r, "effect.add", json!({"clip": audio, "effect": "core.audio/eq", "params": {"frequency": 1000, "gain": -12}, "base_rev": rev(&r).await})).await;
    let fx_id = added["effect"].clone();
    let after = lufs(&ok(&r, "measure.loudness", json!({"from": "0s", "to": "2s"})).await);
    assert!((before - after - 12.0).abs() < 1.0, "{before:.1} LUFS -> {after:.1} LUFS");

    // Keyframes on an animatable param; refused on one that cannot change while playing.
    let keys = json!({"gain": [{"at": "0s", "value": 0}, {"at": "2s", "value": -24}]});
    ok(&r, "effect.set", json!({"id": fx_id, "keyframes": keys, "base_rev": rev(&r).await})).await;
    let de = ok(&r, "effect.add", json!({"clip": audio, "effect": "core.audio/deesser", "base_rev": rev(&r).await})).await["effect"].clone();
    let e = fails(&r, "effect.set", json!({"id": de, "keyframes": {"intensity": [{"at": "0s", "value": 0.2}]}, "base_rev": rev(&r).await})).await;
    assert!(e.contains("cannot be keyframed"), "{e}");

    // render.layers: values at the frame, and one side only.
    let f = ok(&r, "render.layers", json!({"at": "1s"})).await;
    let fx = &f["audio"][0]["effects"][0];
    assert_eq!(fx["effect"], "core.audio/eq@1");
    assert!((fx["params"]["gain"].as_f64().unwrap() - -12.0).abs() < 0.01, "{fx}");
    assert!(f["layers"].is_array() && f["visible"].is_array());
    let a = ok(&r, "render.layers", json!({"at": "1s", "only": "audio"})).await;
    assert!(a.get("layers").is_none() && a.get("visible").is_none() && a["audio"].is_array(), "{a}");
    let v = ok(&r, "render.layers", json!({"at": "1s", "only": "video"})).await;
    assert!(v.get("audio").is_none() && v.get("audible").is_none() && v["layers"].is_array(), "{v}");
    let spans = ok(&r, "render.layers", json!({"only": "audio"})).await;
    assert_eq!(spans["spans"].as_array().unwrap().len(), 1, "{spans}");
    assert!(spans["spans"][0].get("visible").is_none(), "{spans}");

    // An agent's own audio effect: checked, test-run, saved as a manifest alone.
    let loud = json!({"id": "project.loud", "version": "1.0.0", "kind": "audio", "runtime": "ffmpeg", "name": "Loud",
        "filter": "volume=volume={gain}dB",
        "params": {"type": "object", "properties": {"gain": {"type": "number", "minimum": -30, "maximum": 30, "default": 6, "x-animatable": true}}}});
    let d = ok(&r, "fx.define", json!({"manifest": loud})).await;
    assert_eq!(d["tested_on"], "ffmpeg");
    let saved = PathBuf::from(d["path"].as_str().unwrap());
    let files: Vec<String> = fs::read_dir(&saved).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(files, ["effect.json"]);
    ok(&r, "effect.add", json!({"clip": audio, "effect": "project.loud", "base_rev": rev(&r).await})).await;

    // Templates that could reach files, or keyframes ffmpeg would ignore, are refused.
    let mut bad = loud.clone();
    bad["id"] = json!("project.reader");
    bad["filter"] = json!("amovie=secret.wav");
    let e = fails(&r, "fx.define", json!({"manifest": bad})).await;
    assert!(e.contains("not an allowed audio filter"), "{e}");
    let stuck = json!({"id": "project.stuck", "version": "1.0.0", "kind": "audio", "runtime": "ffmpeg", "name": "Stuck",
        "filter": "deesser=i={amount}",
        "params": {"type": "object", "properties": {"amount": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5, "x-animatable": true}}}});
    let e = fails(&r, "fx.define", json!({"manifest": stuck})).await;
    assert!(e.contains("cannot change while playing"), "{e}");
}
