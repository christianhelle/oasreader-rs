//! Counting of the paths, operations and components a specification declares.

use serde_json::Value;

use crate::OpenApiSpecificationVersion;

/// The number of paths, operations and components a specification declares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpenApiStats {
    /// The number of path items.
    pub path_item_count: usize,
    /// The number of operations across all path items.
    pub operation_count: usize,
    /// The number of parameters.
    pub parameter_count: usize,
    /// The number of request bodies.
    pub request_body_count: usize,
    /// The number of response collections, one per operation.
    pub response_count: usize,
    /// The number of response headers.
    pub header_count: usize,
    /// The number of links.
    pub link_count: usize,
    /// The number of callbacks.
    pub callback_count: usize,
    /// The number of inline and component schemas.
    pub schema_count: usize,
}

/// Counts the paths, operations and components of a document tree.
///
/// The counts match the `OpenApiStats` visitor that the .NET generators run over a document.
/// Schema references are not counted; the referenced schema is counted where it is declared.
///
/// # Examples
///
/// ```
/// use oasreader::{OpenApiSpecificationVersion, inspect_value};
/// use serde_json::json;
///
/// let stats = inspect_value(
///     &json!({
///         "openapi": "3.0.0",
///         "paths": { "/pets": { "get": { "responses": { "200": { "description": "ok" } } } } }
///     }),
///     OpenApiSpecificationVersion::OpenApi30,
/// );
///
/// assert_eq!(stats.path_item_count, 1);
/// assert_eq!(stats.operation_count, 1);
/// ```
pub fn inspect_value(value: &Value, version: OpenApiSpecificationVersion) -> OpenApiStats {
    let mut stats = OpenApiStats::default();
    let swagger2 = version == OpenApiSpecificationVersion::Swagger2;

    walk_path_items(value, value.get("paths"), swagger2, &mut stats);
    walk_path_items(value, value.get("webhooks"), swagger2, &mut stats);
    walk_components(value, swagger2, &mut stats);

    stats
}

const HTTP_METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

/// Returns `true` when the value is a reference object, which the walker does not count.
fn is_reference(value: &Value) -> bool {
    value.get("$ref").is_some()
}

fn walk_path_items(root: &Value, items: Option<&Value>, swagger2: bool, stats: &mut OpenApiStats) {
    let Some(items) = items.and_then(Value::as_object) else {
        return;
    };

    for path_item in items.values() {
        walk_path_item(root, path_item, swagger2, stats);
    }
}

fn walk_path_item(root: &Value, path_item: &Value, swagger2: bool, stats: &mut OpenApiStats) {
    stats.path_item_count += 1;
    walk_parameters(path_item, swagger2, stats);

    let Some(members) = path_item.as_object() else {
        return;
    };

    for (method, operation) in members {
        if !HTTP_METHODS.contains(&method.to_ascii_lowercase().as_str()) {
            continue;
        }

        stats.operation_count += 1;
        walk_parameters(operation, swagger2, stats);
        walk_request_body(root, operation, swagger2, stats);
        walk_responses(root, operation, swagger2, stats);
        walk_callbacks(root, operation.get("callbacks"), swagger2, stats);
    }
}

/// Counts callbacks and walks the path items they declare.
fn walk_callbacks(
    root: &Value,
    callbacks: Option<&Value>,
    swagger2: bool,
    stats: &mut OpenApiStats,
) {
    let Some(callbacks) = callbacks.and_then(Value::as_object) else {
        return;
    };

    for callback in callbacks.values() {
        if is_reference(callback) {
            continue;
        }

        stats.callback_count += 1;
        walk_path_items(root, Some(callback), swagger2, stats);
    }
}

fn walk_parameters(owner: &Value, swagger2: bool, stats: &mut OpenApiStats) {
    let Some(parameters) = owner.get("parameters").and_then(Value::as_array) else {
        return;
    };

    for parameter in parameters {
        if is_reference(parameter) {
            continue;
        }

        let location = parameter.get("in").and_then(Value::as_str);
        if swagger2 && matches!(location, Some("body") | Some("formData")) {
            continue;
        }

        stats.parameter_count += 1;

        if swagger2 {
            walk_schema(parameter, stats);
        } else if let Some(schema) = parameter.get("schema") {
            walk_schema(schema, stats);
        }
    }
}

fn walk_request_body(root: &Value, operation: &Value, swagger2: bool, stats: &mut OpenApiStats) {
    if !swagger2 {
        if let Some(request_body) = operation.get("requestBody") {
            stats.request_body_count += 1;
            walk_content(request_body, stats);
        }

        return;
    }

    let Some(parameters) = operation.get("parameters").and_then(Value::as_array) else {
        return;
    };

    let media_types = media_type_count(root, operation, "consumes");
    let mut form_properties = 0;
    for parameter in parameters {
        match parameter.get("in").and_then(Value::as_str) {
            Some("body") => {
                stats.request_body_count += 1;
                if let Some(schema) = parameter.get("schema") {
                    walk_schema_per_media_type(schema, media_types, stats);
                }
            }
            Some("formData") => form_properties += 1,
            _ => {}
        }
    }

    if form_properties > 0 {
        stats.request_body_count += 1;
        // The converted form body is one inline object schema plus one schema per field.
        stats.schema_count += media_types * (1 + form_properties);
    }
}

fn walk_responses(root: &Value, operation: &Value, swagger2: bool, stats: &mut OpenApiStats) {
    let Some(responses) = operation.get("responses").and_then(Value::as_object) else {
        return;
    };

    stats.response_count += 1;
    let media_types = media_type_count(root, operation, "produces");

    for response in responses.values() {
        if let Some(headers) = response.get("headers").and_then(Value::as_object) {
            stats.header_count += headers.len();
            for header in headers.values() {
                if swagger2 {
                    walk_schema(header, stats);
                } else if let Some(schema) = header.get("schema") {
                    walk_schema(schema, stats);
                }
            }
        }

        if let Some(links) = response.get("links").and_then(Value::as_object) {
            stats.link_count += links.values().filter(|link| !is_reference(link)).count();
        }

        if swagger2 {
            if let Some(schema) = response.get("schema") {
                walk_schema_per_media_type(schema, media_types, stats);
            }
        } else {
            walk_content(response, stats);
        }
    }
}

/// Returns how many media types a Swagger 2.0 operation declares for `field`.
///
/// The .NET converter turns each entry into its own content entry, and the walker visits the
/// schema once per entry.
fn media_type_count(root: &Value, operation: &Value, field: &str) -> usize {
    operation
        .get(field)
        .or_else(|| root.get(field))
        .and_then(Value::as_array)
        .map(Vec::len)
        .filter(|count| *count > 0)
        .unwrap_or(1)
}

fn walk_schema_per_media_type(schema: &Value, media_types: usize, stats: &mut OpenApiStats) {
    for _ in 0..media_types {
        walk_schema(schema, stats);
    }
}

fn walk_content(owner: &Value, stats: &mut OpenApiStats) {
    let Some(content) = owner.get("content").and_then(Value::as_object) else {
        return;
    };

    for media_type in content.values() {
        if let Some(schema) = media_type.get("schema") {
            walk_schema(schema, stats);
        }
    }
}

fn walk_components(root: &Value, swagger2: bool, stats: &mut OpenApiStats) {
    if swagger2 {
        walk_schema_map(root.get("definitions"), stats);

        if let Some(parameters) = root.get("parameters").and_then(Value::as_object) {
            for parameter in parameters
                .values()
                .filter(|parameter| !is_reference(parameter))
            {
                stats.parameter_count += 1;
                walk_schema(parameter, stats);
            }
        }

        return;
    }

    let Some(components) = root.get("components") else {
        return;
    };

    walk_schema_map(components.get("schemas"), stats);

    if let Some(parameters) = components.get("parameters").and_then(Value::as_object) {
        for parameter in parameters
            .values()
            .filter(|parameter| !is_reference(parameter))
        {
            stats.parameter_count += 1;
            if let Some(schema) = parameter.get("schema") {
                walk_schema(schema, stats);
            }
        }
    }

    if let Some(request_bodies) = components.get("requestBodies").and_then(Value::as_object) {
        stats.request_body_count += request_bodies.len();
        for request_body in request_bodies.values() {
            walk_content(request_body, stats);
        }
    }

    if let Some(responses) = components.get("responses").and_then(Value::as_object) {
        for response in responses.values() {
            walk_content(response, stats);
        }
    }

    if let Some(headers) = components.get("headers").and_then(Value::as_object) {
        stats.header_count += headers.len();
        for header in headers.values() {
            if let Some(schema) = header.get("schema") {
                walk_schema(schema, stats);
            }
        }
    }

    if let Some(links) = components.get("links").and_then(Value::as_object) {
        stats.link_count += links.values().filter(|link| !is_reference(link)).count();
    }

    walk_callbacks(root, components.get("callbacks"), swagger2, stats);
}

fn walk_schema_map(schemas: Option<&Value>, stats: &mut OpenApiStats) {
    let Some(schemas) = schemas.and_then(Value::as_object) else {
        return;
    };

    for schema in schemas.values() {
        walk_schema(schema, stats);
    }
}

/// Counts an inline schema and everything nested inside it.
///
/// Reference objects are not counted, matching the walker which visits the referenced schema
/// through its own component entry instead.
fn walk_schema(schema: &Value, stats: &mut OpenApiStats) {
    if !schema.is_object() || schema.get("$ref").is_some() {
        return;
    }

    stats.schema_count += 1;

    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for property in properties.values() {
            walk_schema(property, stats);
        }
    }

    for key in ["items", "additionalProperties", "not"] {
        if let Some(nested) = schema.get(key) {
            walk_schema(nested, stats);
        }
    }

    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(nested) = schema.get(key).and_then(Value::as_array) {
            for entry in nested {
                walk_schema(entry, stats);
            }
        }
    }
}
