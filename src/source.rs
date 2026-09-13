//! Classification of OpenAPI inputs into local paths and HTTP(S) URLs.

use std::{fmt, path::PathBuf};

use url::Url;

/// A classified OpenAPI source location.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OpenApiSource {
    /// A local filesystem path.
    Path(PathBuf),
    /// A remote HTTP or HTTPS URL.
    Url(Url),
}

/// Errors raised while classifying an input as a path or URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceClassificationError {
    /// The input was empty or only whitespace.
    EmptyInput,
    /// The input used a URL scheme other than `http` or `https`.
    UnsupportedUrlScheme(String),
    /// The input looked like an HTTP(S) URL but could not be parsed.
    InvalidUrl {
        /// The rejected input.
        value: String,
        /// The parser failure description.
        reason: String,
    },
}

impl fmt::Display for SourceClassificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "the OpenAPI path is empty"),
            Self::UnsupportedUrlScheme(scheme) => {
                write!(f, "the URL scheme '{scheme}' is not supported")
            }
            Self::InvalidUrl { value, reason } => {
                write!(f, "could not parse '{value}' as a URL: {reason}")
            }
        }
    }
}

impl std::error::Error for SourceClassificationError {}

impl fmt::Display for OpenApiSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => write!(f, "{}", path.display()),
            Self::Url(url) => write!(f, "{url}"),
        }
    }
}

/// Classifies an input as either a local path or an HTTP(S) URL.
///
/// Inputs that contain `://` are only treated as URLs when the prefix is a valid URL scheme, which
/// keeps Windows paths such as `C:\specs\petstore.yaml` on the local-path branch.
///
/// # Examples
///
/// ```
/// use oasreader::{OpenApiSource, classify_source};
/// use std::path::PathBuf;
///
/// let path = classify_source("specs/petstore.yaml").unwrap();
/// assert_eq!(path, OpenApiSource::Path(PathBuf::from("specs/petstore.yaml")));
///
/// let url = classify_source("https://example.com/openapi.yaml").unwrap();
/// assert!(matches!(url, OpenApiSource::Url(_)));
/// ```
pub fn classify_source(input: &str) -> Result<OpenApiSource, SourceClassificationError> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err(SourceClassificationError::EmptyInput);
    }

    let Some(scheme) = candidate_url_scheme(trimmed) else {
        return Ok(OpenApiSource::Path(PathBuf::from(trimmed)));
    };

    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(SourceClassificationError::UnsupportedUrlScheme(scheme));
    }

    Url::parse(trimmed)
        .map(OpenApiSource::Url)
        .map_err(|error| SourceClassificationError::InvalidUrl {
            value: trimmed.to_string(),
            reason: error.to_string(),
        })
}

fn candidate_url_scheme(input: &str) -> Option<&str> {
    let (scheme, _) = input.split_once("://")?;

    let starts_with_letter = scheme.chars().next()?.is_ascii_alphabetic();
    let valid_characters = scheme
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'));

    (starts_with_letter && valid_characters).then_some(scheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_relative_file_paths_as_local_paths() {
        let source = classify_source("specs/petstore.yaml").unwrap();

        assert_eq!(
            source,
            OpenApiSource::Path(PathBuf::from("specs/petstore.yaml"))
        );
    }

    #[test]
    fn classifies_windows_absolute_paths_as_local_paths() {
        let source = classify_source("C:\\specs\\petstore.yaml").unwrap();

        assert_eq!(
            source,
            OpenApiSource::Path(PathBuf::from("C:\\specs\\petstore.yaml"))
        );
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let source = classify_source("  petstore.json \n").unwrap();

        assert_eq!(source, OpenApiSource::Path(PathBuf::from("petstore.json")));
    }

    #[test]
    fn classifies_http_and_https_urls() {
        let https = classify_source("https://example.com/specs/petstore.yaml?download=1").unwrap();
        let http = classify_source("HTTP://example.com/petstore.json").unwrap();

        assert!(
            matches!(https, OpenApiSource::Url(url) if url.as_str() == "https://example.com/specs/petstore.yaml?download=1")
        );
        assert!(
            matches!(http, OpenApiSource::Url(url) if url.as_str() == "http://example.com/petstore.json")
        );
    }

    #[test]
    fn rejects_unsupported_url_schemes() {
        let error = classify_source("ftp://example.com/openapi.json").unwrap_err();

        assert_eq!(
            error,
            SourceClassificationError::UnsupportedUrlScheme("ftp".to_string())
        );
        assert_eq!(error.to_string(), "the URL scheme 'ftp' is not supported");
    }

    #[test]
    fn rejects_invalid_http_urls() {
        let error = classify_source("https://").unwrap_err();

        assert!(matches!(
            &error,
            SourceClassificationError::InvalidUrl { value, .. } if value == "https://"
        ));
        assert_eq!(
            error.to_string(),
            "could not parse 'https://' as a URL: empty host"
        );
    }

    #[test]
    fn rejects_empty_input() {
        let error = classify_source("   ").unwrap_err();

        assert_eq!(error, SourceClassificationError::EmptyInput);
        assert_eq!(error.to_string(), "the OpenAPI path is empty");
    }
}
