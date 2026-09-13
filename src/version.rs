//! Detection of the OpenAPI specification version declared by a document.

use std::fmt;

use serde_json::Value;

/// The specification family a document belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenApiSpecificationVersion {
    /// A Swagger 2.0 document.
    Swagger2,
    /// An OpenAPI 3.0.x document.
    OpenApi30,
    /// An OpenAPI 3.1.x document.
    OpenApi31,
}

/// Errors raised while detecting the top-level version field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecificationVersionDetectionError {
    /// Neither `openapi` nor `swagger` was present at the top level.
    MissingVersionField,
    /// The version field was not a non-empty string.
    InvalidVersionFieldType {
        /// The offending field name.
        field: &'static str,
    },
    /// The version is outside the supported specification families.
    UnsupportedVersion {
        /// The field that declared the version.
        field: &'static str,
        /// The declared version.
        value: String,
    },
}

impl fmt::Display for OpenApiSpecificationVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Swagger2 => write!(f, "Swagger 2.0"),
            Self::OpenApi30 => write!(f, "OpenAPI 3.0.x"),
            Self::OpenApi31 => write!(f, "OpenAPI 3.1.x"),
        }
    }
}

impl fmt::Display for SpecificationVersionDetectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVersionField => write!(
                f,
                "OpenAPI document is missing a top-level 'openapi' or 'swagger' version field"
            ),
            Self::InvalidVersionFieldType { field } => {
                write!(f, "OpenAPI document field '{field}' must be a string")
            }
            Self::UnsupportedVersion { field, value } => {
                write!(
                    f,
                    "unsupported OpenAPI version '{value}' in field '{field}'"
                )
            }
        }
    }
}

impl std::error::Error for SpecificationVersionDetectionError {}

/// Detects the specification version from a decoded document.
///
/// # Examples
///
/// ```
/// use oasreader::{OpenApiSpecificationVersion, detect_specification_version};
/// use serde_json::json;
///
/// let version = detect_specification_version(&json!({ "openapi": "3.1.0" })).unwrap();
///
/// assert_eq!(version, OpenApiSpecificationVersion::OpenApi31);
/// ```
pub fn detect_specification_version(
    value: &Value,
) -> Result<OpenApiSpecificationVersion, SpecificationVersionDetectionError> {
    let (field, supported): (_, &[_]) = if value.get("openapi").is_some() {
        (
            "openapi",
            &[
                ((3, 0), OpenApiSpecificationVersion::OpenApi30),
                ((3, 1), OpenApiSpecificationVersion::OpenApi31),
            ],
        )
    } else if value.get("swagger").is_some() {
        (
            "swagger",
            &[((2, 0), OpenApiSpecificationVersion::Swagger2)],
        )
    } else {
        return Err(SpecificationVersionDetectionError::MissingVersionField);
    };

    let version = value[field]
        .as_str()
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .ok_or(SpecificationVersionDetectionError::InvalidVersionFieldType { field })?;

    let major_minor = parse_major_minor(version);
    supported
        .iter()
        .find(|(candidate, _)| Some(*candidate) == major_minor)
        .map(|(_, specification)| *specification)
        .ok_or_else(|| SpecificationVersionDetectionError::UnsupportedVersion {
            field,
            value: version.to_string(),
        })
}

fn parse_major_minor(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.split('.').map(parse_numeric_prefix);
    Some((parts.next()??, parts.next()??))
}

fn parse_numeric_prefix(component: &str) -> Option<u64> {
    let end = component
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(component.len());

    component[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn detect(
        value: Value,
    ) -> Result<OpenApiSpecificationVersion, SpecificationVersionDetectionError> {
        detect_specification_version(&value)
    }

    #[test]
    fn detects_swagger_two_documents() {
        assert_eq!(
            detect(json!({ "swagger": "2.0" })),
            Ok(OpenApiSpecificationVersion::Swagger2)
        );
    }

    #[test]
    fn detects_openapi_thirty_documents() {
        assert_eq!(
            detect(json!({ "openapi": "3.0.3" })),
            Ok(OpenApiSpecificationVersion::OpenApi30)
        );
        assert_eq!(
            detect(json!({ "openapi": " 3.0.0-rc1 " })),
            Ok(OpenApiSpecificationVersion::OpenApi30)
        );
    }

    #[test]
    fn detects_openapi_thirty_one_documents() {
        assert_eq!(
            detect(json!({ "openapi": "3.1.0" })),
            Ok(OpenApiSpecificationVersion::OpenApi31)
        );
    }

    #[test]
    fn reports_missing_version_fields() {
        let error = detect(json!({ "info": {} })).unwrap_err();

        assert_eq!(
            error,
            SpecificationVersionDetectionError::MissingVersionField
        );
        assert_eq!(
            error.to_string(),
            "OpenAPI document is missing a top-level 'openapi' or 'swagger' version field"
        );
    }

    #[test]
    fn reports_invalid_version_field_types() {
        let error = detect(json!({ "openapi": 3.1 })).unwrap_err();

        assert_eq!(
            error,
            SpecificationVersionDetectionError::InvalidVersionFieldType { field: "openapi" }
        );
        assert_eq!(
            error.to_string(),
            "OpenAPI document field 'openapi' must be a string"
        );
    }

    #[test]
    fn reports_unsupported_versions() {
        let error = detect(json!({ "openapi": "4.0.0" })).unwrap_err();

        assert_eq!(
            error,
            SpecificationVersionDetectionError::UnsupportedVersion {
                field: "openapi",
                value: "4.0.0".to_string(),
            }
        );
        assert_eq!(
            error.to_string(),
            "unsupported OpenAPI version '4.0.0' in field 'openapi'"
        );
        assert!(detect(json!({ "swagger": "1.2" })).is_err());
    }

    #[test]
    fn versions_render_friendly_names() {
        assert_eq!(
            OpenApiSpecificationVersion::Swagger2.to_string(),
            "Swagger 2.0"
        );
        assert_eq!(
            OpenApiSpecificationVersion::OpenApi30.to_string(),
            "OpenAPI 3.0.x"
        );
        assert_eq!(
            OpenApiSpecificationVersion::OpenApi31.to_string(),
            "OpenAPI 3.1.x"
        );
    }
}
