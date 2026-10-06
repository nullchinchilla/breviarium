use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[server]
pub async fn load_office(
    date: String,
    hour: String,
    language: String,
) -> Result<OfficeView, ServerFnError> {
    load_office_impl(date, hour, language).map_err(ServerFnError::new)
}

#[cfg(feature = "server")]
fn load_office_impl(date: String, hour: String, language: String) -> Result<OfficeView, String> {
    use breviarium_data::{Breviarium, OfficeBlockContent, OfficeRequest};
    use chrono::NaiveDate;

    let parsed_date = NaiveDate::parse_from_str(&date, "%Y%m%d")
        .map_err(|error| format!("invalid date `{date}`: {error}"))?;
    let parsed_hour =
        parse_data_hour(&hour).ok_or_else(|| format!("unknown Office hour `{hour}`"))?;

    let engine =
        Breviarium::embedded().map_err(|error| format!("failed to load embedded data: {error}"))?;
    let office = engine
        .resolve_office(
            OfficeRequest::new(parsed_date, parsed_hour).with_language(language.as_str()),
        )
        .map_err(|error| format!("failed to resolve Office: {error}"))?;
    let catalog = engine.catalog();

    let title = office
        .principal
        .title
        .clone()
        .unwrap_or_else(|| office.principal.id.clone());
    let diagnostics = office
        .diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>();
    let blocks = office
        .blocks
        .iter()
        .map(|block| {
            let fallback_title = format!("{:?}", block.role);
            let title = block
                .title
                .clone()
                .unwrap_or_else(|| fallback_title.clone());
            let class = fallback_title.to_ascii_lowercase();
            let lines = match &block.content {
                OfficeBlockContent::Resolved { nodes } => document_lines(&language, catalog, nodes),
                OfficeBlockContent::Missing { reason } => vec![LineView {
                    kind: LineKind::Unresolved,
                    class: "missing".to_string(),
                    marker: None,
                    text: format!("Missing: {reason}"),
                }],
                _ => Vec::new(),
            };
            OfficeBlockView {
                title,
                class,
                lines,
            }
        })
        .collect();

    Ok(OfficeView {
        title,
        blocks,
        diagnostics,
    })
}

#[cfg(feature = "server")]
fn parse_data_hour(value: &str) -> Option<breviarium_data::Hour> {
    use breviarium_data::Hour;

    match value.to_ascii_lowercase().as_str() {
        "matins" | "matutinum" => Some(Hour::Matins),
        "lauds" | "laudes" => Some(Hour::Lauds),
        "prime" | "prima" => Some(Hour::Prime),
        "terce" | "tertia" => Some(Hour::Terce),
        "sext" | "sexta" => Some(Hour::Sext),
        "none" | "nona" => Some(Hour::None),
        "vespers" | "vespera" | "vesperae" => Some(Hour::Vespers),
        "compline" | "completorium" => Some(Hour::Compline),
        _ => None,
    }
}

#[cfg(feature = "server")]
fn document_lines(
    language: &str,
    catalog: &breviarium_data::Catalog,
    nodes: &[breviarium_data::DocumentNode],
) -> Vec<LineView> {
    use breviarium_data::DocumentNode;

    let mut lines = Vec::new();
    for node in nodes {
        // Semantic class from the node type, e.g. `versicle`, `response`,
        // `short-response`, `antiphon`, `prayer`, `blessing`, `amen`, `heading`.
        let class = node.kind().replace('_', "-");
        // The leading versicle/response/antiphon/blessing marker travels as
        // structured data (role + localized label), so the renderer presents it
        // without re-parsing the text. The body is the clean, marker-free text.
        let marker = node.line_marker(language, catalog).map(|marker| MarkerView {
            kind: marker.kind.as_str().to_string(),
            label: marker.label,
        });
        let text = node.body_for_language(language, catalog);
        let line = |kind| LineView {
            kind,
            class: class.clone(),
            marker: marker.clone(),
            text: text.clone(),
        };
        match node {
            DocumentNode::Text { .. }
            | DocumentNode::Versicle { .. }
            | DocumentNode::Response { .. }
            | DocumentNode::ShortResponse { .. }
            | DocumentNode::Antiphon { .. }
            | DocumentNode::Prayer { .. }
            | DocumentNode::Blessing { .. }
            | DocumentNode::Amen => lines.push(line(LineKind::Text)),
            DocumentNode::Heading { .. }
            | DocumentNode::Marker { .. }
            | DocumentNode::Citation { .. } => lines.push(line(LineKind::Marker)),
            DocumentNode::Rubric { .. } => lines.push(line(LineKind::Rubric)),
            DocumentNode::Unresolved { .. } => lines.push(line(LineKind::Unresolved)),
            _ => lines.push(LineView {
                kind: LineKind::Unresolved,
                class: "unresolved".to_string(),
                marker: None,
                text: "unknown output node".to_string(),
            }),
        }
    }
    lines
}

/// The resolved Office payload returned by the backend.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct OfficeView {
    /// Liturgical date title, e.g. `S. Iulianae de Falconeriis Virginis`.
    pub(crate) title: String,
    pub(crate) blocks: Vec<OfficeBlockView>,
    pub(crate) diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct OfficeBlockView {
    pub(crate) title: String,
    /// Section class derived from the block's semantic role.
    pub(crate) class: String,
    /// This language's logical lines for the block, in order. Block structure is
    /// language-independent, so these line up by position with the other
    /// languages' blocks when zipped into rows for display.
    pub(crate) lines: Vec<LineView>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct LineView {
    pub(crate) kind: LineKind,
    /// Semantic CSS class derived from the source node type (`versicle`,
    /// `response`, `antiphon`, `hymn`-bearing `text`, ...).
    pub(crate) class: String,
    /// Optional leading marker (versicle/response siglum, antiphon/blessing
    /// label) carried as structured data — the renderer chooses the glyph or
    /// label rather than parsing it back out of `text`.
    pub(crate) marker: Option<MarkerView>,
    pub(crate) text: String,
}

/// A structured inline line marker for the renderer: the semantic `kind`
/// (`versicle`, `response`, `short-response`, `antiphon`, `blessing`) and the
/// localized `label` to show when the renderer presents a label rather than a
/// glyph.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct MarkerView {
    pub(crate) kind: String,
    pub(crate) label: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LineKind {
    Text,
    Marker,
    Rubric,
    Unresolved,
}
