use dioxus::prelude::*;

// Imported by item rather than by module: `lang` is also a prop name here.
use crate::lang::{is_supported_code, remember, Language, LATIN};
use crate::language_picker::LanguagePicker;
use crate::load_office::{load_office, LineKind, LineView, MarkerView};
use crate::Route;

pub(crate) mod hour;
use hour::Hour;

#[component]
pub fn Officium(
    lang: ReadSignal<String>,
    date: ReadSignal<String>,
    hour: ReadSignal<String>,
) -> Element {
    // An unknown segment would otherwise resolve to a document with every
    // vernacular slot empty, which reads as a data problem rather than a bad
    // URL. Reject it up front, and report it as one.
    if !is_supported_code(&lang()) {
        return rsx! { UnknownLanguage { code: lang() } };
    }

    // The backend resolves one language per request, so we load Latin and the
    // requested vernacular (from the `lang` route segment) independently and zip
    // the parallel block lists here for side-by-side display. Block structure is
    // language-independent, so the blocks line up by position across languages.
    let latin = use_loader(move || load_office(date(), hour(), "la".to_string()))?;
    let vernacular = use_loader(move || load_office(date(), hour(), lang()))?;
    let latin = latin();
    let vernacular = vernacular();

    // Metadata (title, navigation, diagnostics) is taken from the Latin
    // document; only the per-block line lists are zipped together. Block
    // structure is language-independent, so blocks line up by position: the
    // Latin block at index `i` pairs with the vernacular lines at the same index.
    let empty: &[LineView] = &[];
    let blocks = latin
        .blocks
        .iter()
        .enumerate()
        .map(|(index, latin_block)| {
            let vernacular_lines = vernacular
                .blocks
                .get(index)
                .map_or(empty, |block| block.lines.as_slice());
            (latin_block, vernacular_lines)
        })
        .collect::<Vec<_>>();
    let page_title = format!(
        "{} - {}",
        latin.title,
        Hour::from_path(&hour()).map_or("Officium", Hour::label)
    );
    let vernacular_lang = lang();

    rsx! {
        document::Title { "{page_title}" }

        main { class: "container officium",
            OfficiumHeader { title: latin.title.clone(), lang, date, hour }

            if !latin.diagnostics.is_empty() {
                strong { "Diagnostics" } br{}
                ul {
                    for diagnostic in &latin.diagnostics {
                        li { "{diagnostic}" }
                    }
                }
            }

            for (latin_block, vernacular_lines) in blocks {
                section { class: "block {latin_block.class}",
                    h2 { class: "block-title", "{latin_block.title}" }
                    // Each row is one logical line (an antiphon, a whole psalm, a
                    // versicle…) with the languages interleaved side by side, so a
                    // single psalm — not a whole section — is the unit of a row.
                    for row in 0..latin_block.lines.len().max(vernacular_lines.len()) {
                        div { class: "row columns",
                            div { class: "lang lang-la",
                                if let Some(line) = latin_block.lines.get(row) {
                                    OfficeLine { line: line.clone() }
                                }
                            }
                            // `/la/...` is reachable by hand but never linked;
                            // showing it would just repeat the left column.
                            if vernacular_lang != LATIN {
                                div { class: "lang lang-{vernacular_lang}",
                                    if let Some(line) = vernacular_lines.get(row) {
                                        OfficeLine { line: line.clone() }
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
    lang: ReadSignal<String>,
    date: ReadSignal<String>,
    hour: ReadSignal<String>,
) -> Element {
    let current = Hour::from_path(&hour());
    let date_path = date();
    let (previous_date, next_date) = adjacent_dates(&date_path);
    // Keep the current hour across date navigation, normalizing aliases to the
    // canonical path; an unrecognized hour is carried through unchanged.
    let hour_path = match current {
        Some(hour) => hour.path().to_string(),
        None => hour(),
    };
    rsx! {
        header {
            nav {
                ul {
                    li {
                        Link { to: Route::Home {}, "Breviarium" }
                    }
                }
                div { class: "date-controls",
                    ul { class: "date-navigation",
                        li {
                            Link {
                                to: Route::Officium { lang: lang(), date: previous_date, hour: hour_path.clone() },
                                "←"
                            }
                        }
                        li { {date_label(&date_path)} }
                        li {
                            Link {
                                to: Route::Officium { lang: lang(), date: next_date, hour: hour_path.clone() },
                                "→"
                            }
                        }
                    }
                    LanguagePicker {
                        id: "office-language-picker",
                        language: lang(),
                        office: (date(), hour_path),
                        onselect: move |choice| remember(choice),
                    }
                }
            }
            h1 { class: "date", "{title}" }
            div {
                class: "hour-links",
                for hour in Hour::ALL {
                    Link {
                        to: Route::Officium { lang: lang(), date: date(), hour: hour.path().to_string() },
                        aria_current: if current == Some(hour) { "page" } else { "false" },
                        "{hour.label()}"
                    }
                }
            }
        }
    }
}

/// Shown for a `:lang` segment the resolver has no column for. Rendering the
/// Office anyway would produce a page of empty vernacular slots, which reads as
/// missing data rather than a mistyped URL.
#[component]
fn UnknownLanguage(code: String) -> Element {
    // The page still renders, but the response should say what it is: a bad
    // URL, not an Office. No-op on the client, where there is no response.
    use_hook(|| {
        dioxus::fullstack::FullstackContext::commit_http_status(
            StatusCode::NOT_FOUND,
            Some("unknown language".to_string()),
        );
    });

    rsx! {
        document::Title { "Unknown language" }
        main { class: "container",
            article {
                header { h1 { "Unknown language" } }
                p { "There is no translation column for `{code}`." }
                ul {
                    for choice in Language::ALL {
                        li { "{choice.label()} — {choice.code()}" }
                    }
                }
                p {
                    Link { to: Route::Home {}, "Go to the home page" }
                }
            }
        }
    }
}

/// Parses a `YYYYMMDD` path segment into a calendar date, rejecting anything
/// that isn't exactly eight digits naming a real date.
fn parse_date(date_path: &str) -> Option<chrono::NaiveDate> {
    if date_path.len() != 8 || !date_path.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year = date_path[0..4].parse().ok()?;
    let month = date_path[4..6].parse().ok()?;
    let day = date_path[6..8].parse().ok()?;
    chrono::NaiveDate::from_ymd_opt(year, month, day)
}

/// Formats a date back into the `YYYYMMDD` path segment.
fn format_date(date: chrono::NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

/// A human-readable `YYYY-MM-DD` label, falling back to the raw segment when it
/// isn't a parseable date.
fn date_label(date_path: &str) -> String {
    parse_date(date_path).map_or_else(
        || date_path.to_string(),
        |date| date.format("%Y-%m-%d").to_string(),
    )
}

/// The `(previous, next)` day path segments around `date_path`, leaving the
/// segment unchanged for either neighbor that isn't representable.
fn adjacent_dates(date_path: &str) -> (String, String) {
    let Some(date) = parse_date(date_path) else {
        return (date_path.to_string(), date_path.to_string());
    };
    let fallback = || date_path.to_string();
    (
        date.pred_opt().map_or_else(fallback, format_date),
        date.succ_opt().map_or_else(fallback, format_date),
    )
}

#[component]
fn OfficeLine(line: LineView) -> Element {
    match line.kind {
        // Each source line is parsed into inline segments: a leading versicle /
        // response / verse-number marker, and cross markers, each wrapped in
        // its own classed span so the stylesheet can present them.
        LineKind::Text => rsx! {
            p { class: "{line.class}",
                // The leading marker arrives as structured data; the renderer
                // picks the ℣/℟ glyph by role, or shows the localized label.
                if let Some(marker) = &line.marker {
                    {render_marker(marker)}
                    " "
                }
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
    /// Verse number at the start of a psalm line, e.g. `39:2`.
    Verse(String),
    /// The ordinary large sign of the cross marker `+`.
    LargeSignOfCross,
    /// The small/lesser sign of the cross marker `++`.
    LesserSignOfCross,
    /// The breast sign of the cross marker `+++`.
    BreastSignOfCross,
}

/// Splits a text line into [`Segment`]s: an optional leading verse number, then
/// the body tokenized into text and cross markers. The versicle/response/
/// antiphon/blessing marker is no longer parsed out here — it arrives as
/// structured data on the line (see [`render_marker`]) — so this only handles
/// markers that are genuinely inline in the source text (verse numbers, crosses).
fn parse_line(line: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let body = if let Some((marker, rest)) = split_verse_marker(line) {
        out.push(Segment::Verse(marker.to_string()));
        // A leading verse number is separated from the body by a single space.
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

/// Renders the structured leading marker: the versicle/response sigla as the
/// ℣/℟ glyphs (by role), and antiphon/blessing as their localized label text.
fn render_marker(marker: &MarkerView) -> Element {
    match marker.kind.as_str() {
        "versicle" => rsx! { span { class: "versicle-mark", "℣" } },
        "response" | "short-response" => rsx! { span { class: "response-mark", "℟" } },
        _ => rsx! { span { class: "inline-mark", "{marker.label}" } },
    }
}

fn render_segment(segment: Segment) -> Element {
    match segment {
        Segment::Text(text) => rsx! { "{text}" },
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
