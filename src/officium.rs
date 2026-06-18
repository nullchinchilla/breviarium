use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[component]
pub fn Officium(date: ReadSignal<String>, hour: ReadSignal<String>) -> Element {
    // The backend resolves one language per request, so we load each language
    // independently and zip the parallel block lists here for side-by-side
    // display. Block structure is language-independent, so the blocks line up by
    // position across languages.
    let latin_date = date.clone();
    let latin_hour = hour.clone();
    let latin = use_loader(move || load_office(latin_date(), latin_hour(), "la".to_string()))?;
    let english_date = date.clone();
    let english_hour = hour.clone();
    let english =
        use_loader(move || load_office(english_date(), english_hour(), "en2".to_string()))?;
    let latin = latin();
    let english = english();

    // Metadata (titles, navigation, diagnostics) is taken from the Latin
    // document; only the per-block line lists are zipped together.
    let office = &latin;
    let blocks = latin
        .blocks
        .iter()
        .enumerate()
        .map(|(index, latin_block)| {
            let empty: &[LineView] = &[];
            let english_lines = english
                .blocks
                .get(index)
                .map(|block| block.lines.as_slice())
                .unwrap_or(empty);
            let row_count = latin_block.lines.len().max(english_lines.len());
            let rows = (0..row_count)
                .map(|row| OfficeRowView {
                    cells: vec![
                        OfficeCellView {
                            lang: "la".to_string(),
                            line: latin_block.lines.get(row).cloned(),
                        },
                        OfficeCellView {
                            lang: "en".to_string(),
                            line: english_lines.get(row).cloned(),
                        },
                    ],
                })
                .collect();
            ZippedBlock {
                title: latin_block.title.clone(),
                class: latin_block.class.clone(),
                rows,
            }
        })
        .collect::<Vec<_>>();

    rsx! {
        document::Title { "{office.page_title}" }

        main { class: "container officium",
            header {
                nav {
                    ul {
                        li {  "Breviarium" }
                    }
                    ul {
                        li { Link { to: "{office.previous_date_path}", "Previous day" } }
                        li { Link { to: "{office.next_date_path}", "Next day" } }
                    }
                }
                h1 { class: "date", "{office.title}" }
                p {
                    "{office.date_label} · {office.hour_label}"
                }
                nav {
                    ul {
                        for link in &office.hour_links {
                            li {
                                Link {
                                    to: "{link.href}",
                                    aria_current: if link.current { "page" } else { "false" },
                                    "{link.label}"
                                }
                            }
                        }
                    }
                }
            }

            if !office.diagnostics.is_empty() {
                strong { "Diagnostics" } br{}
                ul {
                    for diagnostic in &office.diagnostics {
                        li { "{diagnostic}" }
                    }
                }
            }

            for block in blocks {
                section { class: "block {block.class}",
                    h2 { class: "block-title", "{block.title}" }
                    // Each row is one logical line (an antiphon, a whole psalm, a
                    // versicle…) with the languages interleaved side by side, so a
                    // single psalm — not a whole section — is the unit of a row.
                    for row in block.rows {
                        div { class: "row columns",
                            for cell in row.cells {
                                div { class: "lang lang-{cell.lang}",
                                    if let Some(line) = cell.line {
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
}

#[component]
fn OfficeLine(line: LineView) -> Element {
    match line.kind {
        // Each source line is parsed into inline segments: a leading versicle /
        // response / verse-number marker, and `+`/`++` crosses, each wrapped in
        // its own classed span so the stylesheet can present them.
        LineKind::Text => rsx! {
            p { class: "{line.class}",
                for (index , text_line) in line.text.lines().enumerate() {
                    if index > 0 {
                        br {}
                    }
                    for segment in parse_line(text_line) {
                        {render_segment(segment)}
                    }
                }
            }
        },
        LineKind::Marker => rsx! { p { class: "{line.class}",  "{line.text}" } },
        LineKind::Rubric => rsx! { p { class: "{line.class}",  "{line.text}" } },
        LineKind::Unresolved => rsx! { p { class: "{line.class}", mark { "{line.text}" } } },
    }
}

/// One inline segment of a rendered text line.
enum Segment {
    Text(String),
    /// Versicle marker `V.` → ℣.
    Versicle,
    /// Response marker `R.` / `R.br.` → ℟.
    Response,
    /// Antiphon marker `Ant.`.
    Antiphon,
    /// Verse number at the start of a psalm line, e.g. `39:2`.
    Verse(String),
    /// A single cross `+`.
    Cross,
    /// A double cross `++`.
    CrossDouble,
}

/// Splits a text line into [`Segment`]s: an optional leading versicle/response/
/// verse marker, then the body tokenized into text and cross markers.
fn parse_line(line: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let body = if let Some(rest) = line.strip_prefix("V. ") {
        out.push(Segment::Versicle);
        out.push(Segment::Text(" ".to_string()));
        rest
    } else if let Some(rest) = line.strip_prefix("R.br. ") {
        out.push(Segment::Response);
        out.push(Segment::Text(" ".to_string()));
        rest
    } else if let Some(rest) = line.strip_prefix("R. ") {
        out.push(Segment::Response);
        out.push(Segment::Text(" ".to_string()));
        rest
    } else if let Some(rest) = line.strip_prefix("Ant. ") {
        out.push(Segment::Antiphon);
        out.push(Segment::Text(" ".to_string()));
        rest
    } else if let Some((marker, rest)) = split_verse_marker(line) {
        out.push(Segment::Verse(marker.to_string()));
        out.push(Segment::Text(" ".to_string()));
        rest
    } else {
        line
    };
    tokenize_crosses(body, &mut out);
    out
}

/// A leading psalm verse number (`<digits>:<digits>[letter]`) followed by a
/// space or end of line, e.g. `39:2`, `1:1a`. Returns `(marker, rest)`.
fn split_verse_marker(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    i += 1;
    let after_colon = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == after_colon {
        return None;
    }
    if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    match bytes.get(i) {
        None => Some((line, "")),
        Some(b' ') => Some((&line[..i], line[i..].trim_start())),
        Some(_) => None,
    }
}

/// Tokenizes `text` into text runs and `+`/`++` cross segments.
fn tokenize_crosses(text: &str, out: &mut Vec<Segment>) {
    let mut buffer = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '+' {
            if !buffer.is_empty() {
                out.push(Segment::Text(std::mem::take(&mut buffer)));
            }
            if chars.peek() == Some(&'+') {
                chars.next();
                out.push(Segment::CrossDouble);
            } else {
                out.push(Segment::Cross);
            }
        } else {
            buffer.push(ch);
        }
    }
    if !buffer.is_empty() {
        out.push(Segment::Text(buffer));
    }
}

fn render_segment(segment: Segment) -> Element {
    match segment {
        Segment::Text(text) => rsx! { "{text}" },
        Segment::Versicle => rsx! { span { class: "versicle-mark", "℣" } },
        Segment::Response => rsx! { span { class: "response-mark", "℟" } },
        Segment::Antiphon => rsx! { span { class: "antiphon-mark", "Ant." } },
        Segment::Verse(marker) => rsx! { span { class: "verse-marker", "{marker}" } },
        Segment::Cross => rsx! { span { class: "cross", "✠" } },
        Segment::CrossDouble => rsx! { span { class: "cross cross-double", "+" } },
    }
}

#[server]
async fn load_office(
    date: String,
    hour: String,
    language: String,
) -> std::result::Result<OfficeView, ServerFnError> {
    crate::server::resolve_office_view(date, hour, language).map_err(ServerFnError::new)
}

/// A block with its languages zipped into side-by-side rows, ready to render.
/// Built client-side in [`Officium`] from the per-language [`OfficeBlockView`]s.
struct ZippedBlock {
    title: String,
    class: String,
    rows: Vec<OfficeRowView>,
}

/// One language's resolved Office, plus the navigation/title metadata that is
/// the same for every language. The [`Officium`] component loads one of these
/// per language and zips their `blocks` together for display.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct OfficeView {
    pub(crate) page_title: String,
    pub(crate) title: String,
    pub(crate) date_label: String,
    pub(crate) hour_label: String,
    pub(crate) profile_label: String,
    pub(crate) previous_date_path: String,
    pub(crate) next_date_path: String,
    pub(crate) hour_links: Vec<HourLinkView>,
    pub(crate) blocks: Vec<OfficeBlockView>,
    pub(crate) diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct HourLinkView {
    pub(crate) label: String,
    pub(crate) href: String,
    pub(crate) current: bool,
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

/// A single display row, with one cell per language, built client-side by
/// zipping the per-language [`OfficeBlockView::lines`].
#[derive(Clone, Debug, PartialEq)]
struct OfficeRowView {
    cells: Vec<OfficeCellView>,
}

#[derive(Clone, Debug, PartialEq)]
struct OfficeCellView {
    lang: String,
    /// The line for this language, or `None` if this language has fewer lines.
    line: Option<LineView>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct LineView {
    pub(crate) kind: LineKind,
    /// Semantic CSS class derived from the source node type (`versicle`,
    /// `response`, `antiphon`, `hymn`-bearing `text`, …).
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
