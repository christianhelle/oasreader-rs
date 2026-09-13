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
