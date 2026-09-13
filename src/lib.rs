//! Read OpenAPI specifications that are split across multiple files and merge their external
//! references into a single document.

mod format;
mod source;
mod version;

pub use format::{
    ContentFormatDetectionError, OpenApiContentFormat, detect_content_format, sniff_content_format,
};
pub use source::{OpenApiSource, SourceClassificationError, classify_source};
pub use version::{
    OpenApiSpecificationVersion, SpecificationVersionDetectionError, detect_specification_version,
};
