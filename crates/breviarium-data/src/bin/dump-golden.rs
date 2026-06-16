//! Deterministic full-year dump of resolved offices, used as the golden-diff
//! baseline for the resolver redesign. Loads the engine once and renders every
//! date/hour/language in a stable textual form.
//!
//! Usage: dump-golden [YEAR] [lang ...]   (defaults: 2026, la en)

use breviarium_data::{Breviarium, DocumentNode, Hour, OfficeColumnContent, OfficeRequest};
use chrono::{Datelike, Duration, NaiveDate};
use std::env;
use std::fmt::Write as _;

const HOURS: [Hour; 8] = [
    Hour::Matins,
    Hour::Lauds,
    Hour::Prime,
    Hour::Terce,
    Hour::Sext,
    Hour::None,
    Hour::Vespers,
    Hour::Compline,
];

fn main() {
    if let Err(error) = run() {
        eprintln!("dump-golden: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let year = args
        .next()
        .unwrap_or_else(|| "2026".to_string())
        .parse::<i32>()
        .map_err(|error| format!("invalid year: {error}"))?;
    let languages = {
        let values = args.collect::<Vec<_>>();
        if values.is_empty() {
            vec!["la".to_string(), "en".to_string()]
        } else {
            values
        }
    };

    let engine = Breviarium::embedded().map_err(|error| error.to_string())?;
    let mut date =
        NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(|| format!("invalid year `{year}`"))?;
    let mut out = String::new();

    while date.year() == year {
        for hour in HOURS {
            let mut request = OfficeRequest::new(date, hour);
            request.languages = languages.clone();
            write!(out, "{}", render(&engine, request, date, hour)).expect("write to string");
        }
        date += Duration::days(1);
    }

    print!("{out}");
    Ok(())
}

fn render(engine: &Breviarium, request: OfficeRequest, date: NaiveDate, hour: Hour) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "======== {date} {} ========", hour.as_str());
    let office = match engine.resolve_office(request) {
        Ok(office) => office,
        Err(error) => {
            let _ = writeln!(out, "ERROR: {error}");
            return out;
        }
    };

    let _ = writeln!(
        out,
        "profile={} principal={} rank={:?} temporal={:?} sanctoral={:?} commemorations={:?}",
        office.profile,
        office.principal.id,
        office.principal.rank,
        office.temporal.as_ref().map(|o| &o.id),
        office.sanctoral.as_ref().map(|o| &o.id),
        office
            .commemorations
            .iter()
            .map(|o| o.id.clone())
            .collect::<Vec<_>>(),
    );
    for diagnostic in &office.diagnostics {
        let _ = writeln!(
            out,
            "diagnostic {}: {}",
            diagnostic.code, diagnostic.message
        );
    }
    for block in &office.blocks {
        let _ = writeln!(out, "## {} [{:?}]", block.id, block.role);
        for column in &block.columns {
            let _ = writeln!(
                out,
                "### {}{}",
                column.language,
                column
                    .title
                    .as_ref()
                    .map(|title| format!(" - {title}"))
                    .unwrap_or_default()
            );
            match &column.content {
                OfficeColumnContent::Resolved { nodes } => {
                    let _ = writeln!(out, "{}", document_text(&column.language, nodes));
                }
                OfficeColumnContent::Missing { reason } => {
                    let _ = writeln!(out, "[missing: {reason}]");
                }
                _ => {
                    let _ = writeln!(out, "[unknown column content]");
                }
            }
        }
    }
    out
}

fn document_text(language: &str, nodes: &[DocumentNode]) -> String {
    nodes
        .iter()
        .map(|node| node.plain_text_for_language(language))
        .collect::<Vec<_>>()
        .join("\n")
}
