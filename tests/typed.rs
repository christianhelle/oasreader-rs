#![cfg(feature = "typed")]

use std::{fs, path::PathBuf};

use oasreader::{
    OpenApiSpecificationVersion, TypedOpenApiDocument, TypedOpenApiParseError, TypedParseOptions,
    parse_openapi30_document, parse_openapi31_document, parse_typed_document, read,
};
use serde_json::{Value, json};

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn minimal(version_field: &str, version: &str) -> Value {
    json!({ version_field: version, "info": { "title": "Example", "version": "1.0.0" }, "paths": {} })
}

#[test]
fn parses_openapi_thirty_documents() {
    let typed = parse_typed_document(
        &minimal("openapi", "3.0.2"),
        OpenApiSpecificationVersion::OpenApi30,
        TypedParseOptions::default(),
    )
    .unwrap();

    assert!(matches!(typed, TypedOpenApiDocument::OpenApi30(_)));
    assert_eq!(
        typed.specification_version(),
        OpenApiSpecificationVersion::OpenApi30
    );
}

#[test]
fn parses_openapi_thirty_one_documents() {
    let typed = parse_typed_document(
        &minimal("openapi", "3.1.0"),
        OpenApiSpecificationVersion::OpenApi31,
        TypedParseOptions::default(),
    )
    .unwrap();

    assert!(matches!(typed, TypedOpenApiDocument::OpenApi31(_)));
}

#[test]
fn keeps_swagger_two_documents_untyped() {
    let typed = parse_typed_document(
        &minimal("swagger", "2.0"),
        OpenApiSpecificationVersion::Swagger2,
        TypedParseOptions::default(),
    )
    .unwrap();

    assert!(matches!(typed, TypedOpenApiDocument::Swagger2));
    assert_eq!(
        typed.specification_version(),
        OpenApiSpecificationVersion::Swagger2
    );
}

#[test]
fn falls_back_to_untyped_for_webhook_only_openapi_thirty_one_documents() {
    let result = read(&fixture("v3.1/webhook-example.json")).unwrap();

    let typed = result.typed(TypedParseOptions::default()).unwrap();

    assert!(matches!(typed, TypedOpenApiDocument::OpenApi31Untyped));
    assert_eq!(
        typed.specification_version(),
        OpenApiSpecificationVersion::OpenApi31
    );
}

#[test]
fn falls_back_to_untyped_for_invalid_openapi_thirty_one_documents_only_when_tolerated() {
    let result = read(&fixture("v3.1/non-oauth-scopes.json")).unwrap();

    let strict = result.typed(TypedParseOptions::default()).unwrap_err();
    let tolerant = result
        .typed(TypedParseOptions {
            tolerate_invalid_openapi31: true,
        })
        .unwrap();

    assert!(matches!(
        strict,
        TypedOpenApiParseError::Deserialize {
            version: OpenApiSpecificationVersion::OpenApi31,
            ..
        }
    ));
    assert!(
        strict
            .to_string()
            .starts_with("could not deserialize the OpenAPI 3.1.x document: ")
    );
    assert!(matches!(tolerant, TypedOpenApiDocument::OpenApi31Untyped));
}

#[test]
fn version_specific_parsers_reject_other_versions() {
    let error = parse_openapi30_document(&minimal("openapi", "3.1.0")).unwrap_err();

    assert_eq!(
        error,
        TypedOpenApiParseError::UnsupportedVersion {
            expected: OpenApiSpecificationVersion::OpenApi30,
            found: Some(OpenApiSpecificationVersion::OpenApi31),
        }
    );
    assert_eq!(
        error.to_string(),
        "the document is not an OpenAPI 3.0.x document (found OpenAPI 3.1.x)"
    );
    parse_openapi31_document(&minimal("openapi", "3.1.0")).unwrap();
}

#[test]
fn parses_merged_multi_file_documents() {
    let result = read(&fixture("petstore.yaml")).unwrap();

    let TypedOpenApiDocument::OpenApi30(document) =
        result.typed(TypedParseOptions::default()).unwrap()
    else {
        panic!("expected an OpenAPI 3.0 document");
    };

    let schemas = &document.components.unwrap().schemas;
    assert!(schemas.contains_key("Pet"));
    assert!(schemas.contains_key("Category"));
}

#[test]
fn parses_every_valid_openapi_thirty_fixture() {
    for entry in fs::read_dir(fixture("v3.0")).unwrap() {
        let path = entry.unwrap().path();
        if path.ends_with("no-content.yaml") {
            continue;
        }
        let result = read(path.to_str().unwrap()).unwrap();

        let typed = result.typed(TypedParseOptions::default());

        assert!(
            matches!(typed, Ok(TypedOpenApiDocument::OpenApi30(_))),
            "{}",
            path.display()
        );
    }
}

#[test]
fn rejects_openapi_thirty_parameters_without_a_schema_or_content() {
    let result = read(&fixture("v3.0/no-content.yaml")).unwrap();

    let error = result.typed(TypedParseOptions::default()).unwrap_err();

    assert!(matches!(
        error,
        TypedOpenApiParseError::Deserialize {
            version: OpenApiSpecificationVersion::OpenApi30,
            ..
        }
    ));
}
