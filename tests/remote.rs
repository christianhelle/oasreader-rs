#![cfg(feature = "remote")]

mod support;

use oasreader::{Diagnostic, contains_external_references, read};
use support::TestServer;

const COMPONENTS: &str = r##"
openapi: 3.0.3
components:
  schemas:
    Pet:
      type: object
      properties:
        category:
          $ref: '#/components/schemas/Category'
        tags:
          type: array
          items:
            $ref: '../shared/tags.yaml#/components/schemas/Tag'
    Category:
      type: object
"##;

const TAGS: &str = "components:\n  schemas:\n    Tag:\n      type: object\n";

fn petstore(reference: &str) -> String {
    format!(
        r#"
openapi: 3.0.3
info:
  title: Remote petstore
  version: 1.0.0
paths:
  /pet:
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: '{reference}#/components/schemas/Pet'
      responses:
        '200':
          description: ok
"#
    )
}

fn schema_names(document: &serde_json::Value) -> Vec<&str> {
    document["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn merges_references_relative_to_a_remote_document() {
    let main = petstore("components.yaml");
    let server = TestServer::start(&[
        ("/specs/v1/petstore.yaml", 200, &main),
        ("/specs/v1/components.yaml", 200, COMPONENTS),
        ("/specs/shared/tags.yaml", 200, TAGS),
    ]);

    let result = read(&server.url("/specs/v1/petstore.yaml")).unwrap();

    assert!(result.contained_external_references);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(!contains_external_references(&result.document));
    assert_eq!(schema_names(&result.document), ["Category", "Pet", "Tag"]);
    assert_eq!(
        server.requests(),
        [
            "/specs/v1/petstore.yaml",
            "/specs/v1/components.yaml",
            "/specs/shared/tags.yaml"
        ]
    );
}

#[test]
fn merges_absolute_url_references_from_a_local_document() {
    let server = TestServer::start(&[
        ("/specs/v1/components.yaml", 200, COMPONENTS),
        ("/specs/shared/tags.yaml", 200, TAGS),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let main = directory.path().join("petstore.yaml");
    std::fs::write(&main, petstore(&server.url("/specs/v1/components.yaml"))).unwrap();

    let result = read(main.to_str().unwrap()).unwrap();

    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(schema_names(&result.document), ["Category", "Pet", "Tag"]);
}

#[test]
fn reports_remote_references_that_cannot_be_downloaded() {
    let main = petstore("missing.yaml");
    let server = TestServer::start(&[("/petstore.yaml", 200, &main)]);

    let result = read(&server.url("/petstore.yaml")).unwrap();

    assert!(matches!(
        result.diagnostics.as_slice(),
        [Diagnostic::UnresolvedReference { reason, .. }] if reason.contains("404")
    ));
}

#[test]
#[ignore = "requires network access to raw.githubusercontent.com"]
fn reads_the_published_remote_petstore_examples() {
    for url in [
        "https://raw.githubusercontent.com/christianhelle/oasreader/refs/heads/main/src/OasReader.Tests/Resources/remote-petstore.yaml",
        "https://raw.githubusercontent.com/christianhelle/oasreader/refs/heads/main/src/OasReader.Tests/Resources/relative-remote-petstore.yaml",
    ] {
        let result = read(url).unwrap();

        assert!(result.contained_external_references, "{url}");
        assert!(
            result.diagnostics.is_empty(),
            "{url}: {:?}",
            result.diagnostics
        );
        for name in ["Pet", "Category", "Tag"] {
            assert!(
                schema_names(&result.document).contains(&name),
                "{url}: {name}"
            );
        }
    }
}
