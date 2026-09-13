use std::{fs, path::PathBuf};

use oasreader::{OpenApiSpecificationVersion, load_raw_document, read};

fn corpus() -> Vec<(PathBuf, OpenApiSpecificationVersion)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    [
        ("v2.0", OpenApiSpecificationVersion::Swagger2),
        ("v3.0", OpenApiSpecificationVersion::OpenApi30),
        ("v3.1", OpenApiSpecificationVersion::OpenApi31),
    ]
    .into_iter()
    .flat_map(|(directory, version)| {
        fs::read_dir(root.join(directory))
            .unwrap()
            .map(move |entry| (entry.unwrap().path(), version))
    })
    .collect()
}

#[test]
fn reads_every_single_file_fixture_unchanged() {
    let corpus = corpus();
    assert!(corpus.len() > 30);

    for (path, version) in corpus {
        let input = path.to_str().unwrap();
        let raw = load_raw_document(input).unwrap();

        let result = read(input).unwrap_or_else(|error| panic!("{input}: {error}"));

        assert_eq!(result.specification_version, version, "{input}");
        assert!(!result.contained_external_references, "{input}");
        assert!(result.diagnostics.is_empty(), "{input}");
        assert_eq!(&result.document, raw.value(), "{input}");
    }
}
