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
                OfficeBlockContent::Resolved { nodes } => document_lines(&language, nodes),
                OfficeBlockContent::Missing { reason } => vec![LineView {
                    kind: LineKind::Unresolved,
                    class: "missing".to_string(),
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
fn document_lines(language: &str, nodes: &[breviarium_data::DocumentNode]) -> Vec<LineView> {
    use breviarium_data::DocumentNode;

    let mut lines = Vec::new();
    for node in nodes {
        // Semantic class from the node type, e.g. `versicle`, `response`,
        // `short-response`, `antiphon`, `prayer`, `blessing`, `amen`, `heading`.
        let class = node.kind().replace('_', "-");
        match node {
            DocumentNode::Text { .. }
            | DocumentNode::Versicle { .. }
            | DocumentNode::Response { .. }
            | DocumentNode::ShortResponse { .. }
            | DocumentNode::Antiphon { .. }
            | DocumentNode::Prayer { .. }
            | DocumentNode::Blessing { .. }
            | DocumentNode::Amen => push_lines(
                &mut lines,
                LineKind::Text,
                &class,
                &node.plain_text_for_language(language),
            ),
            DocumentNode::Heading { .. }
            | DocumentNode::Marker { .. }
            | DocumentNode::Citation { .. } => push_lines(
                &mut lines,
                LineKind::Marker,
                &class,
                &node.plain_text_for_language(language),
            ),
            DocumentNode::Rubric { .. } => push_lines(
                &mut lines,
                LineKind::Rubric,
                &class,
                &node.plain_text_for_language(language),
            ),
            DocumentNode::Unresolved {
                kind,
                value,
                reason,
            } => lines.push(LineView {
                kind: LineKind::Unresolved,
                class,
                text: format!("unresolved {kind}: {value}; {reason}"),
            }),
            _ => lines.push(LineView {
                kind: LineKind::Unresolved,
                class: "unresolved".to_string(),
                text: "unknown output node".to_string(),
            }),
        }
    }
    lines
}

#[cfg(feature = "server")]
fn push_lines(lines: &mut Vec<LineView>, kind: LineKind, class: &str, text: &str) {
    // Each node is one block: multiline text (a hymn, a psalm's verses) stays
    // together and renders as one paragraph with internal line breaks, rather
    // than one `<p>` per source line.
    lines.push(LineView {
        kind,
        class: class.to_string(),
        text: text.to_string(),
    });
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
    pub(crate) text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LineKind {
    Text,
    Marker,
    Rubric,
    Unresolved,
}
