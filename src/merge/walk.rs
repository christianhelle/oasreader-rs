//! Traversal of the `$ref`s in a document tree.
//!
//! Some keywords hold literal data (examples, defaults, enum values, extensions) where an object
//! with a `$ref` key is a value rather than a reference, so their subtrees are skipped. Keys of
//! maps of named entries (`properties`, `responses`, `schemas`, ...) are names rather than
//! keywords, so a property called `default` is still traversed.

use serde_json::Value;

/// Keywords whose values are named entries rather than keywords.
const NAMED_ENTRY_MAPS: &[&str] = &[
    "$defs",
    "callbacks",
    "content",
    "definitions",
    "dependentSchemas",
    "encoding",
    "examples",
    "headers",
    "links",
    "parameters",
    "pathItems",
    "paths",
    "patternProperties",
    "properties",
    "requestBodies",
    "responses",
    "schemas",
    "securityDefinitions",
    "securitySchemes",
    "variables",
    "webhooks",
];

/// Keywords whose values are literal data.
const LITERAL_KEYWORDS: &[&str] = &["const", "default", "enum", "example", "value"];

/// Returns `true` when the document references another file or URL.
///
/// Every `$ref` in the document is checked, not only the ones under `paths`.
///
/// # Examples
///
/// ```
/// use oasreader::contains_external_references;
/// use serde_json::json;
///
/// let split = json!({
///     "openapi": "3.0.3",
///     "components": { "schemas": { "Pet": { "$ref": "pets.yaml#/components/schemas/Pet" } } }
/// });
/// let local = json!({ "openapi": "3.0.3", "paths": { "/pets": { "$ref": "#/components/pathItems/Pets" } } });
///
/// assert!(contains_external_references(&split));
/// assert!(!contains_external_references(&local));
/// ```
pub fn contains_external_references(document: &Value) -> bool {
    let mut found = false;
    for_each_reference(document, &mut |reference| {
        found |= is_external_reference(reference);
    });
    found
}

/// Returns `true` when a `$ref` value points outside of the current document.
pub(crate) fn is_external_reference(reference: &str) -> bool {
    !reference.is_empty() && !reference.starts_with('#')
}

/// Returns `true` when the subtree under `key` should not be searched for references.
pub(crate) fn is_literal_subtree(key: &str, value: &Value, in_named_entry_map: bool) -> bool {
    if in_named_entry_map {
        return false;
    }

    key.starts_with("x-")
        || LITERAL_KEYWORDS.contains(&key)
        || (key == "examples" && value.is_array())
}

/// Returns `true` when the keys of the object under `key` are entry names.
pub(crate) fn is_named_entry_map(key: &str, in_named_entry_map: bool) -> bool {
    !in_named_entry_map && NAMED_ENTRY_MAPS.contains(&key)
}

/// Calls `visit` with every `$ref` string in the document.
pub(crate) fn for_each_reference(document: &Value, visit: &mut dyn FnMut(&str)) {
    walk(document, false, visit);
}

fn walk(value: &Value, in_named_entry_map: bool, visit: &mut dyn FnMut(&str)) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if !in_named_entry_map && key == "$ref" {
                    if let Some(reference) = child.as_str() {
                        visit(reference);
                    }
                    continue;
                }
                if is_literal_subtree(key, child, in_named_entry_map) {
                    continue;
                }
                walk(child, is_named_entry_map(key, in_named_entry_map), visit);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, false, visit);
            }
        }
        _ => {}
    }
}
