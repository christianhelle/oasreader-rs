//! Read OpenAPI specifications that are split across multiple files and merge their external
//! references into a single document.
//!
//! This is the Rust counterpart of the .NET [oasreader](https://github.com/christianhelle/oasreader)
//! and the shared OpenAPI toolkit for Rust tools such as httpgenerator and curlgenerator. It works
//! on Swagger 2.0, OpenAPI 3.0 and OpenAPI 3.1 documents, in JSON or YAML, from local files or
//! HTTP(S) URLs.
//!
//! # Reading a split specification
//!
//! [`read`] loads a document, merges every externally referenced component into its components
//! section and rewrites the references to point at the merged copies:
//!
//! ```no_run
//! let result = oasreader::read("petstore.yaml").unwrap();
//!
//! assert!(result.contained_external_references);
//! for diagnostic in &result.diagnostics {
//!     eprintln!("warning: {diagnostic}");
//! }
//! println!("{}", result.document["components"]["schemas"]["Pet"]);
//! ```
//!
//! Use [`OpenApiReader`] with a custom [`ResourceLoader`] to serve files from memory, add
//! authentication, or change HTTP settings:
//!
//! ```
//! use oasreader::{FetchError, OpenApiReader, OpenApiSource};
//!
//! let reader = OpenApiReader::new().with_loader(|source: &OpenApiSource| {
//!     match source.to_string().as_str() {
//!         "petstore.yaml" => Ok(r#"
//! openapi: 3.0.3
//! paths:
//!   /pets:
//!     get:
//!       responses:
//!         '200':
//!           description: ok
//!           content:
//!             application/json:
//!               schema:
//!                 $ref: 'components.yaml#/components/schemas/Pet'
//! "#.to_string()),
//!         "components.yaml" => Ok("components:\n  schemas:\n    Pet:\n      type: object\n".to_string()),
//!         other => Err(FetchError::FileRead { path: other.into(), reason: "not found".into() }),
//!     }
//! });
//!
//! let result = reader.read("petstore.yaml").unwrap();
//! let schema = &result.document["paths"]["/pets"]["get"]["responses"]["200"]["content"]["application/json"]["schema"];
//!
//! assert_eq!(schema["$ref"], "#/components/schemas/Pet");
//! assert_eq!(result.document["components"]["schemas"]["Pet"]["type"], "object");
//! ```
//!
//! # Building blocks
//!
//! - [`classify_source`], [`detect_content_format`] and [`detect_specification_version`] identify
//!   where a document comes from, how it is encoded and which specification it follows.
//! - [`load_raw_document`] and [`decode_raw_document`] load and decode a single document.
//! - [`contains_external_references`] and [`merge_external_references`] work on any document tree.
//! - [`serialize_document`] writes a document back to JSON or YAML.
//! - [`inspect_value`] and [`ReadResult::stats`] count paths, operations and components.
//! - With the `typed` feature, `parse_typed_document` and `ReadResult::typed` parse documents into
//!   the [`openapiv3`](https://docs.rs/openapiv3) and [`openapiv3_1`](https://docs.rs/openapiv3_1)
//!   models.
//!
//! # Features
//!
//! - `remote` (default): download documents and references over HTTP(S) with `ureq`.
//! - `typed`: typed OpenAPI 3.0 and 3.1 models.

mod fetch;
mod format;
mod inspect;
mod merge;
mod raw;
mod reader;
mod serialize;
mod source;
#[cfg(feature = "typed")]
mod typed;
mod version;

pub use fetch::{DefaultLoader, FetchError, HttpOptions, ResourceLoader};
pub use format::{
    ContentFormatDetectionError, OpenApiContentFormat, detect_content_format, sniff_content_format,
};
pub use inspect::{OpenApiStats, inspect_value};
pub use merge::{Diagnostic, MergeReport, contains_external_references, merge_external_references};
pub use raw::{
    RawOpenApiDocument, RawOpenApiLoadError, decode_raw_document, load_raw_document,
    load_raw_document_from_source,
};
pub use reader::{OpenApiReader, ReadError, ReadResult, read};
pub use serialize::{SerializationError, merge_external_references_as_string, serialize_document};
pub use source::{OpenApiSource, SourceClassificationError, classify_source};
#[cfg(feature = "typed")]
pub use typed::{
    TypedOpenApiDocument, TypedOpenApiParseError, TypedParseOptions, parse_openapi30_document,
    parse_openapi31_document, parse_typed_document,
};
pub use version::{
    OpenApiSpecificationVersion, SpecificationVersionDetectionError, detect_specification_version,
};
