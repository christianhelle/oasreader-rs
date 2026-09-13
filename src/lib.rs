//! Read OpenAPI specifications that are split across multiple files and merge their external
//! references into a single document.

mod fetch;
mod format;
mod raw;
mod source;
mod version;

pub use fetch::{DefaultLoader, FetchError, HttpOptions, ResourceLoader};
pub use format::{
    ContentFormatDetectionError, OpenApiContentFormat, detect_content_format, sniff_content_format,
};
pub use raw::{
    RawOpenApiDocument, RawOpenApiLoadError, decode_raw_document, load_raw_document,
    load_raw_document_from_source,
};
pub use source::{OpenApiSource, SourceClassificationError, classify_source};
pub use version::{
    OpenApiSpecificationVersion, SpecificationVersionDetectionError, detect_specification_version,
};
