//! Decoding of OpenAPI content into a generic JSON tree that keeps its source metadata.

use std::fmt;

use serde_json::Value;

use crate::{
    ContentFormatDetectionError, DefaultLoader, FetchError, OpenApiContentFormat, OpenApiSource,
    OpenApiSpecificationVersion, ResourceLoader, SourceClassificationError,
    SpecificationVersionDetectionError, classify_source, detect_content_format,
    detect_specification_version, sniff_content_format,
};

/// A decoded OpenAPI document together with where it came from and how it was encoded.
#[derive(Debug, Clone, PartialEq)]
pub struct RawOpenApiDocument {
    source: OpenApiSource,
    format: OpenApiContentFormat,
    content: String,
    value: Value,
}

/// Errors raised while decoding OpenAPI content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawOpenApiLoadError {
    /// The input could not be classified as a path or URL.
    SourceClassification(SourceClassificationError),
    /// The content could not be fetched.
    Fetch(FetchError),
    /// The content format could not be detected.
    FormatDetection {
        /// The source of the content.
        source: OpenApiSource,
        /// The detection failure.
        error: ContentFormatDetectionError,
    },
    /// The content could not be decoded as the detected format.
    Decode {
        /// The source of the content.
        source: OpenApiSource,
        /// The format the content was decoded as.
        format: OpenApiContentFormat,
        /// The parser failure description.
        reason: String,
    },
}

impl fmt::Display for RawOpenApiLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceClassification(error) => write!(f, "{error}"),
            Self::Fetch(error) => write!(f, "{error}"),
            Self::FormatDetection { source, error } => {
                write!(f, "could not detect the format of {source}: {error}")
            }
            Self::Decode {
                source,
                format,
                reason,
            } => write!(f, "could not decode {source} as {format}: {reason}"),
        }
    }
}

impl std::error::Error for RawOpenApiLoadError {}

impl RawOpenApiDocument {
    /// Returns the path or URL the document was loaded from.
    pub fn source(&self) -> &OpenApiSource {
        &self.source
    }

    /// Returns the detected serialization format.
    pub fn format(&self) -> OpenApiContentFormat {
        self.format
    }

    /// Returns the original text content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Returns the decoded document tree.
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the document and returns the decoded document tree.
    pub fn into_value(self) -> Value {
        self.value
    }

    /// Detects the specification version from the decoded document tree.
    pub fn specification_version(
        &self,
    ) -> Result<OpenApiSpecificationVersion, SpecificationVersionDetectionError> {
        detect_specification_version(&self.value)
    }
}

/// Loads and decodes a document from a path or URL with the [`DefaultLoader`].
///
/// # Examples
///
/// ```no_run
/// let raw = oasreader::load_raw_document("specs/petstore.yaml").unwrap();
///
/// println!("{} document loaded from {}", raw.format(), raw.source());
/// ```
pub fn load_raw_document(input: &str) -> Result<RawOpenApiDocument, RawOpenApiLoadError> {
    let source = classify_source(input).map_err(RawOpenApiLoadError::SourceClassification)?;
    load_raw_document_from_source(source, &DefaultLoader::default())
}

/// Loads and decodes a document from a classified source with a custom loader.
pub fn load_raw_document_from_source(
    source: OpenApiSource,
    loader: &dyn ResourceLoader,
) -> Result<RawOpenApiDocument, RawOpenApiLoadError> {
    let content = loader.load(&source).map_err(RawOpenApiLoadError::Fetch)?;
    decode_raw_document(source, content)
}

/// Decodes in-memory content into a [`RawOpenApiDocument`].
///
/// The format is taken from the source extension when it has one, and sniffed from the content
/// otherwise. When decoding with the extension's format fails, the sniffed format is tried too.
///
/// # Examples
///
/// ```
/// use oasreader::{OpenApiContentFormat, OpenApiSource, decode_raw_document};
/// use std::path::PathBuf;
///
/// let raw = decode_raw_document(
///     OpenApiSource::Path(PathBuf::from("petstore.yaml")),
///     "openapi: 3.0.0\ninfo:\n  title: Example\npaths: {}\n",
/// )
/// .unwrap();
///
/// assert_eq!(raw.format(), OpenApiContentFormat::Yaml);
/// assert_eq!(raw.value()["info"]["title"], "Example");
/// ```
pub fn decode_raw_document(
    source: OpenApiSource,
    content: impl Into<String>,
) -> Result<RawOpenApiDocument, RawOpenApiLoadError> {
    let content = content.into();
    let format = detect_content_format(Some(&source), &content).map_err(|error| {
        RawOpenApiLoadError::FormatDetection {
            source: source.clone(),
            error,
        }
    })?;

    let decoded = decode_content(format, &content).map(|value| (format, value));
    let (format, value) = match (decoded, sniff_content_format(&content)) {
        (Err(_), Ok(sniffed)) if sniffed != format => decode_content(sniffed, &content)
            .map(|value| (sniffed, value))
            .map_err(|reason| (format, reason)),
        (decoded, _) => decoded.map_err(|reason| (format, reason)),
    }
    .map_err(|(format, reason)| RawOpenApiLoadError::Decode {
        source: source.clone(),
        format,
        reason,
    })?;

    Ok(RawOpenApiDocument {
        source,
        format,
        content,
        value,
    })
}

fn decode_content(format: OpenApiContentFormat, content: &str) -> Result<Value, String> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);

    match format {
        OpenApiContentFormat::Json => serde_json::from_str(content).map_err(|e| e.to_string()),
        OpenApiContentFormat::Yaml => yaml_serde::from_str(content).map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    fn path(value: &str) -> OpenApiSource {
        OpenApiSource::Path(PathBuf::from(value))
    }

    #[test]
    fn decodes_json_content() {
        let raw = decode_raw_document(
            path("openapi.json"),
            r#"{ "openapi": "3.0.1", "paths": {} }"#,
        )
        .unwrap();

        assert_eq!(raw.format(), OpenApiContentFormat::Json);
        assert_eq!(raw.value(), &json!({ "openapi": "3.0.1", "paths": {} }));
        assert_eq!(raw.source(), &path("openapi.json"));
        assert_eq!(raw.content(), r#"{ "openapi": "3.0.1", "paths": {} }"#);
    }

    #[test]
    fn decodes_yaml_content_and_keeps_key_order() {
        let raw = decode_raw_document(
            path("openapi.yaml"),
            "openapi: 3.1.0\npaths: {}\ninfo:\n  title: Example\n",
        )
        .unwrap();

        assert_eq!(raw.format(), OpenApiContentFormat::Yaml);
        let keys: Vec<_> = raw.value().as_object().unwrap().keys().collect();
        assert_eq!(keys, ["openapi", "paths", "info"]);
        assert_eq!(
            raw.specification_version(),
            Ok(OpenApiSpecificationVersion::OpenApi31)
        );
    }

    #[test]
    fn strips_a_leading_byte_order_mark() {
        let raw = decode_raw_document(path("openapi"), "\u{feff}{\"swagger\":\"2.0\"}").unwrap();

        assert_eq!(raw.value(), &json!({ "swagger": "2.0" }));
    }

    #[test]
    fn falls_back_to_sniffed_format_when_the_extension_is_wrong() {
        let raw = decode_raw_document(path("openapi.json"), "openapi: 3.0.0\npaths: {}\n").unwrap();

        assert_eq!(raw.format(), OpenApiContentFormat::Yaml);
        assert_eq!(raw.value(), &json!({ "openapi": "3.0.0", "paths": {} }));
    }

    #[test]
    fn reports_malformed_content() {
        let error = decode_raw_document(path("openapi.json"), "{ not json ").unwrap_err();

        assert!(matches!(
            &error,
            RawOpenApiLoadError::Decode {
                format: OpenApiContentFormat::Json,
                ..
            }
        ));
        assert!(
            error
                .to_string()
                .starts_with("could not decode openapi.json as JSON: ")
        );
    }

    #[test]
    fn reports_undetectable_content() {
        let error = decode_raw_document(path("openapi"), "   ").unwrap_err();

        assert_eq!(
            error,
            RawOpenApiLoadError::FormatDetection {
                source: path("openapi"),
                error: ContentFormatDetectionError::EmptyContent,
            }
        );
        assert_eq!(
            error.to_string(),
            "could not detect the format of openapi: OpenAPI content cannot be empty"
        );
    }

    #[test]
    fn load_raw_document_reads_and_decodes_local_files() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("petstore.json");
        std::fs::write(&file, r#"{ "swagger": "2.0" }"#).unwrap();

        let raw = load_raw_document(file.to_str().unwrap()).unwrap();

        assert_eq!(raw.source(), &OpenApiSource::Path(file));
        assert_eq!(raw.value(), &json!({ "swagger": "2.0" }));
    }

    #[test]
    fn load_raw_document_reports_classification_and_fetch_errors() {
        assert_eq!(
            load_raw_document("  ").unwrap_err().to_string(),
            "the OpenAPI path is empty"
        );

        let error = load_raw_document("./does-not-exist.json").unwrap_err();
        assert!(matches!(
            error,
            RawOpenApiLoadError::Fetch(FetchError::FileRead { .. })
        ));
        assert!(
            error
                .to_string()
                .starts_with("could not open the file at ./does-not-exist.json: ")
        );
    }

    #[test]
    fn load_raw_document_from_source_uses_the_given_loader() {
        let loader = |_: &OpenApiSource| Ok("openapi: 3.0.0\n".to_string());

        let raw = load_raw_document_from_source(path("memory.yaml"), &loader).unwrap();

        assert_eq!(raw.value(), &json!({ "openapi": "3.0.0" }));
    }
}
