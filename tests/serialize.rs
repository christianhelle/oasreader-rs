use std::path::PathBuf;

use oasreader::{
    DefaultLoader, OpenApiContentFormat, OpenApiSource, decode_raw_document,
    merge_external_references_as_string, serialize_document,
};
use serde_json::json;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn serializes_documents_as_pretty_json_keeping_key_order() {
    let document = json!({ "openapi": "3.0.3", "paths": {}, "info": { "title": "Example" } });

    let json = serialize_document(&document, OpenApiContentFormat::Json).unwrap();

    assert_eq!(
        json,
        "{\n  \"openapi\": \"3.0.3\",\n  \"paths\": {},\n  \"info\": {\n    \"title\": \"Example\"\n  }\n}"
    );
}

#[test]
fn serializes_documents_as_yaml_that_decodes_to_the_same_tree() {
    let document = json!({
        "openapi": "3.0.3",
        "paths": { "/pets": { "get": { "responses": { "200": { "description": "ok" } } } } }
    });

    let yaml = serialize_document(&document, OpenApiContentFormat::Yaml).unwrap();
    let decoded = decode_raw_document(OpenApiSource::Path("out.yaml".into()), yaml.clone())
        .unwrap()
        .into_value();

    assert!(yaml.starts_with("openapi: 3.0.3\n"), "{yaml}");
    assert_eq!(decoded, document);
}

#[test]
fn merges_yaml_input_into_a_yaml_string() {
    let source = OpenApiSource::Path(fixture("petstore.yaml"));
    let content = std::fs::read_to_string(fixture("petstore.yaml")).unwrap();
    let document = decode_raw_document(source.clone(), content)
        .unwrap()
        .into_value();

    let merged =
        merge_external_references_as_string(document, &source, &DefaultLoader::default()).unwrap();

    assert!(merged.contains("components:"));
    assert!(merged.contains("Pet:"));
    assert!(!merged.contains("petstore.components.yaml"));
}

#[test]
fn merges_json_input_into_a_json_string() {
    let source = OpenApiSource::Path(fixture("v3.0/weather.json"));
    let content = std::fs::read_to_string(fixture("v3.0/weather.json")).unwrap();
    let document = decode_raw_document(source.clone(), content)
        .unwrap()
        .into_value();

    let merged =
        merge_external_references_as_string(document, &source, &DefaultLoader::default()).unwrap();

    assert!(merged.trim_start().starts_with('{'));
    assert!(merged.contains("\"openapi\""));
}
