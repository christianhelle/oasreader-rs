//! Fetching the text content of OpenAPI documents from disk or over HTTP.

use std::{fmt, fs, path::PathBuf, time::Duration};

use url::Url;

use crate::OpenApiSource;

/// Loads the text content of an OpenAPI document or an externally referenced file.
///
/// [`DefaultLoader`] reads local files and downloads HTTP(S) URLs. Implement this trait to serve
/// documents from memory, add authentication headers, or restrict which locations may be read.
/// Closures with the matching signature implement it too.
pub trait ResourceLoader {
    /// Returns the text content at `source`.
    fn load(&self, source: &OpenApiSource) -> Result<String, FetchError>;
}

impl<F> ResourceLoader for F
where
    F: Fn(&OpenApiSource) -> Result<String, FetchError>,
{
    fn load(&self, source: &OpenApiSource) -> Result<String, FetchError> {
        self(source)
    }
}

/// Errors raised while fetching document content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// Reading a local file failed.
    FileRead {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying failure description.
        reason: String,
    },
    /// Downloading a remote document failed.
    HttpRequest {
        /// The URL that could not be downloaded.
        url: Url,
        /// The underlying failure description.
        reason: String,
    },
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileRead { path, reason } => {
                write!(f, "could not open the file at {}: {reason}", path.display())
            }
            Self::HttpRequest { url, reason } => {
                write!(f, "could not download the file at {url}: {reason}")
            }
        }
    }
}

impl std::error::Error for FetchError {}

/// Settings for downloading remote documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpOptions {
    /// The maximum time a single download may take. Defaults to 60 seconds.
    pub timeout: Duration,
    /// The largest response body accepted, in bytes. Defaults to 100 MB, which bounds the memory
    /// a misbehaving server can force the reader to allocate.
    pub max_download_bytes: u64,
    /// Skips TLS certificate verification, for development servers with self-signed
    /// certificates. Defaults to `false`.
    pub accept_invalid_certificates: bool,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            max_download_bytes: 100 * 1024 * 1024,
            accept_invalid_certificates: false,
        }
    }
}

/// The built-in loader for local files and HTTP(S) URLs.
///
/// Downloading requires the `remote` feature, which is enabled by default.
#[derive(Debug, Clone)]
pub struct DefaultLoader {
    #[cfg(feature = "remote")]
    agent: ureq::Agent,
    #[cfg_attr(not(feature = "remote"), allow(dead_code))]
    options: HttpOptions,
}

impl DefaultLoader {
    /// Creates a loader that downloads remote documents with the given options.
    pub fn new(options: HttpOptions) -> Self {
        Self {
            #[cfg(feature = "remote")]
            agent: ureq::Agent::new_with_config(
                ureq::Agent::config_builder()
                    .tls_config(
                        ureq::tls::TlsConfig::builder()
                            .disable_verification(options.accept_invalid_certificates)
                            .build(),
                    )
                    .timeout_global(Some(options.timeout))
                    .build(),
            ),
            options,
        }
    }

    #[cfg(feature = "remote")]
    fn download(&self, url: &Url) -> Result<String, FetchError> {
        let http_error = |error: ureq::Error| FetchError::HttpRequest {
            url: url.clone(),
            reason: error.to_string(),
        };

        let body = self
            .agent
            .get(url.as_str())
            .call()
            .map_err(http_error)?
            .body_mut()
            .with_config()
            .limit(self.options.max_download_bytes)
            .read_to_vec()
            .map_err(http_error)?;

        Ok(String::from_utf8_lossy(&body).into_owned())
    }

    #[cfg(not(feature = "remote"))]
    fn download(&self, url: &Url) -> Result<String, FetchError> {
        Err(FetchError::HttpRequest {
            url: url.clone(),
            reason: "remote loading is not enabled (build with the 'remote' feature)".to_string(),
        })
    }
}

impl Default for DefaultLoader {
    fn default() -> Self {
        Self::new(HttpOptions::default())
    }
}

impl ResourceLoader for DefaultLoader {
    fn load(&self, source: &OpenApiSource) -> Result<String, FetchError> {
        match source {
            OpenApiSource::Path(path) => {
                fs::read_to_string(path).map_err(|error| FetchError::FileRead {
                    path: path.clone(),
                    reason: error.to_string(),
                })
            }
            OpenApiSource::Url(url) => self.download(url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_loader_reads_local_files() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("petstore.yaml");
        fs::write(&file, "openapi: 3.0.0\n").unwrap();

        let content = DefaultLoader::default()
            .load(&OpenApiSource::Path(file))
            .unwrap();

        assert_eq!(content, "openapi: 3.0.0\n");
    }

    #[test]
    fn default_loader_reports_missing_files() {
        let missing = PathBuf::from("./does-not-exist.json");

        let error = DefaultLoader::default()
            .load(&OpenApiSource::Path(missing.clone()))
            .unwrap_err();

        assert!(matches!(&error, FetchError::FileRead { path, .. } if path == &missing));
        assert!(
            error
                .to_string()
                .starts_with("could not open the file at ./does-not-exist.json: ")
        );
    }

    #[test]
    fn closures_are_resource_loaders() {
        let loader = |source: &OpenApiSource| Ok(format!("loaded {source}"));

        let content = loader
            .load(&OpenApiSource::Path(PathBuf::from("a.yaml")))
            .unwrap();

        assert_eq!(content, "loaded a.yaml");
    }

    #[test]
    fn http_errors_describe_the_url() {
        let error = FetchError::HttpRequest {
            url: Url::parse("https://example.com/openapi.json").unwrap(),
            reason: "connection refused".to_string(),
        };

        assert_eq!(
            error.to_string(),
            "could not download the file at https://example.com/openapi.json: connection refused"
        );
    }
}
