//! Merging of external references into a single document.

mod components;
mod refs;
mod walk;

use std::{collections::HashMap, fmt};

use serde_json::{Map, Value};

use crate::{
    OpenApiSource, OpenApiSpecificationVersion, ResourceLoader, decode_raw_document,
    detect_specification_version,
};
use components::{ComponentKind, Layout, component_at};
use refs::{local_reference, resolve_pointer, split_reference};
use walk::{for_each_reference, is_external_reference, is_literal_subtree, is_named_entry_map};

pub use walk::contains_external_references;

/// A problem found while merging external references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostic {
    /// The referenced file could not be loaded, or does not contain the referenced location.
    /// The reference is left unchanged.
    UnresolvedReference {
        /// The `$ref` value as written.
        reference: String,
        /// The document containing the reference.
        referenced_from: OpenApiSource,
        /// Why the reference could not be resolved.
        reason: String,
    },
    /// An inlined reference points back at a location that is already being inlined. The
    /// reference is left unchanged.
    CircularReference {
        /// The `$ref` value as written.
        reference: String,
        /// The document containing the reference.
        referenced_from: OpenApiSource,
    },
    /// A referenced component has the same name as a different component that was already in the
    /// document or merged earlier. The existing component is kept and the reference points at it.
    NameConflict {
        /// The component name.
        name: String,
        /// The `$ref` value as written.
        reference: String,
        /// The document containing the reference.
        referenced_from: OpenApiSource,
    },
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedReference {
                reference,
                referenced_from,
                reason,
            } => write!(
                f,
                "could not resolve '{reference}' in {referenced_from}: {reason}"
            ),
            Self::CircularReference {
                reference,
                referenced_from,
            } => write!(
                f,
                "'{reference}' in {referenced_from} refers back to itself and was left unchanged"
            ),
            Self::NameConflict {
                name,
                reference,
                referenced_from,
            } => write!(
                f,
                "'{reference}' in {referenced_from} differs from the existing component '{name}', which was kept"
            ),
        }
    }
}

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
    raw: Value,
}

/// Why a reference could not be merged as written.
enum Problem {
    Unresolved(String),
    Circular,
    NameConflict { name: String, local: String },
}

enum Resolution {
    /// The reference now points at a component in the merged document.
    Component(String),
    /// The reference is replaced by this value, whose own references are already merged.
    Inline(Value),
    /// The reference could not be merged and is left unchanged.
    Unresolved,
}

struct Merger<'a> {
    loader: &'a dyn ResourceLoader,
    layout: Layout,
    documents: HashMap<OpenApiSource, Result<Value, String>>,
    load_order: Vec<OpenApiSource>,
    components: HashMap<(ComponentKind, String), Origin>,
    imports: Vec<(ComponentKind, String, Value)>,
    inlining: Vec<(OpenApiSource, Vec<String>)>,
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
            for (name, raw) in entries.into_iter().flatten() {
                if aliases_external_component(raw, name) {
                    continue;
                }
                let mut pointer = owned(&section);
                pointer.push(name.clone());
                components.insert(
                    (kind, name.clone()),
                    Origin {
                        source: source.clone(),
                        pointer,
                        raw: raw.clone(),
                    },
                );
            }
        }

        Self {
            loader,
            layout,
            documents: HashMap::from([(source.clone(), Ok(root.clone()))]),
            load_order: Vec::new(),
            components,
            imports: Vec::new(),
            inlining: Vec::new(),
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

                let mut inlined = None;
                match reference.map(|reference| self.resolve(&reference, base)) {
                    Some(Resolution::Component(local)) => {
                        object.insert("$ref".to_string(), Value::String(local));
                    }
                    Some(Resolution::Inline(target)) => {
                        object.remove("$ref");
                        inlined = Some(target);
                    }
                    Some(Resolution::Unresolved) | None => {}
                }

                for (key, child) in object.iter_mut() {
                    if (!in_map && key == "$ref") || is_literal_subtree(key, child, in_map) {
                        continue;
                    }
                    let child_in_map = is_named_entry_map(key, in_map);
                    self.rewrite(child, base, in_root, child_in_map);
                }

                if let Some(target) = inlined {
                    *value = with_siblings(target, std::mem::take(object));
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
        let referenced_from = base.clone();
        let (resolution, problem) = match self.try_resolve(reference, base) {
            Ok(resolution) => (resolution, None),
            Err(Problem::Unresolved(reason)) => (
                Resolution::Unresolved,
                Some(Diagnostic::UnresolvedReference {
                    reference: reference.to_string(),
                    referenced_from,
                    reason,
                }),
            ),
            Err(Problem::Circular) => (
                Resolution::Unresolved,
                Some(Diagnostic::CircularReference {
                    reference: reference.to_string(),
                    referenced_from,
                }),
            ),
            Err(Problem::NameConflict { name, local }) => (
                Resolution::Component(local),
                Some(Diagnostic::NameConflict {
                    name,
                    reference: reference.to_string(),
                    referenced_from,
                }),
            ),
        };

        self.diagnostics.extend(problem);
        resolution
    }

    fn try_resolve(
        &mut self,
        reference: &str,
        base: &OpenApiSource,
    ) -> Result<Resolution, Problem> {
        let (resource, pointer) = split_reference(reference);
        let target = if resource.is_empty() {
            base.clone()
        } else {
            base.join(resource)
                .map_err(|error| Problem::Unresolved(error.to_string()))?
        };

        match component_at(&pointer) {
            Some((kind, name)) if self.layout.section(kind).is_some() => {
                let name = name.to_string();
                self.import(kind, name, target, pointer)
            }
            _ => self.inline(target, pointer),
        }
    }

    fn inline(
        &mut self,
        target: OpenApiSource,
        pointer: Vec<String>,
    ) -> Result<Resolution, Problem> {
        let location = (target, pointer);
        if self.inlining.contains(&location) {
            return Err(Problem::Circular);
        }

        let mut value = self
            .value_at(&location.0, &location.1)
            .map_err(Problem::Unresolved)?;

        let base = location.0.clone();
        self.inlining.push(location);
        self.rewrite(&mut value, &base, false, false);
        self.inlining.pop();

        Ok(Resolution::Inline(value))
    }

    fn import(
        &mut self,
        kind: ComponentKind,
        name: String,
        target: OpenApiSource,
        pointer: Vec<String>,
    ) -> Result<Resolution, Problem> {
        let Some(mut section) = self.layout.section(kind) else {
            return Err(Problem::Unresolved(format!(
                "the document layout has no section for {kind:?} components"
            )));
        };
        section.push(&name);
        let local = local_reference(&section);

        let key = (kind, name);
        if let Some(origin) = self.components.get(&key)
            && origin.source == target
            && origin.pointer == pointer
        {
            return Ok(Resolution::Component(local));
        }

        let raw = self
            .value_at(&target, &pointer)
            .map_err(Problem::Unresolved)?;

        if aliases_external_component(&raw, &key.1) {
            return self.follow_alias(&raw, target, pointer);
        }

        if let Some(existing) = self.components.get(&key) {
            // The first component registered under a name wins, as in the .NET oasreader.
            return if existing.raw == raw {
                Ok(Resolution::Component(local))
            } else {
                Err(Problem::NameConflict { name: key.1, local })
            };
        }

        self.components.insert(
            key.clone(),
            Origin {
                source: target.clone(),
                pointer,
                raw: raw.clone(),
            },
        );
        let slot = self.imports.len();
        self.imports.push((key.0, key.1, Value::Null));

        let mut value = raw;
        self.rewrite(&mut value, &target, false, false);
        self.imports[slot].2 = value;

        Ok(Resolution::Component(local))
    }

    /// Resolves a component that only references a same-name component in another file.
    fn follow_alias(
        &mut self,
        alias: &Value,
        target: OpenApiSource,
        pointer: Vec<String>,
    ) -> Result<Resolution, Problem> {
        let location = (target, pointer);
        if self.inlining.contains(&location) {
            return Err(Problem::Circular);
        }

        let reference = alias["$ref"].as_str().unwrap_or_default().to_string();
        let base = location.0.clone();
        self.inlining.push(location);
        let resolution = self.try_resolve(&reference, &base);
        self.inlining.pop();
        resolution
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
            self.load_order.push(source.clone());
        }

        let document = self.documents[source].as_ref().map_err(Clone::clone)?;
        resolve_pointer(document, pointer)
            .cloned()
            .ok_or_else(|| format!("{source} does not contain the referenced location"))
    }

    /// Imports components that the merged document references locally but does not define,
    /// from any document loaded while merging.
    fn import_missing_components(&mut self, document: &Value) {
        loop {
            let mut references = Vec::new();
            for_each_reference(document, &mut |reference| {
                references.push(reference.to_string())
            });
            for (_, _, value) in &self.imports {
                for_each_reference(value, &mut |reference| {
                    references.push(reference.to_string())
                });
            }

            let imported_before = self.imports.len();
            for reference in references {
                let (resource, pointer) = split_reference(&reference);
                let Some((kind, name)) = component_at(&pointer)
                    .filter(|_| resource.is_empty())
                    .map(|(kind, name)| (kind, name.to_string()))
                else {
                    continue;
                };
                if self.components.contains_key(&(kind, name.clone())) {
                    continue;
                }

                let candidate = self.load_order.iter().find(|source| {
                    matches!(&self.documents[*source], Ok(loaded) if resolve_pointer(loaded, &pointer).is_some())
                });
                if let Some(source) = candidate.cloned() {
                    let _ = self.import(kind, name, source, pointer);
                }
            }

            if self.imports.len() == imported_before {
                break;
            }
        }
    }

    fn finish(mut self, document: &mut Value) -> MergeReport {
        self.import_missing_components(document);

        for (kind, name, value) in self.imports {
            let Some(section) = self.layout.section(kind) else {
                continue;
            };
            let local = local_reference(&[section.as_slice(), &[name.as_str()]].concat());
            let entries = section_mut(document, &section);
            match entries.get_mut(&name) {
                Some(existing) if existing.get("$ref").and_then(Value::as_str) == Some(&local) => {
                    *existing = value;
                }
                Some(_) => {}
                None => {
                    entries.insert(name, value);
                }
            }
        }

        // Schemas are ordered by name, as the .NET oasreader does after merging.
        if let Some(section) = self.layout.section(ComponentKind::Schema)
            && resolve_pointer(document, &owned(&section)).is_some()
        {
            section_mut(document, &section).sort_keys();
        }

        MergeReport {
            contained_external_references: true,
            diagnostics: self.diagnostics,
        }
    }
}

/// Returns `true` when a component is only an external `$ref` to a component of the same name,
/// as index files that re-export components from other files do.
fn aliases_external_component(component: &Value, name: &str) -> bool {
    let Some(object) = component.as_object().filter(|object| object.len() == 1) else {
        return false;
    };
    let Some(reference) = object.get("$ref").and_then(Value::as_str) else {
        return false;
    };
    let (resource, pointer) = split_reference(reference);
    !resource.is_empty() && component_at(&pointer).is_some_and(|(_, referenced)| referenced == name)
}

/// Applies the keywords that sat next to an inlined `$ref` on top of the referenced value.
fn with_siblings(target: Value, siblings: Map<String, Value>) -> Value {
    match target {
        Value::Object(mut object) if !siblings.is_empty() => {
            object.extend(siblings);
            Value::Object(object)
        }
        target => target,
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
