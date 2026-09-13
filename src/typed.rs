//! Parsing of merged documents into typed OpenAPI 3.0 and 3.1 models.

use std::fmt;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{OpenApiSpecificationVersion, detect_specification_version};

/// Options for [`parse_typed_document`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypedParseOptions {
    /// Returns [`TypedOpenApiDocument::OpenApi31Untyped`] instead of an error when an OpenAPI 3.1
    /// document does not fit the typed model.
    pub tolerate_invalid_openapi31: bool,
}

/// A document parsed into the typed model for its specification version.
pub enum TypedOpenApiDocument {
    /// A Swagger 2.0 document, for which no typed model exists. Use the document tree instead.
    Swagger2,
    /// An OpenAPI 3.0 document.
    OpenApi30(Box<openapiv3::OpenAPI>),
    /// An OpenAPI 3.1 document.
    OpenApi31(Box<openapiv3_1::OpenApi>),
    /// An OpenAPI 3.1 document that does not fit the typed model, such as a webhook-only document.
    /// Use the document tree instead.
    OpenApi31Untyped,
}

/// Errors raised while parsing a typed model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedOpenApiParseError {
    /// The document is not of the version the parser expects.
    UnsupportedVersion {
        /// The version the parser expects.
        expected: OpenApiSpecificationVersion,
        /// The version the document declares, when it declares a supported one.
        found: Option<OpenApiSpecificationVersion>,
    },
    /// The document does not fit the typed model.
    Deserialize {
        /// The version the document was parsed as.
        version: OpenApiSpecificationVersion,
        /// The deserializer failure description.
        reason: String,
    },
}

impl fmt::Display for TypedOpenApiParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion {
                expected,
                found: Some(found),
            } => write!(
                f,
                "the document is not an {expected} document (found {found})"
            ),
            Self::UnsupportedVersion {
                expected,
                found: None,
            } => write!(f, "the document is not an {expected} document"),
            Self::Deserialize { version, reason } => {
                write!(f, "could not deserialize the {version} document: {reason}")
            }
        }
    }
}

impl std::error::Error for TypedOpenApiParseError {}

impl fmt::Debug for TypedOpenApiDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variant = match self {
            Self::Swagger2 => "Swagger2",
            Self::OpenApi30(_) => "OpenApi30",
            Self::OpenApi31(_) => "OpenApi31",
            Self::OpenApi31Untyped => "OpenApi31Untyped",
        };
        f.write_str(variant)
    }
}

impl TypedOpenApiDocument {
    /// Returns the specification version of the document.
    pub fn specification_version(&self) -> OpenApiSpecificationVersion {
        match self {
            Self::Swagger2 => OpenApiSpecificationVersion::Swagger2,
            Self::OpenApi30(_) => OpenApiSpecificationVersion::OpenApi30,
            Self::OpenApi31(_) | Self::OpenApi31Untyped => OpenApiSpecificationVersion::OpenApi31,
        }
    }
}

/// Parses a document tree into the typed model for `version`.
///
/// Swagger 2.0 documents have no typed model and return [`TypedOpenApiDocument::Swagger2`].
/// OpenAPI 3.1 documents that declare only `webhooks`, or that do not fit the typed model when
/// [`TypedParseOptions::tolerate_invalid_openapi31`] is set, return
/// [`TypedOpenApiDocument::OpenApi31Untyped`].
///
/// # Examples
///
/// ```
/// use oasreader::{
///     OpenApiSpecificationVersion, TypedOpenApiDocument, TypedParseOptions, parse_typed_document,
/// };
/// use serde_json::json;
///
/// let document = json!({
///     "openapi": "3.0.3",
///     "info": { "title": "Example", "version": "1.0.0" },
///     "paths": {}
/// });
///
/// let typed = parse_typed_document(
///     &document,
///     OpenApiSpecificationVersion::OpenApi30,
///     TypedParseOptions::default(),
/// )
/// .unwrap();
///
/// assert!(matches!(typed, TypedOpenApiDocument::OpenApi30(_)));
/// ```
pub fn parse_typed_document(
    document: &Value,
    version: OpenApiSpecificationVersion,
    options: TypedParseOptions,
) -> Result<TypedOpenApiDocument, TypedOpenApiParseError> {
    match version {
        OpenApiSpecificationVersion::Swagger2 => Ok(TypedOpenApiDocument::Swagger2),
        OpenApiSpecificationVersion::OpenApi30 => deserialize(document, version)
            .map(|typed| TypedOpenApiDocument::OpenApi30(Box::new(typed))),
        OpenApiSpecificationVersion::OpenApi31 => match deserialize(document, version) {
            Ok(typed) => Ok(TypedOpenApiDocument::OpenApi31(Box::new(typed))),
            Err(_) if is_webhook_only(document) || options.tolerate_invalid_openapi31 => {
                Ok(TypedOpenApiDocument::OpenApi31Untyped)
            }
            Err(error) => Err(error),
        },
    }
}

/// Parses a document tree as OpenAPI 3.0.
pub fn parse_openapi30_document(
    document: &Value,
) -> Result<openapiv3::OpenAPI, TypedOpenApiParseError> {
    parse_versioned(document, OpenApiSpecificationVersion::OpenApi30)
}

/// Parses a document tree as OpenAPI 3.1.
pub fn parse_openapi31_document(
    document: &Value,
) -> Result<openapiv3_1::OpenApi, TypedOpenApiParseError> {
    parse_versioned(document, OpenApiSpecificationVersion::OpenApi31)
}

fn parse_versioned<T: DeserializeOwned>(
    document: &Value,
    expected: OpenApiSpecificationVersion,
) -> Result<T, TypedOpenApiParseError> {
    let found = detect_specification_version(document).ok();
    if found != Some(expected) {
        return Err(TypedOpenApiParseError::UnsupportedVersion { expected, found });
    }
    deserialize(document, expected)
}

fn deserialize<T: DeserializeOwned>(
    document: &Value,
    version: OpenApiSpecificationVersion,
) -> Result<T, TypedOpenApiParseError> {
    T::deserialize(document).map_err(|error| TypedOpenApiParseError::Deserialize {
        version,
        reason: error.to_string(),
    })
}

fn is_webhook_only(document: &Value) -> bool {
    document.get("paths").is_none() && document.get("webhooks").is_some_and(Value::is_object)
}
