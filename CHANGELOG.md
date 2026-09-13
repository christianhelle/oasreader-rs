# Changelog

## [Unreleased]

### Added

- Read Swagger 2.0, OpenAPI 3.0 and OpenAPI 3.1 documents in JSON or YAML from local files or HTTP(S) URLs
- Merge components referenced from other files and URLs into a single document
- Inline references to whole files and to locations inside other documents
- Report unresolved, circular and conflicting references as diagnostics
- Detect external references, content format and specification version
- Serialize documents back to JSON or YAML
- Count paths, operations and components
- Typed OpenAPI 3.0 and 3.1 models behind the `typed` feature
- Pluggable resource loading, with HTTP downloads behind the default `remote` feature
