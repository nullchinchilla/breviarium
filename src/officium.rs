use dioxus::prelude::*;

use crate::load_office::{load_office, LineKind, LineView};

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
    let date_path = date();
    let hour_path = canonical_hour_path(&hour());
    let page_title = format!("{} - {}", office.title, display_hour(&hour_path));
    let date_label = date_label(&date_path);
    let (previous_date_path, next_date_path) = adjacent_date_paths(&date_path, &hour_path);
    let hour_links = hour_links(&date_path, &hour_path);
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
        document::Title { "{page_title}" }

        main { class: "container officium",
            OfficiumHeader {
                title: office.title.clone(),
                date_label,
                hour_label: display_hour(&hour_path).to_string(),
                previous_date_path,
                next_date_path,
                hour_links,
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
fn OfficiumHeader(
    title: String,
    date_label: String,
    hour_label: String,
    previous_date_path: String,
    next_date_path: String,
    hour_links: Vec<HourLinkView>,
) -> Element {
    rsx! {
        header {
            nav {
                ul {
                    li {  "Breviarium" }
                }
                ul {
                    li { Link { to: "{previous_date_path}", "←" } }
                    li {{date_label}}
                    li { Link { to: "{next_date_path}", "→" } }
                }
            }
            h1 { class: "date", "{title}" }
            div {
                class: "hour-links",
                for link in &hour_links {
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

#[derive(Clone, Debug, PartialEq)]
struct HourLinkView {
    label: String,
    href: String,
    current: bool,
}

fn canonical_hour_path(hour: &str) -> String {
    match hour.to_ascii_lowercase().as_str() {
        "matins" | "matutinum" => "matutinum",
        "lauds" | "laudes" => "laudes",
        "prime" | "prima" => "prima",
        "terce" | "tertia" => "tertia",
        "sext" | "sexta" => "sexta",
        "none" | "nona" => "nona",
        "vespers" | "vespera" | "vesperae" => "vesperae",
        "compline" | "completorium" => "completorium",
        _ => hour,
    }
    .to_string()
}

fn display_hour(hour: &str) -> &'static str {
    match hour {
        "matutinum" => "Matutinum",
        "laudes" => "Laudes",
        "prima" => "Prima",
        "tertia" => "Tertia",
        "sexta" => "Sexta",
        "nona" => "Nona",
        "vesperae" => "Vesperae",
        "completorium" => "Completorium",
        _ => "Officium",
    }
}

fn date_label(date_path: &str) -> String {
    if date_path.len() == 8 {
        format!(
            "{}-{}-{}",
            &date_path[0..4],
            &date_path[4..6],
            &date_path[6..8]
        )
    } else {
        date_path.to_string()
    }
}

fn adjacent_date_paths(date_path: &str, hour_path: &str) -> (String, String) {
    let Some((year, month, day)) = parse_date_path(date_path) else {
        return (
            format!("/officium/{date_path}/{hour_path}"),
            format!("/officium/{date_path}/{hour_path}"),
        );
    };
    let previous = format_date_path(add_days(year, month, day, -1));
    let next = format_date_path(add_days(year, month, day, 1));
    (
        format!("/officium/{previous}/{hour_path}"),
        format!("/officium/{next}/{hour_path}"),
    )
}

fn parse_date_path(date_path: &str) -> Option<(i32, u8, u8)> {
    if date_path.len() != 8 || !date_path.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year = date_path[0..4].parse().ok()?;
    let month = date_path[4..6].parse().ok()?;
    let day = date_path[6..8].parse().ok()?;
    (1..=12)
        .contains(&month)
        .then_some((year, month, day))
        .filter(|(year, month, day)| *day >= 1 && *day <= days_in_month(*year, *month))
}

fn add_days(mut year: i32, mut month: u8, mut day: u8, days: i8) -> (i32, u8, u8) {
    if days < 0 {
        day -= 1;
        if day == 0 {
            if month == 1 {
                year -= 1;
                month = 12;
            } else {
                month -= 1;
            }
            day = days_in_month(year, month);
        }
    } else if days > 0 {
        day += 1;
        if day > days_in_month(year, month) {
            day = 1;
            if month == 12 {
                year += 1;
                month = 1;
            } else {
                month += 1;
            }
        }
    }
    (year, month, day)
}

fn format_date_path((year, month, day): (i32, u8, u8)) -> String {
    format!("{year:04}{month:02}{day:02}")
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn hour_links(date_path: &str, current_hour: &str) -> Vec<HourLinkView> {
    [
        ("matutinum", "Matutinum"),
        ("laudes", "Laudes"),
        ("prima", "Prima"),
        ("tertia", "Tertia"),
        ("sexta", "Sexta"),
        ("nona", "Nona"),
        ("vesperae", "Vesperae"),
        ("completorium", "Completorium"),
    ]
    .into_iter()
    .map(|(hour, label)| HourLinkView {
        label: label.to_string(),
        href: format!("/officium/{date_path}/{hour}"),
        current: hour == current_hour,
    })
    .collect()
}

#[component]
fn OfficeLine(line: LineView) -> Element {
    match line.kind {
        // Each source line is parsed into inline segments: a leading versicle /
        // response / verse-number marker, and cross markers, each wrapped in
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
#[derive(Debug, PartialEq, Eq)]
enum Segment {
    Text(String),
    /// Versicle marker `V.` → ℣.
    Versicle,
    /// Response marker `R.` / `R.br.` → ℟.
    Response,
    /// An inline label rendered emphasized, e.g. `Ant.`, `Benedictio.`. The
    /// string holds the marker text verbatim.
    InlineMark(String),
    /// Verse number at the start of a psalm line, e.g. `39:2`.
    Verse(String),
    /// The ordinary large sign of the cross marker `+`.
    LargeSignOfCross,
    /// The small/lesser sign of the cross marker `++`.
    LesserSignOfCross,
    /// The breast sign of the cross marker `+++`.
    BreastSignOfCross,
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
    } else if let Some((mark, rest)) = split_inline_mark(line) {
        out.push(Segment::InlineMark(mark.to_string()));
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

/// Leading inline-label markers (antiphon/blessing) recognized at the start of
/// a line. Each is matched with its trailing space.
const INLINE_MARKS: &[&str] = &["Ant.", "Benedictio.", "Benediction."];

/// Splits off a leading inline-label marker (`Ant.`, `Benedictio.`, …),
/// returning `(marker, rest)` with the marker text and the remaining body.
fn split_inline_mark(line: &str) -> Option<(&str, &str)> {
    INLINE_MARKS
        .iter()
        .find_map(|mark| line.strip_prefix(mark)?.strip_prefix(' ').map(|rest| (*mark, rest)))
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

/// Tokenizes `text` into text runs and `+`/`++`/`+++` cross segments.
fn tokenize_crosses(text: &str, out: &mut Vec<Segment>) {
    let mut rest = text;

    while let Some(index) = rest.find('+') {
        let (before, after) = rest.split_at(index);
        if !before.is_empty() {
            out.push(Segment::Text(before.to_string()));
        }

        if let Some(next) = after.strip_prefix("+++") {
            out.push(Segment::BreastSignOfCross);
            rest = next;
        } else if let Some(next) = after.strip_prefix("++") {
            out.push(Segment::LesserSignOfCross);
            rest = next;
        } else if let Some(next) = after.strip_prefix('+') {
            out.push(Segment::LargeSignOfCross);
            rest = next;
        }
    }

    if !rest.is_empty() {
        out.push(Segment::Text(rest.to_string()));
    }
}

fn render_segment(segment: Segment) -> Element {
    match segment {
        Segment::Text(text) => rsx! { "{text}" },
        Segment::Versicle => rsx! { span { class: "versicle-mark", "℣" } },
        Segment::Response => rsx! { span { class: "response-mark", "℟" } },
        Segment::InlineMark(mark) => rsx! { span { class: "inline-mark", "{mark}" } },
        Segment::Verse(marker) => rsx! { span { class: "verse-marker", "{marker}" } },
        Segment::LargeSignOfCross => rsx! { span { class: "cross cross-large", "✠" } },
        Segment::LesserSignOfCross => rsx! { span { class: "cross cross-lesser", "☩" } },
        Segment::BreastSignOfCross => rsx! { span { class: "cross cross-breast", "✙" } },
    }
}

#[cfg(test)]
mod tests {
    use super::{tokenize_crosses, Segment};

    #[test]
    fn tokenize_crosses_uses_longest_cross_marker_first() {
        let mut segments = Vec::new();
        tokenize_crosses("a+b++c+++d++++e", &mut segments);

        assert_eq!(
            segments,
            vec![
                Segment::Text("a".to_string()),
                Segment::LargeSignOfCross,
                Segment::Text("b".to_string()),
                Segment::LesserSignOfCross,
                Segment::Text("c".to_string()),
                Segment::BreastSignOfCross,
                Segment::Text("d".to_string()),
                Segment::BreastSignOfCross,
                Segment::LargeSignOfCross,
                Segment::Text("e".to_string()),
            ]
        );
    }
}

/// A block with its languages zipped into side-by-side rows, ready to render.
/// Built client-side in [`Officium`] from the per-language [`OfficeBlockView`]s.
struct ZippedBlock {
    title: String,
    class: String,
    rows: Vec<OfficeRowView>,
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
