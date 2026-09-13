//! Merging of external references into a single document.

mod components;
mod refs;
mod walk;

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::{
    OpenApiSource, OpenApiSpecificationVersion, ResourceLoader, decode_raw_document,
    detect_specification_version,
};
use components::{ComponentKind, Layout, component_at};
use refs::{local_reference, resolve_pointer, split_reference};
use walk::{is_external_reference, is_literal_subtree, is_named_entry_map};

pub use walk::contains_external_references;

/// A problem found while merging external references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {}

/// The outcome of [`merge_external_references`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    /// Whether the document referenced other files or URLs before merging.
    pub contained_external_references: bool,
    /// Problems found while merging. References that could not be merged are left unchanged.
    pub diagnostics: Vec<Diagnostic>,
}

/// Merges every externally referenced component into `document`.
///
/// `source` is where `document` was loaded from; relative references are resolved against it.
/// Referenced components are copied into the document's own components section and the
/// references are rewritten to point at the copies. Documents without external references are
/// left untouched.
pub fn merge_external_references(
    document: &mut Value,
    source: &OpenApiSource,
    loader: &dyn ResourceLoader,
) -> MergeReport {
    if !contains_external_references(document) {
        return MergeReport::default();
    }

    let layout = match detect_specification_version(document) {
        Ok(OpenApiSpecificationVersion::Swagger2) => Layout::Swagger2,
        _ => Layout::OpenApi3,
    };

    let mut merger = Merger::new(document, source, layout, loader);
    merger.rewrite(document, source, true, false);
    merger.finish(document)
}

/// Where a component known to the merged document came from.
struct Origin {
    source: OpenApiSource,
    pointer: Vec<String>,
}

enum Resolution {
    /// The reference now points at a component in the merged document.
    Component(String),
    /// The reference could not be merged and is left unchanged.
    Unresolved,
}

struct Merger<'a> {
    loader: &'a dyn ResourceLoader,
    layout: Layout,
    documents: HashMap<OpenApiSource, Result<Value, String>>,
    components: HashMap<(ComponentKind, String), Origin>,
    imports: Vec<(ComponentKind, String, Value)>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Merger<'a> {
    fn new(
        root: &Value,
        source: &OpenApiSource,
        layout: Layout,
        loader: &'a dyn ResourceLoader,
    ) -> Self {
        let mut components = HashMap::new();
        for (kind, section) in layout.sections() {
            let entries = resolve_pointer(root, &owned(&section)).and_then(Value::as_object);
            for name in entries.into_iter().flat_map(Map::keys) {
                let mut pointer = owned(&section);
                pointer.push(name.clone());
                components.insert(
                    (kind, name.clone()),
                    Origin {
                        source: source.clone(),
                        pointer,
                    },
                );
            }
        }

        Self {
            loader,
            layout,
            documents: HashMap::from([(source.clone(), Ok(root.clone()))]),
            components,
            imports: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Rewrites the references in `value`, which belongs to the document at `base`.
    ///
    /// Local references are kept in the root document and imported from any other document.
    fn rewrite(&mut self, value: &mut Value, base: &OpenApiSource, in_root: bool, in_map: bool) {
        match value {
            Value::Object(object) => {
                let reference = object
                    .get("$ref")
                    .and_then(Value::as_str)
                    .filter(|reference| !in_map && (!in_root || is_external_reference(reference)))
                    .map(str::to_string);

                if let Some(reference) = reference
                    && let Resolution::Component(local) = self.resolve(&reference, base)
                {
                    object.insert("$ref".to_string(), Value::String(local));
                }

                for (key, child) in object.iter_mut() {
                    if (!in_map && key == "$ref") || is_literal_subtree(key, child, in_map) {
                        continue;
                    }
                    let child_in_map = is_named_entry_map(key, in_map);
                    self.rewrite(child, base, in_root, child_in_map);
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.rewrite(item, base, in_root, false);
                }
            }
            _ => {}
        }
    }

    fn resolve(&mut self, reference: &str, base: &OpenApiSource) -> Resolution {
        let (resource, pointer) = split_reference(reference);
        let target = if resource.is_empty() {
            base.clone()
        } else {
            match base.join(resource) {
                Ok(target) => target,
                Err(_) => return Resolution::Unresolved,
            }
        };

        match component_at(&pointer) {
            Some((kind, name)) if self.layout.section(kind).is_some() => {
                let name = name.to_string();
                self.import(kind, name, target, pointer)
            }
            _ => Resolution::Unresolved,
        }
    }

    fn import(
        &mut self,
        kind: ComponentKind,
        name: String,
        target: OpenApiSource,
        pointer: Vec<String>,
    ) -> Resolution {
        let Some(mut section) = self.layout.section(kind) else {
            return Resolution::Unresolved;
        };
        section.push(&name);
        let local = local_reference(&section);

        let key = (kind, name);
        if let Some(origin) = self.components.get(&key)
            && origin.source == target
            && origin.pointer == pointer
        {
            return Resolution::Component(local);
        }

        let raw = match self.value_at(&target, &pointer) {
            Ok(raw) => raw,
            Err(_) => return Resolution::Unresolved,
        };

        if self.components.contains_key(&key) {
            // The first component registered under a name wins, as in the .NET oasreader.
            return Resolution::Component(local);
        }

        self.components.insert(
            key.clone(),
            Origin {
                source: target.clone(),
                pointer,
            },
        );
        let slot = self.imports.len();
        self.imports.push((key.0, key.1, Value::Null));

        let mut value = raw;
        self.rewrite(&mut value, &target, false, false);
        self.imports[slot].2 = value;

        Resolution::Component(local)
    }

    fn value_at(&mut self, source: &OpenApiSource, pointer: &[String]) -> Result<Value, String> {
        if !self.documents.contains_key(source) {
            let loaded = self
                .loader
                .load(source)
                .map_err(|error| error.to_string())
                .and_then(|content| {
                    decode_raw_document(source.clone(), content)
                        .map(|raw| raw.into_value())
                        .map_err(|error| error.to_string())
                });
            self.documents.insert(source.clone(), loaded);
        }

        let document = self.documents[source].as_ref().map_err(Clone::clone)?;
        resolve_pointer(document, pointer)
            .cloned()
            .ok_or_else(|| format!("{source} does not contain the referenced location"))
    }

    fn finish(self, document: &mut Value) -> MergeReport {
        for (kind, name, value) in self.imports {
            let Some(section) = self.layout.section(kind) else {
                continue;
            };
            let entries = section_mut(document, &section);
            entries.entry(name).or_insert(value);
        }

        MergeReport {
            contained_external_references: true,
            diagnostics: self.diagnostics,
        }
    }
}

fn owned(segments: &[&str]) -> Vec<String> {
    segments.iter().map(|segment| segment.to_string()).collect()
}

/// Returns the object at `section`, creating it (and its parents) when missing.
fn section_mut<'v>(document: &'v mut Value, section: &[&str]) -> &'v mut Map<String, Value> {
    let mut current = document;
    for segment in section {
        let object = ensure_object(current);
        current = object
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    ensure_object(current)
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value
        .as_object_mut()
        .expect("value was just made an object")
}
