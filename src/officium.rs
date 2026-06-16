use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[component]
pub fn Officium(date: String, hour: String) -> Element {
    let route_date = date.clone();
    let route_hour = hour.clone();
    let office = use_loader(move || load_office(route_date.clone(), route_hour.clone()))?;
    let office = office();

    rsx! {
        document::Title { "{office.page_title}" }

        main { class: "container",
            header {
                nav {
                    ul {
                        li { strong { "Breviarium" } }
                    }
                    ul {
                        li { a { href: "{office.previous_date_path}", "Previous day" } }
                        li { a { href: "{office.next_date_path}", "Next day" } }
                    }
                }
                h1 { "{office.title}" }
                p {
                    "{office.date_label} · {office.hour_label}"
                    br {}
                    small { "{office.profile_label}" }
                }
                nav {
                    ul {
                        for link in &office.hour_links {
                            li {
                                a {
                                    href: "{link.href}",
                                    aria_current: if link.current { "page" } else { "false" },
                                    "{link.label}"
                                }
                            }
                        }
                    }
                }
            }

            if !office.diagnostics.is_empty() {
                article {
                    header { strong { "Diagnostics" } }
                    ul {
                        for diagnostic in &office.diagnostics {
                            li { "{diagnostic}" }
                        }
                    }
                }
            }

            for block in office.blocks {
                section {
                    h2 { "{block.title}" }
                    div { class: "grid",
                        for column in block.columns {
                            article {
                                header { h3 { "{column.title}" } }
                                if let Some(reason) = &column.missing {
                                    p { mark { "Missing: {reason}" } }
                                }
                                for line in column.lines {
                                    OfficeLine { line }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn OfficeLine(line: LineView) -> Element {
    match line.kind {
        LineKind::Text => rsx! { p { "{line.text}" } },
        LineKind::Marker => rsx! { p { small { em { "{line.text}" } } } },
        LineKind::Rubric => rsx! { p { small { "{line.text}" } } },
        LineKind::Unresolved => rsx! { p { mark { "{line.text}" } } },
    }
}

#[server]
async fn load_office(date: String, hour: String) -> std::result::Result<OfficeView, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use breviarium_data::{Breviarium, OfficeColumnContent, OfficeRequest};
        use chrono::{Duration, NaiveDate};

        let parsed_date = NaiveDate::parse_from_str(&date, "%Y%m%d")
            .map_err(|error| ServerFnError::new(format!("invalid date `{date}`: {error}")))?;
        let parsed_hour = parse_data_hour(&hour)
            .ok_or_else(|| ServerFnError::new(format!("unknown Office hour `{hour}`")))?;

        let mut request = OfficeRequest::new(parsed_date, parsed_hour);
        request.languages = vec!["la".to_string(), "en".to_string()];

        let engine = Breviarium::embedded().map_err(|error| {
            ServerFnError::new(format!("failed to load embedded data: {error}"))
        })?;
        let office = engine
            .resolve_office(request)
            .map_err(|error| ServerFnError::new(format!("failed to resolve Office: {error}")))?;

        let date_path = parsed_date.format("%Y%m%d").to_string();
        let hour_path = canonical_hour_path(parsed_hour).to_string();
        let title = office
            .principal
            .title
            .clone()
            .unwrap_or_else(|| office.principal.id.clone());
        let page_title = format!("{title} - {}", display_hour(parsed_hour));
        let previous_date = parsed_date - Duration::days(1);
        let next_date = parsed_date + Duration::days(1);
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
                let columns = block
                    .columns
                    .iter()
                    .map(|column| {
                        let title = column.title.clone().unwrap_or_else(|| {
                            format!("{} - {fallback_title}", language_label(&column.language))
                        });
                        match &column.content {
                            OfficeColumnContent::Resolved { nodes } => OfficeColumnView {
                                title,
                                missing: None,
                                lines: document_lines(&column.language, nodes),
                            },
                            OfficeColumnContent::Missing { reason } => OfficeColumnView {
                                title,
                                missing: Some(reason.clone()),
                                lines: Vec::new(),
                            },
                            _ => OfficeColumnView {
                                title,
                                missing: Some("unknown content state".to_string()),
                                lines: Vec::new(),
                            },
                        }
                    })
                    .collect();
                OfficeBlockView {
                    title: block
                        .columns
                        .iter()
                        .find_map(|column| column.title.clone())
                        .unwrap_or(fallback_title),
                    columns,
                }
            })
            .collect();

        Ok(OfficeView {
            page_title,
            title,
            date_label: parsed_date.format("%B %-d, %Y").to_string(),
            hour_label: display_hour(parsed_hour).to_string(),
            profile_label: office.profile,
            previous_date_path: format!(
                "/officium/{}/{}",
                previous_date.format("%Y%m%d"),
                hour_path
            ),
            next_date_path: format!("/officium/{}/{}", next_date.format("%Y%m%d"), hour_path),
            hour_links: hour_links(&date_path, &hour_path),
            blocks,
            diagnostics,
        })
    }

    #[cfg(not(feature = "server"))]
    {
        let _ = (date, hour);
        unreachable!("server functions are executed by the server runtime")
    }
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
fn canonical_hour_path(hour: breviarium_data::Hour) -> &'static str {
    use breviarium_data::Hour;

    match hour {
        Hour::Matins => "matutinum",
        Hour::Lauds => "laudes",
        Hour::Prime => "prima",
        Hour::Terce => "tertia",
        Hour::Sext => "sexta",
        Hour::None => "nona",
        Hour::Vespers => "vesperae",
        Hour::Compline => "completorium",
        _ => "office",
    }
}

#[cfg(feature = "server")]
fn display_hour(hour: breviarium_data::Hour) -> &'static str {
    use breviarium_data::Hour;

    match hour {
        Hour::Matins => "Matins",
        Hour::Lauds => "Lauds",
        Hour::Prime => "Prime",
        Hour::Terce => "Terce",
        Hour::Sext => "Sext",
        Hour::None => "None",
        Hour::Vespers => "Vespers",
        Hour::Compline => "Compline",
        _ => "Office",
    }
}

#[cfg(feature = "server")]
fn language_label(language: &str) -> &'static str {
    match language {
        "la" => "Latin",
        "en" => "English",
        _ => "Text",
    }
}

#[cfg(feature = "server")]
fn document_lines(language: &str, nodes: &[breviarium_data::DocumentNode]) -> Vec<LineView> {
    use breviarium_data::DocumentNode;

    let mut lines = Vec::new();
    for node in nodes {
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
                &node.plain_text_for_language(language),
            ),
            DocumentNode::Heading { .. }
            | DocumentNode::Marker { .. }
            | DocumentNode::Citation { .. } => push_lines(
                &mut lines,
                LineKind::Marker,
                &node.plain_text_for_language(language),
            ),
            DocumentNode::Rubric { .. } => push_lines(
                &mut lines,
                LineKind::Rubric,
                &node.plain_text_for_language(language),
            ),
            DocumentNode::Unresolved {
                kind,
                value,
                reason,
            } => lines.push(LineView {
                kind: LineKind::Unresolved,
                text: format!("unresolved {kind}: {value}; {reason}"),
            }),
            _ => lines.push(LineView {
                kind: LineKind::Unresolved,
                text: "unknown output node".to_string(),
            }),
        }
    }
    lines
}

#[cfg(feature = "server")]
fn push_lines(lines: &mut Vec<LineView>, kind: LineKind, text: &str) {
    if text.is_empty() {
        lines.push(LineView {
            kind,
            text: String::new(),
        });
        return;
    }

    lines.extend(text.lines().map(|line| LineView {
        kind,
        text: line.to_string(),
    }));
}

#[cfg(feature = "server")]
fn hour_links(date_path: &str, current_hour: &str) -> Vec<HourLinkView> {
    [
        ("matutinum", "Matins"),
        ("laudes", "Lauds"),
        ("prima", "Prime"),
        ("tertia", "Terce"),
        ("sexta", "Sext"),
        ("nona", "None"),
        ("vesperae", "Vespers"),
        ("completorium", "Compline"),
    ]
    .into_iter()
    .map(|(hour, label)| HourLinkView {
        label: label.to_string(),
        href: format!("/officium/{date_path}/{hour}"),
        current: hour == current_hour,
    })
    .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct OfficeView {
    page_title: String,
    title: String,
    date_label: String,
    hour_label: String,
    profile_label: String,
    previous_date_path: String,
    next_date_path: String,
    hour_links: Vec<HourLinkView>,
    blocks: Vec<OfficeBlockView>,
    diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct HourLinkView {
    label: String,
    href: String,
    current: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct OfficeBlockView {
    title: String,
    columns: Vec<OfficeColumnView>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct OfficeColumnView {
    title: String,
    missing: Option<String>,
    lines: Vec<LineView>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct LineView {
    kind: LineKind,
    text: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum LineKind {
    Text,
    Marker,
    Rubric,
    Unresolved,
}
