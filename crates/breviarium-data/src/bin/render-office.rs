use breviarium_data::{Breviarium, DocumentNode, Hour, OfficeBlockContent, OfficeRequest};
use chrono::NaiveDate;
use std::env;

fn main() {
    if let Err(error) = run() {
        eprintln!("render-office: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let date = args
        .next()
        .ok_or_else(|| "usage: render-office YYYY-MM-DD hour [lang ...]".to_string())
        .and_then(|value| {
            NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                .map_err(|error| format!("invalid date `{value}`: {error}"))
        })?;
    let hour = args
        .next()
        .ok_or_else(|| "usage: render-office YYYY-MM-DD hour [lang ...]".to_string())
        .and_then(|value| parse_hour(&value))?;
    let languages = {
        let values = args.collect::<Vec<_>>();
        if values.is_empty() {
            vec!["la".to_string(), "en".to_string()]
        } else {
            values
        }
    };

    let engine = Breviarium::embedded().map_err(|error| error.to_string())?;

    // The backend resolves one language per request; render each requested
    // language in turn (clients display columns by zipping these documents).
    for language in &languages {
        let request = OfficeRequest::new(date, hour).with_language(language.clone());
        let office = engine
            .resolve_office(request)
            .map_err(|error| error.to_string())?;

        println!(
            "\n# {} {} [{language}] profile={} principal={} rank={:?} temporal={:?} sanctoral={:?}",
            office.date_facts.date,
            office.hour.as_str(),
            office.profile,
            office.principal.id,
            office.principal.rank,
            office.temporal.as_ref().map(|observance| &observance.id),
            office.sanctoral.as_ref().map(|observance| &observance.id)
        );
        for diagnostic in &office.diagnostics {
            println!("diagnostic {}: {}", diagnostic.code, diagnostic.message);
        }
        for block in &office.blocks {
            println!(
                "\n## {} [{:?}]{}",
                block.id,
                block.role,
                block
                    .title
                    .as_ref()
                    .map(|title| format!(" - {title}"))
                    .unwrap_or_default()
            );
            match &block.content {
                OfficeBlockContent::Resolved { nodes } => {
                    println!("{}", document_text(language, nodes));
                }
                OfficeBlockContent::Missing { reason } => {
                    println!("[missing: {reason}]");
                }
                _ => {
                    println!("[unknown block content]");
                }
            }
        }
    }

    Ok(())
}

fn parse_hour(value: &str) -> Result<Hour, String> {
    match value.to_ascii_lowercase().as_str() {
        "matins" | "matutinum" => Ok(Hour::Matins),
        "lauds" | "laudes" => Ok(Hour::Lauds),
        "prime" | "prima" => Ok(Hour::Prime),
        "terce" | "tertia" => Ok(Hour::Terce),
        "sext" | "sexta" => Ok(Hour::Sext),
        "none" | "nona" => Ok(Hour::None),
        "vespers" | "vespera" | "vesperae" => Ok(Hour::Vespers),
        "compline" | "completorium" => Ok(Hour::Compline),
        _ => Err(format!("unknown hour `{value}`")),
    }
}

fn document_text(language: &str, nodes: &[DocumentNode]) -> String {
    nodes
        .iter()
        .map(|node| {
            let text = node.plain_text_for_language(language);
            format!("[{}] {}", node.kind(), text.replace('\n', "\n        | "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
