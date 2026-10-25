//! OpenRPC generation from the method registry.
//!
//! No crate generates OpenRPC from jsonrpsee, so this is ours: schemars gives
//! each params/result schema, openrpc-types gives the document structure.
//! Subschemas are inlined so the document has no dangling `$ref`s.

use openrpc_types::{ContentDescriptor, Info, Method, OpenRPC, ParamStructure, ReferenceOr};
use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema};
use serde_json::Value;

use crate::MethodSpec;

pub(crate) fn schema_for<T: JsonSchema>() -> Schema {
    let settings = SchemaSettings::draft07().with(|s| s.inline_subschemas = true);
    let mut schema = settings.into_generator().into_root_schema_for::<T>();
    schema.remove("$schema");
    schema
}

/// Build the document. Params are by-name: one descriptor per top-level property
/// of the params schema, so tools that render OpenRPC show real argument names.
pub(crate) fn document(title: &str, version: &str, specs: &[MethodSpec], with_describe: bool) -> OpenRPC {
    let mut methods: Vec<ReferenceOr<Method>> = specs.iter().map(|s| ReferenceOr::Item(method(s))).collect();
    if with_describe {
        // system.describe is registered after the document is built; list it too.
        methods.push(ReferenceOr::Item(Method {
            name: "system.describe".into(),
            summary: Some("OpenRPC document for every method and plugin".into()),
            params: vec![],
            result: Some(ReferenceOr::Item(ContentDescriptor {
                name: "result".into(),
                schema: schemars::json_schema!({ "type": "object" }),
                ..Default::default()
            })),
            param_structure: Some(ParamStructure::ByName),
            ..Default::default()
        }));
    }
    OpenRPC {
        info: Info { title: title.into(), version: version.into(), ..Default::default() },
        methods,
        ..Default::default()
    }
}

fn method(spec: &MethodSpec) -> Method {
    method_from(spec.name, spec.summary, spec.mutates, &spec.params, spec.result.clone())
}

/// A method added at run time (plugins), as an OpenRPC method object.
pub(crate) fn dyn_method(m: &crate::DynMethod) -> Value {
    let schema = |v: &Value| Schema::try_from(v.clone()).unwrap_or_else(|_| schemars::json_schema!(true));
    let mut out = method_from(&m.name, &m.summary, m.mutates, &schema(&m.params), schema(&m.result));
    out.extensions.0.insert("x-plugin".into(), Value::String(m.plugin.clone()));
    serde_json::to_value(out).unwrap_or(Value::Null)
}

fn method_from(name: &str, summary: &str, mutates: bool, params_schema: &Schema, result: Schema) -> Method {
    let required: Vec<String> = params_schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let params = params_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| {
            props
                .iter()
                .map(|(name, schema)| {
                    ReferenceOr::Item(ContentDescriptor {
                        name: name.clone(),
                        required: Some(required.contains(name)),
                        schema: Schema::try_from(schema.clone()).unwrap_or_else(|_| schemars::json_schema!(true)),
                        ..Default::default()
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let mut m = Method {
        name: name.into(),
        summary: Some(summary.into()),
        params,
        result: Some(ReferenceOr::Item(ContentDescriptor {
            name: "result".into(),
            schema: result,
            ..Default::default()
        })),
        param_structure: Some(ParamStructure::ByName),
        ..Default::default()
    };
    m.extensions.0.insert("x-mutates".into(), Value::Bool(mutates));
    m
}
