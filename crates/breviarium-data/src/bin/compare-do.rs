use breviarium_data::{Breviarium, DocumentNode, Hour, OfficeColumnContent, OfficeRequest};
use chrono::{Datelike, NaiveDate};
use std::collections::BTreeSet;
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if let Err(error) = run() {
        eprintln!("compare-do: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let date = args.next().ok_or_else(usage).and_then(|value| {
        NaiveDate::parse_from_str(&value, "%Y-%m-%d")
            .map_err(|error| format!("invalid date `{value}`: {error}"))
    })?;
    let hour = args
        .next()
        .ok_or_else(usage)
        .and_then(|value| parse_hour(&value))?;
    let do_root = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/divinum-officium-master"));

    let do_html = render_divinum_officium(date, hour, &do_root)?;
    let do_lines = normalize_lines(&html_to_text(&do_html));
    let rust_lines = normalize_lines(&render_rust(date, hour)?);

    let do_set = do_lines.iter().cloned().collect::<BTreeSet<_>>();
    let rust_set = rust_lines.iter().cloned().collect::<BTreeSet<_>>();
    let missing_from_rust = do_set
        .difference(&rust_set)
        .take(30)
        .cloned()
        .collect::<Vec<_>>();
    let extra_in_rust = rust_set
        .difference(&do_set)
        .take(30)
        .cloned()
        .collect::<Vec<_>>();
    let unresolved = rust_lines
        .iter()
        .filter(|line| line.contains("[unresolved "))
        .cloned()
        .collect::<Vec<_>>();
    let marker_leaks = rust_lines
        .iter()
        .filter(|line| leaks_source_marker(line))
        .cloned()
        .collect::<Vec<_>>();

    println!("date: {date}");
    println!("hour: {}", hour.as_str());
    println!("divinum officium lines: {}", do_lines.len());
    println!("rust lines: {}", rust_lines.len());
    println!(
        "shared unique lines: {}",
        do_set.intersection(&rust_set).count()
    );
    println!(
        "missing from rust unique lines: {}",
        do_set.difference(&rust_set).count()
    );
    println!(
        "extra in rust unique lines: {}",
        rust_set.difference(&do_set).count()
    );
    println!("rust unresolved lines: {}", unresolved.len());
    println!("rust marker leaks: {}", marker_leaks.len());
    print_examples("missing", &missing_from_rust);
    print_examples("extra", &extra_in_rust);
    print_examples("unresolved", &unresolved);
    print_examples("marker leak", &marker_leaks);

    if !unresolved.is_empty() || !marker_leaks.is_empty() {
        Err("rust output contains unresolved nodes or source marker leaks".to_string())
    } else {
        Ok(())
    }
}

fn usage() -> String {
    "usage: compare-do YYYY-MM-DD hour [/path/to/divinum-officium]".to_string()
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

fn divinum_command(hour: Hour) -> &'static str {
    match hour {
        Hour::Matins => "prayMatutinum",
        Hour::Lauds => "prayLaudes",
        Hour::Prime => "prayPrima",
        Hour::Terce => "prayTertia",
        Hour::Sext => "praySexta",
        Hour::None => "prayNona",
        Hour::Vespers => "prayVespera",
        Hour::Compline => "prayCompletorium",
        _ => "prayLaudes",
    }
}

fn render_divinum_officium(date: NaiveDate, hour: Hour, root: &PathBuf) -> Result<String, String> {
    let script = root.join("web/cgi-bin/horas/officium.pl");
    let date = format!("{:02}-{:02}-{}", date.month(), date.day(), date.year());
    let query = format!(
        "date1={date}&date={date}&command={}&version=Rubrics%201960&lang1=Latin&lang2=English&testmode=regular&content=1",
        divinum_command(hour)
    );
    let output = Command::new("perl")
        .arg(&script)
        .env("REQUEST_METHOD", "GET")
        .env("QUERY_STRING", query)
        .output()
        .map_err(|error| format!("failed to run `{}`: {error}", script.display()))?;
    if !output.status.success() {
        return Err(format!(
            "DO exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn render_rust(date: NaiveDate, hour: Hour) -> Result<String, String> {
    let mut request = OfficeRequest::new(date, hour);
    request.languages = vec!["la".to_string(), "en".to_string()];
    let office = Breviarium::embedded()
        .map_err(|error| error.to_string())?
        .resolve_office(request)
        .map_err(|error| error.to_string())?;
    let mut lines = Vec::new();
    for block in &office.blocks {
        for column in &block.columns {
            match &column.content {
                OfficeColumnContent::Resolved { nodes } => {
                    lines.extend(document_lines(&column.language, nodes));
                }
                OfficeColumnContent::Missing { reason } => {
                    lines.push(format!("[missing {}: {reason}]", column.language));
                }
                _ => {
                    lines.push(format!("[unknown {} content]", column.language));
                }
            }
        }
    }
    Ok(lines.join("\n"))
}

fn document_lines(language: &str, nodes: &[DocumentNode]) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|node| {
            node.plain_text_for_language(language)
                .lines()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn html_to_text(html: &str) -> String {
    let mut output = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        if in_tag {
            if ch == '>' {
                let tag_name = tag
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if matches!(
                    tag_name.as_str(),
                    "br" | "p" | "tr" | "td" | "h1" | "h2" | "h3"
                ) {
                    output.push('\n');
                }
                tag.clear();
                in_tag = false;
            } else {
                tag.push(ch);
            }
        } else if ch == '<' {
            in_tag = true;
        } else {
            output.push(ch);
        }
    }
    decode_entities(&output)
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#8213;", "-")
        .replace("&#8217;", "'")
        .replace("&#8220;", "\"")
        .replace("&#8221;", "\"")
}

fn normalize_lines(input: &str) -> Vec<String> {
    input
        .lines()
        .filter_map(|line| {
            let line = line
                .replace("℣.", "V.")
                .replace("℟.", "R.")
                .replace('\u{00a0}', " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let line = line.trim_matches('|').trim().to_string();
            if line.is_empty()
                || line == "Content-type: text/html; charset=utf-8"
                || line == "Top"
                || line == "Next"
                || line.chars().all(|ch| ch.is_ascii_digit())
            {
                None
            } else {
                Some(line)
            }
        })
        .collect()
}

fn leaks_source_marker(line: &str) -> bool {
    line.contains("{:")
        || line.contains(":}")
        || line.contains("(rubrica")
        || line.contains("$rubrica")
        || line.contains("$")
        || line.contains("<FONT")
        || line.contains("unexpanded")
}

fn print_examples(label: &str, values: &[String]) {
    for value in values.iter().take(10) {
        println!("{label}: {value}");
    }
    if values.len() > 10 {
        println!("{label}: ... {} more", values.len() - 10);
    }
}
