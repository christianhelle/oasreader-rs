//! Parsing of `$ref` values and JSON pointers.

use serde_json::Value;

/// Splits a reference into the resource before `#` and the JSON pointer segments after it.
pub(crate) fn split_reference(reference: &str) -> (&str, Vec<String>) {
    let (resource, fragment) = reference.split_once('#').unwrap_or((reference, ""));
    (resource, parse_pointer(fragment))
}

/// Parses a JSON pointer such as `/components/schemas/Pet` into unescaped segments.
fn parse_pointer(fragment: &str) -> Vec<String> {
    let decoded = percent_decode(fragment);
    decoded
        .split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// Builds a local reference (`#/a/b`) from pointer segments.
pub(crate) fn local_reference(segments: &[&str]) -> String {
    let mut reference = String::from("#");
    for segment in segments {
        reference.push('/');
        reference.push_str(&segment.replace('~', "~0").replace('/', "~1"));
    }
    reference
}

/// Returns the value at the pointer segments.
pub(crate) fn resolve_pointer<'a>(document: &'a Value, segments: &[String]) -> Option<&'a Value> {
    segments
        .iter()
        .try_fold(document, |value, segment| match value {
            Value::Object(object) => object.get(segment),
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?),
            _ => None,
        })
}

fn percent_decode(value: &str) -> String {
    if !value.contains('%') {
        return value.to_string();
    }

    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| value.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}
