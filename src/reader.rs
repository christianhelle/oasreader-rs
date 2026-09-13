//! Reading OpenAPI specifications and merging their external references.

use std::fmt;

use serde_json::Value;

use crate::{
    DefaultLoader, Diagnostic, OpenApiContentFormat, OpenApiSource, OpenApiSpecificationVersion,
    OpenApiStats, RawOpenApiLoadError, ResourceLoader, SpecificationVersionDetectionError,
    classify_source, inspect_value, load_raw_document_from_source, merge_external_references,
};

/// A specification read by [`read`] or [`OpenApiReader`], with external references merged.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadResult {
    /// The path or URL the main document was loaded from.
    pub source: OpenApiSource,
    /// The serialization format of the main document.
    pub format: OpenApiContentFormat,
    /// The specification version the main document declares.
    pub specification_version: OpenApiSpecificationVersion,
    /// The merged document tree.
    pub document: Value,
    /// Whether the main document referenced other files or URLs.
    pub contained_external_references: bool,
    /// Problems found while merging external references.
    pub diagnostics: Vec<Diagnostic>,
}

impl ReadResult {
    /// Counts the paths, operations and components of the merged document.
    pub fn stats(&self) -> OpenApiStats {
        inspect_value(&self.document, self.specification_version)
    }

    /// Parses the merged document into the typed model for its specification version.
    #[cfg(feature = "typed")]
    pub fn typed(
        &self,
        options: crate::TypedParseOptions,
    ) -> Result<crate::TypedOpenApiDocument, crate::TypedOpenApiParseError> {
        crate::parse_typed_document(&self.document, self.specification_version, options)
    }
}

/// Errors raised while reading a specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The main document could not be loaded or decoded.
    Load(RawOpenApiLoadError),
    /// The main document does not declare a supported specification version.
    VersionDetection(SpecificationVersionDetectionError),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => write!(f, "{error}"),
            Self::VersionDetection(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ReadError {}

/// Reads specifications with a configurable loader.
///
/// # Examples
///
/// ```
/// use oasreader::{FetchError, OpenApiReader, OpenApiSource};
///
/// let reader = OpenApiReader::new().with_loader(|source: &OpenApiSource| {
///     match source.to_string().as_str() {
///         "main.yaml" => Ok("openapi: 3.0.3\ncomponents:\n  schemas:\n    Pet:\n      $ref: 'pet.yaml#/components/schemas/Pet'\n".to_string()),
///         "pet.yaml" => Ok("components:\n  schemas:\n    Pet:\n      type: object\n".to_string()),
///         _ => Err(FetchError::FileRead { path: source.to_string().into(), reason: "not found".into() }),
///     }
/// });
///
/// let result = reader.read("main.yaml").unwrap();
///
/// assert!(result.contained_external_references);
/// assert_eq!(result.document["components"]["schemas"]["Pet"]["type"], "object");
/// ```
pub struct OpenApiReader {
    loader: Box<dyn ResourceLoader>,
}

impl OpenApiReader {
    /// Creates a reader that uses the [`DefaultLoader`].
    pub fn new() -> Self {
        Self {
            loader: Box::new(DefaultLoader::default()),
        }
    }

    /// Uses `loader` to load the main document and every referenced file.
    pub fn with_loader(mut self, loader: impl ResourceLoader + 'static) -> Self {
        self.loader = Box::new(loader);
        self
    }

    /// Reads a specification from a path or URL.
    pub fn read(&self, input: &str) -> Result<ReadResult, ReadError> {
        let source = classify_source(input)
            .map_err(|error| ReadError::Load(RawOpenApiLoadError::SourceClassification(error)))?;
        self.read_source(source)
    }

    /// Reads a specification from a classified source.
    pub fn read_source(&self, source: OpenApiSource) -> Result<ReadResult, ReadError> {
        let raw =
            load_raw_document_from_source(source, self.loader.as_ref()).map_err(ReadError::Load)?;
        let specification_version = raw
            .specification_version()
            .map_err(ReadError::VersionDetection)?;
        let source = raw.source().clone();
        let format = raw.format();

        let mut document = raw.into_value();
        let report = merge_external_references(&mut document, &source, self.loader.as_ref());

        Ok(ReadResult {
            source,
            format,
            specification_version,
            document,
            contained_external_references: report.contained_external_references,
            diagnostics: report.diagnostics,
        })
    }
}

impl Default for OpenApiReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads a specification from a path or URL and merges its external references.
///
/// This is the Rust counterpart of `OpenApiMultiFileReader.Read` in the .NET oasreader. Use
/// [`OpenApiReader`] to configure how files and URLs are loaded.
///
/// # Examples
///
/// ```no_run
/// let result = oasreader::read("petstore.yaml").unwrap();
///
/// println!("{}", result.document["components"]["schemas"]["Pet"]);
/// ```
pub fn read(input: &str) -> Result<ReadResult, ReadError> {
    OpenApiReader::new().read(input)
}
