//! Compact JSON Patch (RFC 6902) diffs.
//!
//! Generic JSON diffs compare arrays index by index, so deleting one clip from
//! a 400-item track rewrites the 399 after it. OTIO positions are implicit
//! (an item's start is the sum of what comes before it), so after a ripple
//! edit everything outside the edited span is unchanged; trimming the common
//! prefix and suffix of each array first keeps patches to the items that
//! really changed. That matters because every patch is stored for undo and
//! sent to every client as `project.changed`.

use json_patch::jsonptr::PointerBuf;
use json_patch::{AddOperation, Patch, PatchOperation, RemoveOperation, ReplaceOperation};
use serde_json::Value;

/// Patch that turns `left` into `right`.
pub fn diff(left: &Value, right: &Value) -> Patch {
    let mut ops = Vec::new();
    let mut path = PointerBuf::new();
    walk(left, right, &mut path, &mut ops);
    Patch(ops)
}

fn walk(l: &Value, r: &Value, path: &mut PointerBuf, ops: &mut Vec<PatchOperation>) {
    if l == r {
        return;
    }
    match (l, r) {
        (Value::Object(a), Value::Object(b)) if a.get("OTIO_SCHEMA") == b.get("OTIO_SCHEMA") => {
            for (k, va) in a {
                path.push_back(k.clone());
                match b.get(k) {
                    Some(vb) => walk(va, vb, path, ops),
                    None => ops.push(PatchOperation::Remove(RemoveOperation { path: path.clone() })),
                }
                path.pop_back();
            }
            for (k, vb) in b {
                if !a.contains_key(k) {
                    path.push_back(k.clone());
                    ops.push(PatchOperation::Add(AddOperation { path: path.clone(), value: vb.clone() }));
                    path.pop_back();
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
            let room = a.len().min(b.len()) - pre;
            let suf = a.iter().rev().zip(b.iter().rev()).take(room).take_while(|(x, y)| x == y).count();
            let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
            let paired = am.len().min(bm.len());
            for i in 0..paired {
                path.push_back(pre + i);
                walk(&am[i], &bm[i], path, ops);
                path.pop_back();
            }
            // Remove from the back so earlier indices stay valid.
            for i in (paired..am.len()).rev() {
                path.push_back(pre + i);
                ops.push(PatchOperation::Remove(RemoveOperation { path: path.clone() }));
                path.pop_back();
            }
            for (i, v) in bm.iter().enumerate().skip(paired) {
                path.push_back(pre + i);
                ops.push(PatchOperation::Add(AddOperation { path: path.clone(), value: v.clone() }));
                path.pop_back();
            }
        }
        _ => ops.push(PatchOperation::Replace(ReplaceOperation { path: path.clone(), value: r.clone() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn check(a: &Value, b: &Value) -> Patch {
        let p = diff(a, b);
        let mut doc = a.clone();
        json_patch::patch(&mut doc, &p).unwrap();
        assert_eq!(&doc, b);
        p
    }

    #[test]
    fn deleting_one_array_item_is_one_op() {
        let a = json!({"children": (0..400).map(|i| json!({"n": i})).collect::<Vec<_>>()});
        let mut b = a.clone();
        b["children"].as_array_mut().unwrap().remove(17);
        let p = check(&a, &b);
        assert_eq!(p.0.len(), 1, "{p:?}");
    }

    #[test]
    fn splitting_an_item_patches_it_and_adds_one() {
        let a = json!([{"d": 10}, {"d": 20}, {"d": 30}]);
        let b = json!([{"d": 10}, {"d": 5}, {"d": 15}, {"d": 30}]);
        let p = check(&a, &b);
        assert_eq!(p.0.len(), 2, "{p:?}");
    }

    #[test]
    fn different_schemas_replace_whole_objects() {
        let a = json!([{"OTIO_SCHEMA": "Clip.2", "x": 1, "y": 2}]);
        let b = json!([{"OTIO_SCHEMA": "Gap.1", "x": 1}]);
        let p = check(&a, &b);
        assert_eq!(p.0.len(), 1, "{p:?}");
    }

    fn arb_json() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            (0i64..5).prop_map(Value::from),
            "[ab]{0,2}".prop_map(Value::String),
        ];
        leaf.prop_recursive(4, 40, 5, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
                prop::collection::btree_map("[abc~/]{1,2}", inner, 0..4)
                    .prop_map(|m| Value::Object(m.into_iter().collect())),
            ]
        })
    }

    proptest! {
        #[test]
        fn patch_and_inverse_round_trip(a in arb_json(), b in arb_json()) {
            let fwd = diff(&a, &b);
            let inv = diff(&b, &a);
            let mut doc = a.clone();
            json_patch::patch(&mut doc, &fwd).unwrap();
            prop_assert_eq!(&doc, &b);
            json_patch::patch(&mut doc, &inv).unwrap();
            prop_assert_eq!(&doc, &a);
        }
    }
}
