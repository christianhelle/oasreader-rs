mod support;

use std::path::PathBuf;

use oasreader::{
    OpenApiContentFormat, OpenApiReader, OpenApiSource, OpenApiSpecificationVersion, ReadError,
    ReadResult, contains_external_references, read,
};
use support::MemoryFiles;

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn schema_names(result: &ReadResult) -> Vec<String> {
    result.document["components"]["schemas"]
        .as_object()
        .map(|schemas| schemas.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn reads_petstore_split_across_two_files() {
    let result = read(&fixture("petstore.yaml")).unwrap();

    assert!(result.contained_external_references);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(!contains_external_references(&result.document));
    assert_eq!(result.format, OpenApiContentFormat::Yaml);
    assert_eq!(
        result.specification_version,
        OpenApiSpecificationVersion::OpenApi30
    );
    assert_eq!(
        result.source,
        OpenApiSource::Path(PathBuf::from(fixture("petstore.yaml")))
    );
    for name in ["Order", "Pet", "Tag", "Category", "ApiResponse"] {
        assert!(schema_names(&result).contains(&name.to_string()), "{name}");
    }
}

#[test]
fn reads_bot_split_across_two_files() {
    let result = read(&fixture("bot.yaml")).unwrap();

    assert!(result.contained_external_references);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(!contains_external_references(&result.document));
    for name in [
        "BaseGuildID",
        "BaseChannelID",
        "BaseChannelCategoryID",
        "BaseUserID",
        "BaseRoleID",
        "BaseMessageID",
        "BaseSequenceInChannel",
        "BaseScheduleID",
        "Guild",
        "User",
        "Channel",
        "Member",
        "Role",
        "ChannelPermissions",
        "Message",
    ] {
        assert!(schema_names(&result).contains(&name.to_string()), "{name}");
    }
}

#[test]
fn reads_single_file_documents_unchanged() {
    let path = fixture("v3.0/petstore.json");
    let raw = oasreader::load_raw_document(&path).unwrap();

    let result = read(&path).unwrap();

    assert!(!result.contained_external_references);
    assert_eq!(result.format, OpenApiContentFormat::Json);
    assert_eq!(&result.document, raw.value());
}

#[test]
fn reports_missing_files() {
    let error = read("./does-not-exist/missing.yaml").unwrap_err();

    assert!(matches!(error, ReadError::Load(_)));
    assert!(
        error
            .to_string()
            .starts_with("could not open the file at ./does-not-exist/missing.yaml: ")
    );
}

#[test]
fn reports_unsupported_specification_versions() {
    let files = MemoryFiles::new(&[("/specs/main.yaml", "openapi: 4.0.0\npaths: {}\n")]);

    let error = OpenApiReader::new()
        .with_loader(files)
        .read("/specs/main.yaml")
        .unwrap_err();

    assert!(matches!(error, ReadError::VersionDetection(_)));
    assert_eq!(
        error.to_string(),
        "unsupported OpenAPI version '4.0.0' in field 'openapi'"
    );
}

#[test]
fn reads_through_a_custom_loader() {
    let files = MemoryFiles::new(&[
        (
            "/memory/main.yaml",
            "openapi: 3.1.0\ncomponents:\n  schemas:\n    Pet:\n      $ref: 'pet.yaml#/components/schemas/Pet'\n",
        ),
        (
            "/memory/pet.yaml",
            "components:\n  schemas:\n    Pet:\n      type: object\n",
        ),
    ]);

    let result = OpenApiReader::new()
        .with_loader(files)
        .read_source(OpenApiSource::Path("/memory/main.yaml".into()))
        .unwrap();

    assert_eq!(
        result.specification_version,
        OpenApiSpecificationVersion::OpenApi31
    );
    assert_eq!(
        result.document["components"]["schemas"]["Pet"]["type"],
        "object"
    );
}
