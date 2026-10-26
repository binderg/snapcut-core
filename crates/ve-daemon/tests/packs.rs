//! Effect packs: plugins with `contributes.effects` and no process. Their
//! effects show in `fx.list` as `pack`, and the first use in a project copies
//! the effect into the project's `effects/` folder (not on a dry run, and not
//! when the edit fails), after which the project's copy wins over the pack.
//! Needs ffmpeg on PATH.

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

async fn call(r: &Registry, method: &str, params: Value) -> Value {
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
    serde_json::from_str(&r.dispatch(&req).await).unwrap()
}

async fn ok(r: &Registry, method: &str, params: Value) -> Value {
    let resp = call(r, method, params.clone()).await;
    assert!(resp.get("error").is_none(), "{method} {params}: {resp}");
    resp["result"].clone()
}

async fn rev(r: &Registry) -> Value {
    ok(r, "project.summary", json!({})).await["revision"].clone()
}

/// One effect folder in a pack.
fn write_effect(dir: &Path, id: &str, version: &str, kind: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    let m = json!({"id": id, "version": version, "kind": kind, "name": id, "shader": "fx.wgsl"});
    fs::write(dir.join("effect.json"), m.to_string()).unwrap();
    let src = match kind {
        "effect" => format!("fn effect(uv: vec2<f32>) -> vec4<f32> {{ {body} }}"),
        _ => format!("fn transition(uv: vec2<f32>) -> vec4<f32> {{ {body} }}"),
    };
    fs::write(dir.join("fx.wgsl"), src).unwrap();
}

fn find<'a>(list: &'a Value, id: &str) -> Vec<&'a Value> {
    list["effects"].as_array().unwrap().iter().filter(|v| v["id"] == id).collect()
}

fn effect_folders(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(root.join("effects"))
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

#[tokio::test]
async fn pack_effects_are_listed_and_copied_on_use() {
    let dir = scratch("packs-basic");
    let st = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=320x240:r=30:d=3", "-c:v", "libx264", "-preset", "ultrafast"])
        .arg(dir.join("clip.mp4"))
        .status()
        .expect("ffmpeg on PATH");
    assert!(st.success());
    let r: Arc<Registry> = ve_daemon::registry(Arc::new(ve_daemon::Session::default()));
    let root = dir.join("edit");
    ok(&r, "project.init", json!({"path": root, "fps": "30", "width": 320, "height": 240})).await;
    let asset = ok(&r, "media.import", json!({"paths": [dir.join("clip.mp4")], "base_rev": 0})).await["assets"][0]["id"].clone();
    ok(&r, "edit.insert", json!({"asset": asset, "base_rev": 1})).await;
    let tl = ok(&r, "project.get", json!({})).await;
    let clip = tl["timeline"]["tracks"]["children"][0]["children"][0]["metadata"]["ve"]["clip"]["id"].clone();

    // A pack: a plugin with only effects, one folder of them plus a broken one.
    let pack = root.join("plugins").join("acme-looks");
    fs::create_dir_all(&pack).unwrap();
    let manifest = json!({"id": "acme-looks", "version": "1.0.0", "engine": "*", "protocol": "1", "name": "Acme Looks",
        "contributes": {"effects": ["effects"]}});
    fs::write(pack.join("plugin.json"), manifest.to_string()).unwrap();
    write_effect(&pack.join("effects/invert"), "acme.looks/invert", "1.0.0", "effect", "let c = src(uv); return vec4<f32>(c.a - c.rgb, c.a);");
    write_effect(&pack.join("effects/flash"), "acme.looks/flash", "1.0.0", "transition", "return mix(outgoing(uv), incoming(uv), u.progress);");
    write_effect(&pack.join("effects/sneaky"), "project.sneaky", "1.0.0", "effect", "return src(uv);");

    let list = ok(&r, "fx.list", json!({})).await;
    let inv = find(&list, "acme.looks/invert");
    assert_eq!(inv.len(), 1, "{list}");
    assert_eq!(inv[0]["origin"], "pack");
    assert_eq!(inv[0]["plugin"], "acme-looks");
    assert!(find(&list, "project.sneaky").is_empty());
    assert!(list["problems"].as_array().unwrap().iter().any(|p| p.as_str().unwrap().contains("`project.` ids")), "{list}");
    let plugins = ok(&r, "plugin.list", json!({})).await;
    let p = plugins["plugins"].as_array().unwrap().iter().find(|p| p["id"] == "acme-looks").unwrap();
    assert_eq!(p["effects"], 3);
    assert_eq!(p["commands"], json!([]));

    // A dry run copies nothing.
    let dry = ok(&r, "effect.add", json!({"clip": clip, "effect": "acme.looks/invert", "base_rev": rev(&r).await, "dry_run": true})).await;
    assert_eq!(dry["changed"], true);
    assert!(effect_folders(&root).is_empty());

    // First use copies the effect in; then it is the project's.
    ok(&r, "effect.add", json!({"clip": clip, "effect": "acme.looks/invert", "base_rev": rev(&r).await})).await;
    assert_eq!(effect_folders(&root), ["acme_looks_invert"]);
    let list = ok(&r, "fx.list", json!({})).await;
    let inv = find(&list, "acme.looks/invert");
    assert_eq!(inv.len(), 1, "{list}");
    assert_eq!(inv[0]["origin"], "project");

    // A newer minor in the pack does not reach the project: its copy wins.
    write_effect(&pack.join("effects/invert"), "acme.looks/invert", "1.1.0", "effect", "return src(uv).bgra;");
    ok(&r, "effect.add", json!({"clip": clip, "effect": "acme.looks/invert", "base_rev": rev(&r).await})).await;
    assert_eq!(effect_folders(&root), ["acme_looks_invert"]);
    let copy = fs::read_to_string(root.join("effects/acme_looks_invert/fx.wgsl")).unwrap();
    assert!(copy.contains("c.a - c.rgb"), "{copy}");

    // A new major is asked for by `@2` and lands next to the old one.
    write_effect(&pack.join("effects/invert"), "acme.looks/invert", "2.0.0", "effect", "return src(uv).bgra;");
    ok(&r, "effect.add", json!({"clip": clip, "effect": "acme.looks/invert@2", "base_rev": rev(&r).await})).await;
    assert_eq!(effect_folders(&root), ["acme_looks_invert", "acme_looks_invert_v2"]);

    // A failing edit leaves no copy behind.
    ok(&r, "edit.split", json!({"at": "1.5s", "base_rev": rev(&r).await})).await;
    let stale = call(&r, "transition.add", json!({"at": "1.5s", "transition": "acme.looks/flash", "base_rev": 0})).await;
    assert!(stale.get("error").is_some(), "{stale}");
    assert_eq!(effect_folders(&root), ["acme_looks_invert", "acme_looks_invert_v2"]);
    let added = ok(&r, "transition.add", json!({"at": "1.5s", "transition": "acme.looks/flash", "base_rev": rev(&r).await})).await;
    assert_eq!(added["transition"], "acme.looks/flash@1");
    assert_eq!(effect_folders(&root), ["acme_looks_flash", "acme_looks_invert", "acme_looks_invert_v2"]);

    // With the pack gone, the project still has everything it uses.
    fs::remove_dir_all(&pack).unwrap();
    let list = ok(&r, "fx.list", json!({})).await;
    assert_eq!(find(&list, "acme.looks/flash")[0]["origin"], "project");
    assert_eq!(find(&list, "acme.looks/invert").len(), 2, "{list}");
}

/// The gl-transitions pack shipped in `plugins/` loads with no problems.
#[test]
fn shipped_gl_transitions_pack_loads() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/gl-transitions");
    let m = ve_plugin::Manifest::load(&dir).unwrap();
    assert!(m.run.is_none() && m.contributes.commands.is_empty());
    let dirs = m.effect_dirs(&dir);
    let t = std::time::Instant::now();
    let r = ve_render::fx::Registry::packs(&dirs);
    eprintln!("{} gl-transitions validated in {:?}", r.list.len(), t.elapsed());
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    assert_eq!(r.list.len(), dirs.len());
    assert!(r.list.len() >= 100);
    for fx in &r.list {
        assert!(fx.manifest.id.starts_with("gl.transition/"), "{}", fx.manifest.id);
        assert_eq!(ve_render::fx::kind_word(fx.kind()), "transition");
        assert!(fx.source.contains("License"), "{} lost its license header", fx.manifest.id);
    }
}
