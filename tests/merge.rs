mod support;

use oasreader::{Diagnostic, MergeReport, OpenApiSource, merge_external_references};
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

#[test]
fn inlines_references_to_whole_files() {
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
                $ref: 'schemas/pet.yaml'
"#,
        ),
        (
            "/specs/schemas/pet.yaml",
            r#"
type: object
properties:
  category:
    $ref: 'category.yaml#/components/schemas/Category'
"#,
        ),
        (
            "/specs/schemas/category.yaml",
            "components:\n  schemas:\n    Category:\n      type: string\n",
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["paths"]["/pets"]["get"]["responses"]["200"]["content"]["application/json"]["schema"],
        json!({
            "type": "object",
            "properties": { "category": { "$ref": "#/components/schemas/Category" } }
        })
    );
    assert_eq!(
        document["components"]["schemas"]["Category"],
        json!({ "type": "string" })
    );
}

#[test]
fn inlines_references_to_locations_inside_other_documents() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
paths:
  /pets/{id}:
    $ref: 'legacy.yaml#/paths/~1pets~1%7Bid%7D'
  /owners:
    get:
      responses:
        '200':
          $ref: 'legacy.yaml#/components/responses/Owners/content/application~1json'
"#,
        ),
        (
            "/specs/legacy.yaml",
            r##"
paths:
  /pets/{id}:
    get:
      responses:
        '200':
          $ref: '#/components/responses/Pet'
components:
  responses:
    Pet:
      description: A pet
    Owners:
      content:
        application/json:
          schema:
            type: array
"##,
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["paths"]["/pets/{id}"],
        json!({ "get": { "responses": { "200": { "$ref": "#/components/responses/Pet" } } } })
    );
    assert_eq!(
        document["components"]["responses"]["Pet"],
        json!({ "description": "A pet" })
    );
    assert_eq!(
        document["paths"]["/owners"]["get"]["responses"]["200"],
        json!({ "schema": { "type": "array" } })
    );
}

#[test]
fn keeps_sibling_keywords_of_inlined_references() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.1.0
components:
  schemas:
    Pet:
      properties:
        name:
          $ref: 'common.yaml#/definitions/Name/properties/value'
          description: The name of the pet
"#,
        ),
        (
            "/specs/common.yaml",
            r#"
definitions:
  Name:
    properties:
      value:
        type: string
        description: A name
"#,
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["components"]["schemas"]["Pet"]["properties"]["name"],
        json!({ "type": "string", "description": "The name of the pet" })
    );
}

/// Uses `/` as the path separator, since joined paths use the platform's separator.
fn unix_paths(message: impl ToString) -> String {
    message.to_string().replace('\\', "/")
}

fn main_source() -> OpenApiSource {
    OpenApiSource::Path("/specs/main.yaml".into())
}

#[test]
fn reports_references_to_missing_files_and_leaves_them_unchanged() {
    let files = MemoryFiles::new(&[(
        "/specs/main.yaml",
        r#"
openapi: 3.0.3
components:
  schemas:
    Pet:
      $ref: 'missing.yaml#/components/schemas/Pet'
"#,
    )]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert_eq!(
        document["components"]["schemas"]["Pet"],
        json!({ "$ref": "missing.yaml#/components/schemas/Pet" })
    );
    let [
        Diagnostic::UnresolvedReference {
            reference,
            referenced_from,
            reason,
        },
    ] = report.diagnostics.as_slice()
    else {
        panic!("unexpected diagnostics {:?}", report.diagnostics);
    };
    assert_eq!(reference, "missing.yaml#/components/schemas/Pet");
    assert_eq!(referenced_from, &main_source());
    assert_eq!(
        unix_paths(reason),
        "could not open the file at /specs/missing.yaml: file not found"
    );
    assert_eq!(
        unix_paths(&report.diagnostics[0]),
        "could not resolve 'missing.yaml#/components/schemas/Pet' in /specs/main.yaml: could not open the file at /specs/missing.yaml: file not found"
    );
}

#[test]
fn reports_references_to_missing_locations_and_unsupported_schemes() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
components:
  schemas:
    Pet:
      $ref: 'components.yaml#/components/schemas/Pet'
    Owner:
      $ref: 'components.yaml#/definitions/Owner/properties/name'
    Remote:
      $ref: 'ftp://example.com/components.yaml#/components/schemas/Remote'
"#,
        ),
        ("/specs/components.yaml", "components: {}\n"),
    ]);

    let (_, report) = merge(&files, "/specs/main.yaml");

    let reasons: Vec<_> = report
        .diagnostics
        .iter()
        .map(|diagnostic| match diagnostic {
            Diagnostic::UnresolvedReference { reason, .. } => unix_paths(reason),
            other => panic!("unexpected diagnostic {other:?}"),
        })
        .collect();
    assert_eq!(
        reasons,
        [
            "/specs/components.yaml does not contain the referenced location",
            "/specs/components.yaml does not contain the referenced location",
            "the URL scheme 'ftp' is not supported",
        ]
    );
}

#[test]
fn reports_circular_inlined_references() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
components:
  schemas:
    Tree:
      $ref: 'tree.yaml'
"#,
        ),
        (
            "/specs/tree.yaml",
            r#"
type: object
properties:
  children:
    type: array
    items:
      $ref: 'tree.yaml'
"#,
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert_eq!(
        document["components"]["schemas"]["Tree"]["properties"]["children"]["items"],
        json!({ "$ref": "tree.yaml" })
    );
    let tree = OpenApiSource::Path("/specs/tree.yaml".into());
    assert_eq!(
        report.diagnostics,
        [Diagnostic::CircularReference {
            reference: "tree.yaml".to_string(),
            referenced_from: tree,
        }]
    );
    assert_eq!(
        unix_paths(&report.diagnostics[0]),
        "'tree.yaml' in /specs/tree.yaml refers back to itself and was left unchanged"
    );
}

#[test]
fn keeps_the_first_component_when_names_conflict() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
paths:
  /pets:
    $ref: 'paths.yaml#/components/pathItems/Pets'
components:
  schemas:
    Pet:
      type: object
"#,
        ),
        (
            "/specs/paths.yaml",
            r#"
components:
  pathItems:
    Pets:
      get:
        responses:
          '200':
            description: ok
            content:
              application/json:
                schema:
                  $ref: 'v2.yaml#/components/schemas/Pet'
          default:
            description: same
            content:
              application/json:
                schema:
                  $ref: 'v1.yaml#/components/schemas/Pet'
"#,
        ),
        (
            "/specs/v1.yaml",
            "components:\n  schemas:\n    Pet:\n      type: object\n",
        ),
        (
            "/specs/v2.yaml",
            "components:\n  schemas:\n    Pet:\n      type: string\n",
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert_eq!(
        document["components"]["schemas"]["Pet"],
        json!({ "type": "object" })
    );
    assert_eq!(
        document["components"]["pathItems"]["Pets"]["get"]["responses"]["200"]["content"]["application/json"]
            ["schema"],
        json!({ "$ref": "#/components/schemas/Pet" })
    );
    let paths = OpenApiSource::Path("/specs/paths.yaml".into());
    assert_eq!(
        report.diagnostics,
        [Diagnostic::NameConflict {
            name: "Pet".to_string(),
            reference: "v2.yaml#/components/schemas/Pet".to_string(),
            referenced_from: paths,
        }]
    );
    assert_eq!(
        unix_paths(&report.diagnostics[0]),
        "'v2.yaml#/components/schemas/Pet' in /specs/paths.yaml differs from the existing component 'Pet', which was kept"
    );
}

#[test]
fn imports_undefined_local_components_from_loaded_documents() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r##"
openapi: 3.0.3
paths:
  /pets:
    get:
      parameters:
        - $ref: '#/components/parameters/Limit'
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                $ref: 'components.yaml#/components/schemas/Pet'
        default:
          $ref: '#/components/responses/Undefined'
"##,
        ),
        (
            "/specs/components.yaml",
            r##"
components:
  schemas:
    Pet:
      type: object
    Count:
      type: integer
  parameters:
    Limit:
      name: limit
      in: query
      schema:
        $ref: '#/components/schemas/Count'
"##,
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["components"]["parameters"]["Limit"]["schema"],
        json!({ "$ref": "#/components/schemas/Count" })
    );
    assert_eq!(
        document["components"]["schemas"]["Count"],
        json!({ "type": "integer" })
    );
    assert!(document["components"].get("responses").is_none());
}

#[test]
fn sorts_schemas_by_name_after_merging() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r#"
openapi: 3.0.3
paths:
  /a:
    $ref: 'more.yaml#/components/pathItems/A'
components:
  schemas:
    Zebra:
      $ref: 'more.yaml#/components/schemas/Mango'
  parameters:
    Zulu:
      name: zulu
      in: query
    Alpha:
      name: alpha
      in: query
"#,
        ),
        (
            "/specs/more.yaml",
            r##"
components:
  pathItems:
    A:
      get:
        responses:
          '200':
            $ref: '#/components/responses/Ant'
  responses:
    Ant:
      description: ok
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/Ant'
  schemas:
    Mango:
      type: object
    Ant:
      type: object
"##,
        ),
    ]);

    let (document, _) = merge(&files, "/specs/main.yaml");

    let schemas: Vec<_> = document["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    let parameters: Vec<_> = document["components"]["parameters"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(schemas, ["Ant", "Mango", "Zebra"]);
    assert_eq!(parameters, ["Zulu", "Alpha"]);
}

#[test]
fn merges_into_swagger_two_definitions() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.json",
            r#"{
  "swagger": "2.0",
  "paths": {
    "/pets": {
      "get": {
        "parameters": [{ "$ref": "shared.json#/parameters/Limit" }],
        "responses": {
          "200": { "description": "ok", "schema": { "$ref": "shared.json#/definitions/Pet" } },
          "default": { "description": "error", "schema": { "$ref": "errors.yaml#/components/schemas/Error" } }
        }
      }
    }
  }
}"#,
        ),
        (
            "/specs/shared.json",
            r##"{
  "definitions": {
    "Pet": { "properties": { "owner": { "$ref": "#/definitions/Owner" } } },
    "Owner": { "type": "object" }
  },
  "parameters": { "Limit": { "name": "limit", "in": "query", "type": "integer" } }
}"##,
        ),
        (
            "/specs/errors.yaml",
            "components:\n  schemas:\n    Error:\n      type: object\n",
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.json");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let get = &document["paths"]["/pets"]["get"];
    assert_eq!(
        get["parameters"][0],
        json!({ "$ref": "#/parameters/Limit" })
    );
    assert_eq!(
        get["responses"]["200"]["schema"],
        json!({ "$ref": "#/definitions/Pet" })
    );
    assert_eq!(
        get["responses"]["default"]["schema"],
        json!({ "$ref": "#/definitions/Error" })
    );
    let definitions: Vec<_> = document["definitions"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(definitions, ["Error", "Owner", "Pet"]);
    assert_eq!(document["parameters"]["Limit"]["name"], "limit");
    assert!(document.get("components").is_none());
}

#[test]
fn replaces_components_that_alias_an_external_component_of_the_same_name() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            r##"
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
                $ref: '#/components/schemas/Pet'
components:
  schemas:
    Pet:
      $ref: 'schemas.yaml#/components/schemas/Pet'
    Animal:
      $ref: 'schemas.yaml#/components/schemas/Pet'
"##,
        ),
        (
            "/specs/schemas.yaml",
            "components:\n  schemas:\n    Pet:\n      type: object\n",
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["components"]["schemas"],
        json!({
            "Animal": { "$ref": "#/components/schemas/Pet" },
            "Pet": { "type": "object" }
        })
    );
}

#[test]
fn follows_same_name_aliases_through_intermediate_files() {
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
                $ref: 'index.yaml#/components/schemas/Pet'
"#,
        ),
        (
            "/specs/index.yaml",
            "components:\n  schemas:\n    Pet:\n      $ref: 'schemas/pet.yaml#/components/schemas/Pet'\n",
        ),
        (
            "/specs/schemas/pet.yaml",
            "components:\n  schemas:\n    Pet:\n      type: object\n",
        ),
    ]);

    let (document, report) = merge(&files, "/specs/main.yaml");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(
        document["components"]["schemas"],
        json!({ "Pet": { "type": "object" } })
    );
}

#[test]
fn reports_same_name_aliases_that_loop_between_files() {
    let files = MemoryFiles::new(&[
        (
            "/specs/main.yaml",
            "openapi: 3.0.3\npaths:\n  /pets:\n    $ref: 'a.yaml#/components/pathItems/Pets'\n",
        ),
        (
            "/specs/a.yaml",
            "components:\n  pathItems:\n    Pets:\n      $ref: 'b.yaml#/components/pathItems/Pets'\n",
        ),
        (
            "/specs/b.yaml",
            "components:\n  pathItems:\n    Pets:\n      $ref: 'a.yaml#/components/pathItems/Pets'\n",
        ),
    ]);

    let (_, report) = merge(&files, "/specs/main.yaml");

    assert!(matches!(
        report.diagnostics.as_slice(),
        [Diagnostic::CircularReference { .. }]
    ));
}
