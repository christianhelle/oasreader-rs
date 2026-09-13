//! Detection of the serialization format (JSON or YAML) of OpenAPI content.

use std::{fmt, path::Path};

use crate::OpenApiSource;

/// The serialization format of an OpenAPI document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenApiContentFormat {
    /// JavaScript Object Notation.
    Json,
    /// YAML Ain't Markup Language.
    Yaml,
}

/// Errors raised while detecting whether content is JSON or YAML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentFormatDetectionError {
    /// The content was empty or whitespace-only.
    EmptyContent,
    /// The content did not look like JSON or YAML.
    UnknownFormat,
}

impl fmt::Display for ContentFormatDetectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyContent => write!(f, "OpenAPI content cannot be empty"),
            Self::UnknownFormat => write!(f, "unable to detect OpenAPI content format"),
        }
    }
}

impl std::error::Error for ContentFormatDetectionError {}

impl OpenApiContentFormat {
    /// Returns a human-readable name for the format.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Yaml => "YAML",
        }
    }

    /// Infers the format from a file extension (`.json`, `.yaml` or `.yml`).
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let extension = path.as_ref().extension()?.to_str()?;

        match extension.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            _ => None,
        }
    }
}

impl fmt::Display for OpenApiContentFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl OpenApiSource {
    /// Returns a format hint derived from the path or URL extension.
    pub fn format_hint(&self) -> Option<OpenApiContentFormat> {
        match self {
            Self::Path(path) => OpenApiContentFormat::from_path(path),
            Self::Url(url) => OpenApiContentFormat::from_path(url.path()),
        }
    }
}

/// Detects the content format using the source extension first, then content sniffing.
///
/// # Examples
///
/// ```
/// use oasreader::{OpenApiContentFormat, classify_source, detect_content_format};
///
/// let source = classify_source("specs/petstore").unwrap();
/// let format = detect_content_format(Some(&source), "openapi: 3.0.0\n").unwrap();
///
/// assert_eq!(format, OpenApiContentFormat::Yaml);
/// ```
pub fn detect_content_format(
    source: Option<&OpenApiSource>,
    content: &str,
) -> Result<OpenApiContentFormat, ContentFormatDetectionError> {
    if normalized_content(content).is_empty() {
        return Err(ContentFormatDetectionError::EmptyContent);
    }

    match source.and_then(OpenApiSource::format_hint) {
        Some(format) => Ok(format),
        None => sniff_content_format(content),
    }
}

/// Detects the content format from the text alone.
pub fn sniff_content_format(
    content: &str,
) -> Result<OpenApiContentFormat, ContentFormatDetectionError> {
    let normalized = normalized_content(content);

    match normalized.chars().next() {
        None => Err(ContentFormatDetectionError::EmptyContent),
        Some('{' | '[') => Ok(OpenApiContentFormat::Json),
        Some(_) if looks_like_yaml(normalized) => Ok(OpenApiContentFormat::Yaml),
        Some(_) => Err(ContentFormatDetectionError::UnknownFormat),
    }
}

fn normalized_content(content: &str) -> &str {
    content
        .strip_prefix('\u{feff}')
        .unwrap_or(content)
        .trim_start()
}

fn looks_like_yaml(content: &str) -> bool {
    let first_line = content
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .find(|line| !line.is_empty());

    first_line.is_some_and(|line| {
        let looks_like_mapping = line.find(':').is_some_and(|index| {
            let key = line[..index].trim();
            let value = line[index + 1..].chars().next();

            !key.is_empty() && value.is_none_or(char::is_whitespace)
        });

        line == "---" || line.starts_with("%YAML") || line.starts_with("- ") || looks_like_mapping
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify_source;

    #[test]
    fn detects_json_from_path_extension() {
        assert_eq!(
            OpenApiContentFormat::from_path("petstore.json"),
            Some(OpenApiContentFormat::Json)
        );
    }

    #[test]
    fn detects_yaml_from_path_extension_case_insensitively() {
        assert_eq!(
            OpenApiContentFormat::from_path("petstore.YML"),
            Some(OpenApiContentFormat::Yaml)
        );
        assert_eq!(
            OpenApiContentFormat::from_path("petstore.yaml"),
            Some(OpenApiContentFormat::Yaml)
        );
        assert_eq!(OpenApiContentFormat::from_path("petstore"), None);
    }

    #[test]
    fn url_sources_hint_from_the_url_path_only() {
        let source = classify_source("https://example.com/openapi.yaml?download=1").unwrap();

        assert_eq!(source.format_hint(), Some(OpenApiContentFormat::Yaml));
    }

    #[test]
    fn prefers_source_hint_when_available() {
        let source = classify_source("https://example.com/openapi.yaml").unwrap();

        let format = detect_content_format(Some(&source), "{\"openapi\":\"3.1.0\"}").unwrap();

        assert_eq!(format, OpenApiContentFormat::Yaml);
    }

    #[test]
    fn falls_back_to_content_sniffing_without_a_known_extension() {
        let source = classify_source("specs/petstore").unwrap();

        let format = detect_content_format(Some(&source), "{\"openapi\":\"3.1.0\"}").unwrap();

        assert_eq!(format, OpenApiContentFormat::Json);
    }

    #[test]
    fn sniffs_json_after_utf8_bom() {
        let format = sniff_content_format("\u{feff}\n  {\"openapi\":\"3.0.0\"}").unwrap();

        assert_eq!(format, OpenApiContentFormat::Json);
    }

    #[test]
    fn sniffs_yaml_from_mapping_content() {
        let format =
            sniff_content_format("# comment\nopenapi: 3.0.0\ninfo:\n  title: Example").unwrap();

        assert_eq!(format, OpenApiContentFormat::Yaml);
    }

    #[test]
    fn reports_empty_content() {
        let error = detect_content_format(None, "  \n\t").unwrap_err();

        assert_eq!(error, ContentFormatDetectionError::EmptyContent);
        assert_eq!(error.to_string(), "OpenAPI content cannot be empty");
    }

    #[test]
    fn reports_unrecognized_content() {
        let error = sniff_content_format("https://example.com/openapi.json").unwrap_err();

        assert_eq!(error, ContentFormatDetectionError::UnknownFormat);
        assert_eq!(error.to_string(), "unable to detect OpenAPI content format");
    }

    #[test]
    fn formats_render_friendly_names() {
        assert_eq!(OpenApiContentFormat::Json.to_string(), "JSON");
        assert_eq!(OpenApiContentFormat::Yaml.to_string(), "YAML");
    }
}
