//! A small JSON Schema checker for the keywords the generated schemas use:
//! `$ref` into `$defs`, `type`, `enum`, `const`, `pattern`, `maxLength`,
//! `minimum`, `maximum`, `required`, `properties`, `items`, `anyOf`, `allOf` and
//! `oneOf`. Unknown keywords are ignored. It exists to test our own documents,
//! not as a general validator, and is shared by the unit tests of
//! `outcome::schema` and the integration tests.

use regex::Regex;
use serde_json::Value;

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect()
}

/// Whether `instance` satisfies `schema`, resolving references in `root`.
pub fn conforms(root: &Value, schema: &Value, instance: &Value) -> bool {
    let Some(keywords) = schema.as_object() else {
        return schema.as_bool().unwrap_or(false);
    };
    keywords
        .iter()
        .all(|(keyword, rule)| match keyword.as_str() {
            "$ref" => {
                let name = rule.as_str().unwrap().rsplit('/').next().unwrap();
                conforms(root, &root["$defs"][name], instance)
            }
            "type" => {
                let allowed: Vec<&str> = match rule {
                    Value::String(one) => vec![one.as_str()],
                    other => strings(other),
                };
                allowed.iter().any(|kind| match *kind {
                    "string" => instance.is_string(),
                    "null" => instance.is_null(),
                    "boolean" => instance.is_boolean(),
                    "integer" => instance.is_i64() || instance.is_u64(),
                    "array" => instance.is_array(),
                    "object" => instance.is_object(),
                    _ => false,
                })
            }
            "enum" => rule.as_array().unwrap().contains(instance),
            "const" => rule == instance,
            "pattern" => instance
                .as_str()
                .is_none_or(|text| Regex::new(rule.as_str().unwrap()).unwrap().is_match(text)),
            "maxLength" => instance
                .as_str()
                .is_none_or(|text| text.len() as u64 <= rule.as_u64().unwrap()),
            "minimum" => instance
                .as_i64()
                .is_none_or(|number| number >= rule.as_i64().unwrap()),
            "maximum" => instance
                .as_i64()
                .is_none_or(|number| number <= rule.as_i64().unwrap()),
            "required" => instance
                .as_object()
                .is_none_or(|object| strings(rule).iter().all(|key| object.contains_key(*key))),
            "properties" => instance.as_object().is_none_or(|object| {
                rule.as_object()
                    .unwrap()
                    .iter()
                    .all(|(key, sub)| object.get(key).is_none_or(|item| conforms(root, sub, item)))
            }),
            "items" => instance
                .as_array()
                .is_none_or(|items| items.iter().all(|item| conforms(root, rule, item))),
            "anyOf" => rule
                .as_array()
                .unwrap()
                .iter()
                .any(|sub| conforms(root, sub, instance)),
            "allOf" => rule
                .as_array()
                .unwrap()
                .iter()
                .all(|sub| conforms(root, sub, instance)),
            "oneOf" => {
                rule.as_array()
                    .unwrap()
                    .iter()
                    .filter(|sub| conforms(root, sub, instance))
                    .count()
                    == 1
            }
            _ => true,
        })
}
