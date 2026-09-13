mod support;

use oasreader::{DefaultLoader, HttpOptions, OpenApiSource, ResourceLoader, classify_source};

fn url_source(url: &str) -> OpenApiSource {
    classify_source(url).unwrap()
}

#[cfg(feature = "remote")]
mod remote {
    use super::*;
    use crate::support::TestServer;
    use oasreader::{FetchError, load_raw_document};

    #[test]
    fn downloads_documents_over_http() {
        let server = TestServer::start(&[("/openapi.json", 200, r#"{"openapi":"3.0.0"}"#)]);

        let content = DefaultLoader::default()
            .load(&url_source(&server.url("/openapi.json")))
            .unwrap();

        assert_eq!(content, r#"{"openapi":"3.0.0"}"#);
    }

    #[test]
    fn load_raw_document_accepts_urls() {
        let server = TestServer::start(&[("/specs/petstore", 200, "openapi: 3.1.0\n")]);

        let raw = load_raw_document(&server.url("/specs/petstore")).unwrap();

        assert_eq!(raw.value()["openapi"], "3.1.0");
        assert_eq!(raw.source(), &url_source(&server.url("/specs/petstore")));
    }

    #[test]
    fn reports_unsuccessful_status_codes() {
        let server = TestServer::start(&[]);
        let url = server.url("/missing.json");

        let error = DefaultLoader::default()
            .load(&url_source(&url))
            .unwrap_err();

        assert!(matches!(&error, FetchError::HttpRequest { reason, .. } if reason.contains("404")));
        assert!(
            error
                .to_string()
                .starts_with(&format!("could not download the file at {url}: "))
        );
    }

    #[test]
    fn rejects_bodies_larger_than_the_configured_limit() {
        let body = "x".repeat(2048);
        let server = TestServer::start(&[("/huge.yaml", 200, &body)]);
        let loader = DefaultLoader::new(HttpOptions {
            max_download_bytes: 1024,
            ..HttpOptions::default()
        });

        let error = loader
            .load(&url_source(&server.url("/huge.yaml")))
            .unwrap_err();

        assert!(matches!(error, FetchError::HttpRequest { .. }));
    }
}

#[cfg(not(feature = "remote"))]
#[test]
fn urls_fail_when_remote_loading_is_disabled() {
    let error = DefaultLoader::default()
        .load(&url_source("https://example.com/openapi.json"))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "could not download the file at https://example.com/openapi.json: remote loading is not enabled (build with the 'remote' feature)"
    );
}

#[test]
fn http_options_default_to_safe_limits() {
    let options = HttpOptions::default();

    assert_eq!(options.timeout, std::time::Duration::from_secs(60));
    assert_eq!(options.max_download_bytes, 100 * 1024 * 1024);
    assert!(!options.accept_invalid_certificates);
}
