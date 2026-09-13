# Multi Document Reader for OpenAPI in Rust

An OpenAPI reader that merges external references into a single document. This is the Rust counterpart of the .NET [oasreader](https://github.com/christianhelle/oasreader), and the shared OpenAPI library for Rust tools like [httpgenerator](https://github.com/christianhelle/httpgenerator) and [curlgenerator](https://github.com/christianhelle/curlgenerator).

- Reads Swagger 2.0, OpenAPI 3.0 and OpenAPI 3.1 documents
- JSON or YAML, from local files or HTTP(S) URLs
- Merges components referenced from other files, following references across files and directories
- Detects the content format and specification version
- Counts paths, operations and components
- Optional typed models via [`openapiv3`](https://crates.io/crates/openapiv3) and [`openapiv3_1`](https://crates.io/crates/openapiv3_1)

## Installation

```toml
[dependencies]
oasreader = "0.1"
```

| Feature | Default | Description |
|---|---|---|
| `remote` | yes | Download documents and references over HTTP(S) using [`ureq`](https://crates.io/crates/ureq) |
| `typed` | no | Parse merged documents into the `openapiv3` (3.0) and `openapiv3_1` (3.1) models |

## Usage

`oasreader::read` loads an OpenAPI document from a local file or URL. If the document references other files, their components are merged into the document automatically.

```rust
let result = oasreader::read("petstore.yaml")?;

println!("{} ({})", result.source, result.specification_version);
println!("merged external references: {}", result.contained_external_references);
for diagnostic in &result.diagnostics {
    eprintln!("warning: {diagnostic}");
}

let document: &serde_json::Value = &result.document;
let yaml = oasreader::serialize_document(document, oasreader::OpenApiContentFormat::Yaml)?;
```

**Local files**: references are resolved relative to the file that contains them, so `components.yaml`, `schemas/pet.yaml` and `../shared/types.yaml` all work.

**Remote files**: references can be absolute URLs, like `https://example.com/components.yaml#/components/schemas/Pet`, or relative URLs, like `components.yaml#/components/schemas/Pet`, which are resolved against the URL of the file that contains them.

### Example with local external references

In the example below, the OpenAPI specification is split into two documents. **`petstore.yaml`** contains the **`paths`** and **`petstore.components.yaml`** contains the **`components/schemas`**.

**`petstore.yaml`**

```yaml
openapi: 3.0.3
paths:
  /pet:
    post:
      tags:
      - pet
      summary: Add a new pet to the store
      description: Add a new pet to the store
      operationId: addPet
      requestBody:
        description: Create a new pet in the store
        content:
          application/json:
            schema:
              $ref: 'petstore.components.yaml#/components/schemas/Pet'
        required: true
      responses:
        "200":
          description: Successful operation
          content:
            application/json:
              schema:
                $ref: 'petstore.components.yaml#/components/schemas/Pet'
```

**`petstore.components.yaml`**

```yaml
openapi: 3.0.3
components:
  schemas:
    Pet:
      required:
      - name
      - photoUrls
      type: object
      properties:
        id:
          type: integer
          format: int64
          example: 10
        name:
          type: string
          example: doggie
        category:
          $ref: '#/components/schemas/Category'
        photoUrls:
          type: array
          items:
            type: string
        status:
          type: string
          description: pet status in the store
          enum:
          - available
          - pending
          - sold
    Category:
      type: object
      properties:
        id:
          type: integer
          format: int64
          example: 1
        name:
          type: string
          example: Dogs
```

Reading `petstore.yaml` returns a single document where both references point to `#/components/schemas/Pet`. `Pet` and the `Category` schema it references are copied into `components/schemas`, sorted by name.

### Example with remote external references

References can use absolute URLs:

```yaml
openapi: 3.0.3
paths:
  /pet:
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: 'https://example.com/openapi/components.yaml#/components/schemas/Pet'
```

Or relative URLs, when the main file is also remote:

```yaml
openapi: 3.0.3
paths:
  /pet:
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: 'components.yaml#/components/schemas/Pet'  # Resolved relative to the main file's URL
```

### Custom loading

`OpenApiReader` takes any `ResourceLoader`, including a closure. Use it to serve documents from memory, add authentication headers, restrict which locations can be read, or change HTTP settings:

```rust
use std::time::Duration;
use oasreader::{DefaultLoader, HttpOptions, OpenApiReader};

let reader = OpenApiReader::new().with_loader(DefaultLoader::new(HttpOptions {
    timeout: Duration::from_secs(10),
    accept_invalid_certificates: true, // for development servers with self-signed certificates
    ..HttpOptions::default()
}));

let result = reader.read("https://localhost:5001/swagger/v1/swagger.json")?;
```

### Working with document trees

The merge works on any `serde_json::Value`:

```rust
use oasreader::{DefaultLoader, classify_source, contains_external_references, merge_external_references};

let source = classify_source("specs/petstore.yaml")?;
let mut document = oasreader::load_raw_document("specs/petstore.yaml")?.into_value();

if contains_external_references(&document) {
    let report = merge_external_references(&mut document, &source, &DefaultLoader::default());
    assert!(report.diagnostics.is_empty());
}
```

`merge_external_references_as_string` merges and serializes in one step. It writes YAML for `.yaml`/`.yml` sources and JSON otherwise.

### Stats and typed models

```rust
let result = oasreader::read("petstore.yaml")?;
let stats = result.stats();
println!("{} operations across {} paths", stats.operation_count, stats.path_item_count);

// with the `typed` feature
match result.typed(oasreader::TypedParseOptions::default())? {
    oasreader::TypedOpenApiDocument::OpenApi30(openapi) => println!("{}", openapi.info.title),
    oasreader::TypedOpenApiDocument::OpenApi31(openapi) => println!("{}", openapi.info.title),
    _ => println!("no typed model for this document; use result.document instead"),
}
```

Try it on your own specification with the bundled example:

```sh
cargo run --example merge -- path/or/url/to/openapi.yaml --json
```

## How references are merged

- A reference to a whole named component, such as `#/components/schemas/Pet` or `#/definitions/Pet` in Swagger 2.0, is copied into the document's own components section. The reference is rewritten to point at the copy. Components referenced from inside a copied component are merged too.
- A reference to anything else, such as a whole file (`pet.yaml`) or a location inside a document (`common.yaml#/paths/~1pets`), is replaced by the referenced content. Keywords next to such a `$ref` are kept.
- A component that is only a reference to a component of the same name in another file (`Pet: {$ref: 'pet.yaml#/components/schemas/Pet'}`) is replaced by that component.
- Local references to components that the main document does not define are looked up in the other loaded documents.
- Problems do not stop the merge. References that cannot be loaded, circular inlined references, and components whose names clash with a different component are reported in `diagnostics`. Clashing references point at the component that was merged first.

## Differences from the .NET oasreader

- References are resolved relative to the file that contains them, instead of by matching file names under the main file's directory. This means `../` works, and so do chains of references across directories.
- External references are detected anywhere in the document, not only under `paths`.
- Merged documents keep the specification version they declare. The .NET version writes OpenAPI 3.0.
- Problems are reported as diagnostics instead of being ignored.
- Validation rules are not part of this library.

#

For tips and tricks on software development, check out [my blog](https://christianhelle.com)

If you find this useful and feel a bit generous then feel free to [buy me a coffee ☕](https://www.buymeacoffee.com/christianhelle)
