//! Protocol stability: within a protocol major, changes must be additive.
//!
//! `fixtures/protocol-<major>.json` is a snapshot of every stable method
//! (`ui.*` is experimental and plugin methods are their plugins'; both are
//! left out): its params (required or not, and
//! their JSON types) and the fields of its result. This test fails when a
//! method, param or result field disappears, a param becomes required, a
//! required param is added, or a param's type narrows. Those changes wait for
//! the next protocol major (deprecate the old form meanwhile).
//!
//! Additions are fine but must be recorded, so a later removal is caught:
//! after adding a method, param or result field, run
//! `VE_UPDATE_PROTOCOL=1 cargo test -p ve-daemon --test protocol` and commit
//! the updated snapshot.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::{Value, json};

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/protocol-{}.json", ve_protocol::PROTOCOL_VERSION))
}

/// JSON types a schema accepts, without `null` (making a param optional is not a break).
fn types(schema: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    match schema.get("type") {
        Some(Value::String(t)) => {
            out.insert(t.clone());
        }
        Some(Value::Array(a)) => out.extend(a.iter().filter_map(Value::as_str).map(String::from)),
        _ => {}
    }
    for key in ["anyOf", "oneOf"] {
        for s in schema.get(key).and_then(Value::as_array).into_iter().flatten() {
            out.extend(types(s));
        }
    }
    out.remove("null");
    out
}

/// Top-level fields of a result: its properties, through `allOf` (flattened
/// structs) and every `anyOf`/`oneOf` branch (a job id or the result).
fn fields(schema: &Value, out: &mut BTreeSet<String>) {
    if let Some(p) = schema.get("properties").and_then(Value::as_object) {
        out.extend(p.keys().cloned());
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        for s in schema.get(key).and_then(Value::as_array).into_iter().flatten() {
            fields(s, out);
        }
    }
}

/// The stable surface as it is now, from `system.describe`.
async fn current() -> Value {
    let r = ve_daemon::build_registry();
    let req = json!({"jsonrpc": "2.0", "id": 1, "method": "system.describe"}).to_string();
    let doc: Value = serde_json::from_str(&r.dispatch(&req).await).unwrap();
    let mut methods = BTreeMap::new();
    for m in doc["result"]["methods"].as_array().unwrap() {
        let name = m["name"].as_str().unwrap();
        // ui.* is experimental; plugin methods belong to their plugins.
        if name.starts_with("ui.") || m.get("x-plugin").is_some() {
            continue;
        }
        let mut params = BTreeMap::new();
        for p in m["params"].as_array().into_iter().flatten() {
            params.insert(p["name"].as_str().unwrap().to_owned(), json!({"required": p["required"].as_bool().unwrap_or(false), "types": types(&p["schema"])}));
        }
        let mut result = BTreeSet::new();
        fields(&m["result"]["schema"], &mut result);
        methods.insert(name.to_owned(), json!({"params": params, "result": result, "deprecated": m["deprecated"].as_bool().unwrap_or(false)}));
    }
    json!({"protocol": ve_protocol::PROTOCOL_VERSION, "methods": methods})
}

/// Breaking changes from `old` to `new`, and additions (which only need recording).
fn compare(old: &Value, new: &Value) -> (Vec<String>, Vec<String>) {
    let (mut breaks, mut adds) = (Vec::new(), Vec::new());
    let empty = serde_json::Map::new();
    let (om, nm) = (old["methods"].as_object().unwrap_or(&empty), new["methods"].as_object().unwrap_or(&empty));
    for (name, o) in om {
        let Some(n) = nm.get(name) else {
            breaks.push(format!("{name} was removed"));
            continue;
        };
        let (op, np) = (o["params"].as_object().unwrap_or(&empty), n["params"].as_object().unwrap_or(&empty));
        for (p, od) in op {
            let Some(nd) = np.get(p) else {
                breaks.push(format!("{name}: param `{p}` was removed"));
                continue;
            };
            if nd["required"] == true && od["required"] != true {
                breaks.push(format!("{name}: param `{p}` became required"));
            }
            let set = |v: &Value| v.as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect::<BTreeSet<_>>();
            let (ot, nt) = (set(&od["types"]), set(&nd["types"]));
            if !ot.is_empty() && !nt.is_empty() && !ot.is_subset(&nt) {
                breaks.push(format!("{name}: param `{p}` no longer accepts {:?} (now {:?})", ot.difference(&nt).collect::<Vec<_>>(), nt));
            }
        }
        for (p, nd) in np {
            if !op.contains_key(p) {
                if nd["required"] == true {
                    breaks.push(format!("{name}: new param `{p}` is required (new params must be optional)"));
                } else {
                    adds.push(format!("{name}: param `{p}`"));
                }
            }
        }
        let list = |v: &Value| v.as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect::<BTreeSet<_>>();
        let (or, nr) = (list(&o["result"]), list(&n["result"]));
        for f in or.difference(&nr) {
            breaks.push(format!("{name}: result field `{f}` was removed"));
        }
        for f in nr.difference(&or) {
            adds.push(format!("{name}: result field `{f}`"));
        }
        if n["deprecated"] != o["deprecated"] {
            adds.push(format!("{name}: deprecated is now {}", n["deprecated"]));
        }
    }
    for name in nm.keys().filter(|n| !om.contains_key(*n)) {
        adds.push(format!("method {name}"));
    }
    (breaks, adds)
}

#[tokio::test]
async fn protocol_changes_are_additive() {
    let now = current().await;
    let path = snapshot_path();
    if std::env::var_os("VE_UPDATE_PROTOCOL").is_some() || !path.is_file() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let (breaks, _) = compare(&serde_json::from_str(&text).unwrap(), &now);
            assert!(breaks.is_empty(), "refusing to record breaking changes in protocol {}:\n{}", ve_protocol::PROTOCOL_VERSION, breaks.join("\n"));
        }
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&now).unwrap() + "\n").unwrap();
        return;
    }
    let old: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let (breaks, adds) = compare(&old, &now);
    assert!(
        breaks.is_empty(),
        "breaking protocol changes (keep the old form, deprecate it, and break only in protocol {}):\n{}",
        ve_protocol::PROTOCOL_VERSION + 1,
        breaks.join("\n")
    );
    assert!(
        adds.is_empty(),
        "the protocol grew; record it with `VE_UPDATE_PROTOCOL=1 cargo test -p ve-daemon --test protocol` and commit {}:\n{}",
        path.display(),
        adds.join("\n")
    );
}

#[test]
fn compare_catches_each_kind_of_break() {
    let old = json!({"methods": {
        "a.get": {"params": {"id": {"required": true, "types": ["string"]}, "n": {"required": false, "types": ["integer"]}}, "result": ["x", "y"], "deprecated": false},
        "a.gone": {"params": {}, "result": [], "deprecated": false}
    }});
    let new = json!({"methods": {
        "a.get": {"params": {"id": {"required": true, "types": ["string", "integer"]}, "n": {"required": true, "types": ["string"]}, "must": {"required": true, "types": []}, "may": {"required": false, "types": []}}, "result": ["x", "z"], "deprecated": true},
        "a.new": {"params": {}, "result": [], "deprecated": false}
    }});
    let (breaks, adds) = compare(&old, &new);
    let all = breaks.join("\n");
    for want in ["a.gone was removed", "`n` became required", "`n` no longer accepts", "new param `must` is required", "result field `y` was removed"] {
        assert!(all.contains(want), "missing {want}:\n{all}");
    }
    assert!(!all.contains("`id`"), "widening a type is fine:\n{all}");
    let adds = adds.join("\n");
    for want in ["param `may`", "result field `z`", "method a.new", "deprecated is now true"] {
        assert!(adds.contains(want), "missing {want}:\n{adds}");
    }
}
