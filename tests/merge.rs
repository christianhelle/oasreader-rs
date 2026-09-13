mod support;

use oasreader::{MergeReport, OpenApiSource, merge_external_references};
use serde_json::{Value, json};
use support::MemoryFiles;

fn merge(files: &MemoryFiles, main: &str) -> (Value, MergeReport) {
    let mut document = files.document(main);
    let report = merge_external_references(&mut document, &OpenApiSource::Path(main.into()), files);
    (document, report)
}

#[test]
fn imports_referenced_components_from_another_file() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
paths:
  /pets:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                $ref: 'components.yaml#/components/schemas/Pet'
"#,
        ),
        (
            "/specs/components.yaml",
            r#"
components:
  schemas:
    Pet:
      type: object
      properties:
        name:
          type: string
"#,
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.contained_external_references);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["paths"]["/pets"]["get"]["responses"]["200"]["content"]["application/json"]["schema"],
        json!({ "$ref": "#/components/schemas/Pet" })
    );
    assert_eq!(
        document["components"]["schemas"]["Pet"],
        json!({ "type": "object", "properties": { "name": { "type": "string" } } })
    );
}

#[test]
fn leaves_documents_without_external_references_untouched() {
    let files = MemoryFiles::new(&[(
        "/specs/main.json",
        r##"{
  "openapi": "3.0.3",
  "paths": { "/pets": { "$ref": "#/components/pathItems/Pets" } },
  "components": { "schemas": { "Zebra": {}, "Ant": {} } }
}"##,
    )]);

    let (document, report) = merge(&files, "/specs/main.json");

    assert!(!report.contained_external_references);
    assert!(report.diagnostics.is_empty());
    assert_eq!(document, files.document("/specs/main.json"));
    assert!(files.loads().is_empty());
}

#[test]
fn imports_components_referenced_locally_inside_an_imported_component() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
paths:
  /pets:
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: 'components.yaml#/components/schemas/Pet'
"#,
        ),
        (
            "/specs/components.yaml",
            r##"
components:
  schemas:
    Pet:
      properties:
        category:
          $ref: '#/components/schemas/Category'
        tags:
          type: array
          items:
            $ref: '#/components/schemas/Tag'
    Category:
      type: object
    Tag:
      type: object
    Unused:
      type: object
"##,
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let schemas = document["components"]["schemas"].as_object().unwrap();
    assert!(schemas.contains_key("Category"));
    assert!(schemas.contains_key("Tag"));
    assert!(!schemas.contains_key("Unused"));
    assert_eq!(
        schemas["Pet"]["properties"]["tags"]["items"],
        json!({ "$ref": "#/components/schemas/Tag" })
    );
}

#[test]
fn follows_chains_of_references_across_directories() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
paths:
  /pets/{id}:
    parameters:
      - $ref: './parameters/path.yaml#/components/parameters/PetId'
"#,
        ),
        (
            "/specs/parameters/path.yaml",
            r#"
components:
  parameters:
    PetId:
      name: id
      in: path
      required: true
      schema:
        $ref: '../common/types.yaml#/components/schemas/Id'
"#,
        ),
        (
            "/specs/common/types.yaml",
            r#"
components:
  schemas:
    Id:
      type: integer
      format: int64
"#,
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["paths"]["/pets/{id}"]["parameters"][0],
        json!({ "$ref": "#/components/parameters/PetId" })
    );
    assert_eq!(
        document["components"]["parameters"]["PetId"]["schema"],
        json!({ "$ref": "#/components/schemas/Id" })
    );
    assert_eq!(
        document["components"]["schemas"]["Id"],
        json!({ "type": "integer", "format": "int64" })
    );
}

#[test]
fn loads_each_referenced_file_once() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
paths:
  /a:
    $ref: 'shared.yaml#/components/pathItems/A'
  /b:
    $ref: './shared.yaml#/components/pathItems/B'
"#,
        ),
        (
            "/specs/shared.yaml",
            r#"
components:
  pathItems:
    A:
      get: { responses: { '200': { description: ok } } }
    B:
      get: { responses: { '200': { description: ok } } }
"#,
        ),
    ]);

    let (document, _) = merge(&files, "/specs/main.yaml");

    assert_eq!(files.loads().len(), 1);
    assert_eq!(
        document["paths"],
        json!({
            "/a": { "$ref": "#/components/pathItems/A" },
            "/b": { "$ref": "#/components/pathItems/B" }
        })
    );
}

#[test]
fn imports_components_that_reference_each_other_across_files() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
components:
  schemas:
    Root:
      $ref: 'nodes.yaml#/components/schemas/Node'
"#,
        ),
        (
            "/specs/nodes.yaml",
            r#"
components:
  schemas:
    Node:
      properties:
        leaf:
          $ref: 'leaves.yaml#/components/schemas/Leaf'
"#,
        ),
        (
            "/specs/leaves.yaml",
            r#"
components:
  schemas:
    Leaf:
      properties:
        parent:
          $ref: 'nodes.yaml#/components/schemas/Node'
"#,
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["components"]["schemas"]["Node"]["properties"]["leaf"],
        json!({ "$ref": "#/components/schemas/Leaf" })
    );
    assert_eq!(
        document["components"]["schemas"]["Leaf"]["properties"]["parent"],
        json!({ "$ref": "#/components/schemas/Node" })
    );
}
