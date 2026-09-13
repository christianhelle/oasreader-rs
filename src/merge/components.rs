//! Mapping of reusable components between the Swagger 2.0 and OpenAPI 3.x layouts.

/// The kinds of reusable components a reference can point to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ComponentKind {
    Schema,
    Response,
    Parameter,
    Example,
    RequestBody,
    Header,
    SecurityScheme,
    Link,
    Callback,
    PathItem,
}

/// The component layout of the document components are merged into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// `#/definitions`, `#/parameters`, `#/responses`, `#/securityDefinitions`.
    Swagger2,
    /// `#/components/<kind>`.
    OpenApi3,
}

const OPENAPI3_SECTIONS: &[(&str, ComponentKind)] = &[
    ("schemas", ComponentKind::Schema),
    ("responses", ComponentKind::Response),
    ("parameters", ComponentKind::Parameter),
    ("examples", ComponentKind::Example),
    ("requestBodies", ComponentKind::RequestBody),
    ("headers", ComponentKind::Header),
    ("securitySchemes", ComponentKind::SecurityScheme),
    ("links", ComponentKind::Link),
    ("callbacks", ComponentKind::Callback),
    ("pathItems", ComponentKind::PathItem),
];

const SWAGGER2_SECTIONS: &[(&str, ComponentKind)] = &[
    ("definitions", ComponentKind::Schema),
    ("responses", ComponentKind::Response),
    ("parameters", ComponentKind::Parameter),
    ("securityDefinitions", ComponentKind::SecurityScheme),
];

/// Returns the component kind and name when the pointer addresses a whole named component.
///
/// Both layouts are recognized regardless of the referenced document's own version.
pub(crate) fn component_at(segments: &[String]) -> Option<(ComponentKind, &str)> {
    match segments {
        [components, section, name] if components == "components" => OPENAPI3_SECTIONS
            .iter()
            .find(|(candidate, _)| candidate == section)
            .map(|(_, kind)| (*kind, name.as_str())),
        [section, name] => SWAGGER2_SECTIONS
            .iter()
            .find(|(candidate, _)| candidate == section)
            .map(|(_, kind)| (*kind, name.as_str())),
        _ => None,
    }
}

impl Layout {
    /// Returns every component kind the layout supports with its section pointer.
    pub(crate) fn sections(self) -> Vec<(ComponentKind, Vec<&'static str>)> {
        let table = match self {
            Self::OpenApi3 => OPENAPI3_SECTIONS,
            Self::Swagger2 => SWAGGER2_SECTIONS,
        };
        table
            .iter()
            .filter_map(|(_, kind)| Some((*kind, self.section(*kind)?)))
            .collect()
    }

    /// Returns the pointer segments of the section holding `kind`, if the layout supports it.
    pub(crate) fn section(self, kind: ComponentKind) -> Option<Vec<&'static str>> {
        match self {
            Self::OpenApi3 => OPENAPI3_SECTIONS
                .iter()
                .find(|(_, candidate)| *candidate == kind)
                .map(|(section, _)| vec!["components", *section]),
            Self::Swagger2 => SWAGGER2_SECTIONS
                .iter()
                .find(|(_, candidate)| *candidate == kind)
                .map(|(section, _)| vec![*section]),
        }
    }
}
