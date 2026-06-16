use breviarium_data::{Breviarium, DocumentNode, Hour, OfficeColumnContent, OfficeRequest};
use chrono::{Datelike, Duration, NaiveDate};
use std::env;

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
        eprintln!("check-office: {error}");
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
    let mut resolved = 0usize;
    let mut hard_failures = Vec::new();
    let mut missing_columns = Vec::new();
    let mut unresolved_nodes = Vec::new();
    let mut diagnostics = Vec::new();

    while date.year() == year {
        for hour in HOURS {
            let mut request = OfficeRequest::new(date, hour);
            request.languages = languages.clone();
            match engine.resolve_office(request) {
                Ok(office) => {
                    resolved += 1;
                    for diagnostic in &office.diagnostics {
                        diagnostics.push(format!(
                            "{} {} {}: {}",
                            date,
                            hour.as_str(),
                            diagnostic.code,
                            diagnostic.message
                        ));
                    }
                    for block in &office.blocks {
                        for column in &block.columns {
                            match &column.content {
                                OfficeColumnContent::Resolved { nodes } => {
                                    for node in nodes {
                                        if let DocumentNode::Unresolved {
                                            kind,
                                            value,
                                            reason,
                                        } = node
                                        {
                                            unresolved_nodes.push(format!(
                                                "{} {} {} {} {}: {}",
                                                date,
                                                hour.as_str(),
                                                column.language,
                                                kind,
                                                value,
                                                reason
                                            ));
                                        }
                                    }
                                }
                                OfficeColumnContent::Missing { reason } => {
                                    missing_columns.push(format!(
                                        "{} {} {} {}: {}",
                                        date,
                                        hour.as_str(),
                                        block.id,
                                        column.language,
                                        reason
                                    ));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Err(error) => hard_failures.push(format!("{date} {}: {error}", hour.as_str())),
            }
        }
        date += Duration::days(1);
    }

    println!("year: {year}");
    println!("languages: {}", languages.join(","));
    println!("resolved requests: {resolved}");
    println!("hard failures: {}", hard_failures.len());
    println!("missing columns: {}", missing_columns.len());
    println!("unresolved nodes: {}", unresolved_nodes.len());
    println!("diagnostics: {}", diagnostics.len());
    print_examples("hard failure", &hard_failures);
    print_examples("missing column", &missing_columns);
    print_examples("unresolved node", &unresolved_nodes);
    print_examples("diagnostic", &diagnostics);

    if hard_failures.is_empty()
        && missing_columns.is_empty()
        && unresolved_nodes.is_empty()
        && diagnostics.is_empty()
    {
        Ok(())
    } else {
        Err(
            "office sweep found failures, missing columns, unresolved nodes, or diagnostics"
                .to_string(),
        )
    }
}

fn print_examples(label: &str, values: &[String]) {
    for value in values.iter().take(20) {
        println!("{label}: {value}");
    }
    if values.len() > 20 {
        println!("{label}: ... {} more", values.len() - 20);
    }
}
