use oasreader::{OpenApiSpecificationVersion, OpenApiStats, inspect_value, read};
use serde_json::{Value, json};

fn stats_of(value: &Value, version: OpenApiSpecificationVersion) -> OpenApiStats {
    inspect_value(value, version)
}

#[test]
fn counts_paths_operations_and_responses() {
    let stats = stats_of(
        &json!({
            "openapi": "3.0.0",
            "paths": {
                "/pets": {
                    "get": { "responses": { "200": { "description": "ok" } } },
                    "post": {
                        "requestBody": { "content": { "application/json": { "schema": { "type": "object" } } } },
                        "responses": { "201": { "description": "created" } }
                    }
                }
            }
        }),
        OpenApiSpecificationVersion::OpenApi30,
    );

    assert_eq!(stats.path_item_count, 1);
    assert_eq!(stats.operation_count, 2);
    assert_eq!(stats.response_count, 2);
    assert_eq!(stats.request_body_count, 1);
    assert_eq!(stats.schema_count, 1);
}

#[test]
fn counts_headers_links_and_callbacks() {
    let stats = stats_of(
        &json!({
            "openapi": "3.0.0",
            "paths": {
                "/pets": {
                    "get": {
                        "callbacks": { "onData": {} },
                        "responses": {
                            "200": {
                                "headers": { "X-Rate-Limit": { "schema": { "type": "integer" } } },
                                "links": { "next": {} }
                            }
                        }
                    }
                }
            }
        }),
        OpenApiSpecificationVersion::OpenApi30,
    );

    assert_eq!(stats.header_count, 1);
    assert_eq!(stats.link_count, 1);
    assert_eq!(stats.callback_count, 1);
    assert_eq!(stats.schema_count, 1);
}

#[test]
fn counts_nested_schemas_but_not_references() {
    let stats = stats_of(
        &json!({
            "openapi": "3.0.0",
            "paths": {},
            "components": {
                "schemas": {
                    "Pet": {
                        "type": "object",
                        "properties": {
                            "tags": { "type": "array", "items": { "type": "string" } },
                            "owner": { "$ref": "#/components/schemas/Owner" }
                        }
                    },
                    "Owner": { "type": "object" }
                }
            }
        }),
        OpenApiSpecificationVersion::OpenApi30,
    );

    assert_eq!(stats.schema_count, 4);
}

#[test]
fn counts_swagger2_body_and_form_parameters_as_request_bodies() {
    let stats = stats_of(
        &json!({
            "swagger": "2.0",
            "paths": {
                "/pet": {
                    "post": {
                        "parameters": [
                            { "name": "petId", "in": "path", "type": "integer" },
                            { "name": "body", "in": "body", "schema": { "$ref": "#/definitions/Pet" } }
                        ],
                        "responses": { "200": { "description": "ok" } }
                    }
                },
                "/pet/{petId}": {
                    "post": {
                        "parameters": [
                            { "name": "name", "in": "formData", "type": "string" },
                            { "name": "status", "in": "formData", "type": "string" }
                        ],
                        "responses": { "200": { "description": "ok" } }
                    }
                }
            },
            "definitions": { "Pet": { "type": "object" } }
        }),
        OpenApiSpecificationVersion::Swagger2,
    );

    assert_eq!(stats.parameter_count, 1);
    assert_eq!(stats.request_body_count, 2);
    assert_eq!(stats.schema_count, 1 + 1 + 3);
}

#[test]
fn read_results_expose_stats_of_the_merged_document() {
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/petstore.yaml");

    let result = read(path.to_str().unwrap()).unwrap();
    let stats = result.stats();

    assert_eq!(stats.path_item_count, 13);
    assert_eq!(stats.operation_count, 19);
    assert_eq!(
        stats,
        inspect_value(&result.document, OpenApiSpecificationVersion::OpenApi30)
    );
    assert!(stats.schema_count > 5);
}
