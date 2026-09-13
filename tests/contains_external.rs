use std::{fs, path::PathBuf};

use oasreader::{OpenApiSource, contains_external_references, decode_raw_document};
use serde_json::Value;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let content = fs::read_to_string(&path).unwrap();
    decode_raw_document(OpenApiSource::Path(path), content)
        .unwrap()
        .into_value()
}

fn yaml(content: &str) -> Value {
    decode_raw_document(OpenApiSource::Path(PathBuf::from("inline.yaml")), content)
        .unwrap()
        .into_value()
}

#[test]
fn split_specifications_contain_external_references() {
    assert!(contains_external_references(&fixture("petstore.yaml")));
    assert!(contains_external_references(&fixture("bot.yaml")));
    assert!(contains_external_references(&fixture(
        "remote-petstore.yaml"
    )));
    assert!(contains_external_references(&fixture(
        "relative-remote-petstore.yaml"
    )));
}

#[test]
fn single_file_specifications_do_not_contain_external_references() {
    for version in ["v2.0", "v3.0", "v3.1"] {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(version);
        for entry in fs::read_dir(directory).unwrap() {
            let name = format!("{version}/{}", entry.unwrap().file_name().to_string_lossy());
            assert!(
                !contains_external_references(&fixture(&name)),
                "{name} should not contain external references"
            );
        }
    }
}

#[test]
fn documents_without_paths_do_not_contain_external_references() {
    assert!(!contains_external_references(&yaml("openapi: 3.0.1\n")));
}

#[test]
fn detects_path_item_parameter_schema_references() {
    let document = yaml(
        r#"
openapi: 3.0.1
paths:
  /pets:
    parameters:
      - name: PetId
        in: query
        schema:
          $ref: 'components.yaml#/components/schemas/Pet'
"#,
    );

    assert!(contains_external_references(&document));
}

#[test]
fn ignores_local_path_item_parameter_schema_references() {
    let document = yaml(
        r##"
openapi: 3.0.1
paths:
  /pets:
    parameters:
      - name: Limit
        in: query
        schema:
          $ref: '#/components/schemas/Limit'
components:
  schemas:
    Limit:
      type: integer
"##,
    );

    assert!(!contains_external_references(&document));
}

#[test]
fn detects_path_item_and_operation_parameter_references() {
    let path_item = yaml(
        r#"
openapi: 3.0.1
paths:
  /pets:
    parameters:
      - $ref: 'components.yaml#/components/parameters/PetId'
"#,
    );
    let operation = yaml(
        r#"
openapi: 3.0.1
paths:
  /pets:
    get:
      parameters:
        - $ref: 'components.yaml#/components/parameters/PetId'
"#,
    );

    assert!(contains_external_references(&path_item));
    assert!(contains_external_references(&operation));
}

#[test]
fn detects_parameter_content_schema_references() {
    let document = yaml(
        r#"
openapi: 3.0.1
paths:
  /pets:
    get:
      parameters:
        - name: filter
          in: query
          content:
            application/json:
              schema:
                $ref: 'components.yaml#/components/schemas/Filter'
"#,
    );

    assert!(contains_external_references(&document));
}

#[test]
fn detects_request_body_and_response_references() {
    let request_body = yaml(
        r#"
openapi: 3.0.1
paths:
  /pets:
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: 'components.yaml#/components/schemas/Pet'
"#,
    );
    let response_header = yaml(
        r#"
openapi: 3.0.1
paths:
  /pets:
    get:
      responses:
        '200':
          description: ok
          headers:
            X-Rate-Limit:
              schema:
                $ref: 'components.yaml#/components/schemas/RateLimit'
"#,
    );
    let second_media_type = yaml(
        r#"
openapi: 3.0.1
paths:
  /pets:
    get:
      responses:
        default:
          description: ok
          content:
            application/json:
              schema:
                type: object
            application/xml:
              schema:
                $ref: 'components.yaml#/components/schemas/Pet'
"#,
    );

    assert!(contains_external_references(&request_body));
    assert!(contains_external_references(&response_header));
    assert!(contains_external_references(&second_media_type));
}

#[test]
fn detects_references_outside_of_paths() {
    let document = yaml(
        r#"
openapi: 3.0.1
paths: {}
components:
  schemas:
    Pet:
      properties:
        owner:
          $ref: 'people.yaml#/components/schemas/Owner'
"#,
    );

    assert!(contains_external_references(&document));
}

#[test]
fn detects_references_in_schema_properties_named_like_keywords() {
    let document = yaml(
        r#"
openapi: 3.0.1
components:
  schemas:
    Setting:
      properties:
        default:
          $ref: 'common.yaml#/components/schemas/Value'
"#,
    );

    assert!(contains_external_references(&document));
}

#[test]
fn ignores_ref_like_literal_values() {
    let document = yaml(
        r#"
openapi: 3.1.0
components:
  schemas:
    Pet:
      type: object
      x-generator:
        $ref: 'ignored.yaml'
      example:
        $ref: 'example.yaml'
      default:
        $ref: 'default.yaml'
      enum:
        - $ref: 'enum.yaml'
      const:
        $ref: 'const.yaml'
      examples:
        - $ref: 'examples.yaml'
      properties:
        $ref:
          type: string
  examples:
    Pet:
      value:
        $ref: 'value.yaml'
"#,
    );

    assert!(!contains_external_references(&document));
}
