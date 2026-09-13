//! Serialization of document trees back to JSON or YAML text.

use std::fmt;

use serde_json::Value;

use crate::{OpenApiContentFormat, OpenApiSource, ResourceLoader, merge_external_references};

/// Errors raised while serializing a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationError {
    /// The format the document was serialized as.
    pub format: OpenApiContentFormat,
    /// The serializer failure description.
    pub reason: String,
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "could not serialize the OpenAPI document as {}: {}",
            self.format, self.reason
        )
    }
}

impl std::error::Error for SerializationError {}

/// Serializes a document tree as pretty-printed JSON or as YAML, keeping key order.
///
/// # Examples
///
/// ```
/// use oasreader::{OpenApiContentFormat, serialize_document};
/// use serde_json::json;
///
/// let yaml = serialize_document(&json!({ "openapi": "3.1.0" }), OpenApiContentFormat::Yaml).unwrap();
///
/// assert_eq!(yaml, "openapi: 3.1.0\n");
/// ```
pub fn serialize_document(
    document: &Value,
    format: OpenApiContentFormat,
) -> Result<String, SerializationError> {
    let serialized = match format {
        OpenApiContentFormat::Json => {
            serde_json::to_string_pretty(document).map_err(|error| error.to_string())
        }
        OpenApiContentFormat::Yaml => {
            yaml_serde::to_string(document).map_err(|error| error.to_string())
        }
    };

    serialized.map_err(|reason| SerializationError { format, reason })
}

/// Merges external references into `document` and serializes the result.
///
/// The output is YAML when `source` has a `.yaml` or `.yml` extension and JSON otherwise, as in
/// `MergeExternalReferencesAsStringAsync` of the .NET oasreader. Unlike the .NET version, the
/// document keeps the specification version it declares.
pub fn merge_external_references_as_string(
    mut document: Value,
    source: &OpenApiSource,
    loader: &dyn ResourceLoader,
) -> Result<String, SerializationError> {
    merge_external_references(&mut document, source, loader);

    let format = match source.format_hint() {
        Some(OpenApiContentFormat::Yaml) => OpenApiContentFormat::Yaml,
        _ => OpenApiContentFormat::Json,
    };
    serialize_document(&document, format)
}
