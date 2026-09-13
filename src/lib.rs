//! Read OpenAPI specifications that are split across multiple files and merge their external
//! references into a single document.

mod source;

pub use source::{OpenApiSource, SourceClassificationError, classify_source};
