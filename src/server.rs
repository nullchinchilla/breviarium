use chrono::{Local, NaiveDateTime, Timelike};
use dioxus::prelude::{DioxusRouterExt, ServeConfig};
use dioxus::server::axum::{
    http::{header::LOCATION, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::{
    fs,
    net::{IpAddr, SocketAddr},
    process::Command,
};

pub fn launch() -> ! {
    dioxus::serve(|| async {
        Ok(Router::new()
            .route("/", get(home_redirect))
            .serve_dioxus_application(ServeConfig::new(), crate::App))
    })
}

async fn home_redirect(headers: HeaderMap) -> Response {
    let now = approximate_client_now(&headers).unwrap_or_else(|| Local::now().naive_local());
    let target = format!(
        "/officium/{}/{}",
        now.date().format("%Y%m%d"),
        hour_for_clock(now.hour())
    );
    let mut response = ().into_response();
    *response.status_mut() = StatusCode::FOUND;
    response.headers_mut().insert(
        LOCATION,
        HeaderValue::from_str(&target).expect("internal redirect path is a valid header value"),
    );
    response
}

fn approximate_client_now(headers: &HeaderMap) -> Option<NaiveDateTime> {
    let ip = client_ip(headers)?;
    let country = geoip_country(ip)?;
    let timezone = timezone_for_country(&country)?;
    date_in_timezone(&timezone)
}

fn client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    for name in ["cf-connecting-ip", "x-real-ip", "x-forwarded-for"] {
        let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) else {
            continue;
        };
        for token in value.split(',') {
            if let Some(ip) = parse_ip_token(token) {
                return Some(ip);
            }
        }
    }

    let forwarded = headers.get("forwarded")?.to_str().ok()?;
    for entry in forwarded.split(',') {
        for part in entry.split(';') {
            let Some(value) = part.trim().strip_prefix("for=") else {
                continue;
            };
            if let Some(ip) = parse_ip_token(value) {
                return Some(ip);
            }
        }
    }
    None
}

fn parse_ip_token(token: &str) -> Option<IpAddr> {
    let token = token.trim().trim_matches('"');
    if let Ok(ip) = token.parse::<IpAddr>() {
        return Some(ip);
    }
    if let Ok(socket) = token.parse::<SocketAddr>() {
        return Some(socket.ip());
    }
    if let Some(stripped) = token
        .strip_prefix('[')
        .and_then(|value| value.split(']').next())
    {
        return stripped.parse::<IpAddr>().ok();
    }
    if let Some((host, _port)) = token.rsplit_once(':') {
        if host.contains('.') {
            return host.parse::<IpAddr>().ok();
        }
    }
    None
}

fn geoip_country(ip: IpAddr) -> Option<String> {
    let binary = if ip.is_ipv6() {
        "geoiplookup6"
    } else {
        "geoiplookup"
    };
    let output = Command::new(binary).arg(ip.to_string()).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_geoip_country(&String::from_utf8_lossy(&output.stdout))
}

fn parse_geoip_country(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, detail) = line.split_once(':')?;
        let code = detail.trim().split(',').next()?.trim();
        if code.len() == 2 && code.chars().all(|ch| ch.is_ascii_uppercase()) {
            Some(code.to_string())
        } else {
            None
        }
    })
}

fn timezone_for_country(country: &str) -> Option<String> {
    timezone_for_country_in("/usr/share/zoneinfo/zone1970.tab", country)
        .or_else(|| timezone_for_country_in("/usr/share/zoneinfo/zone.tab", country))
}

fn timezone_for_country_in(path: &str, country: &str) -> Option<String> {
    let table = fs::read_to_string(path).ok()?;
    table.lines().find_map(|line| {
        if line.starts_with('#') || line.trim().is_empty() {
            return None;
        }
        let mut fields = line.split('\t');
        let countries = fields.next()?;
        let _coordinates = fields.next()?;
        let timezone = fields.next()?;
        countries
            .split(',')
            .any(|candidate| candidate == country)
            .then(|| timezone.to_string())
    })
}

fn date_in_timezone(timezone: &str) -> Option<NaiveDateTime> {
    let output = Command::new("date")
        .env("TZ", timezone)
        .arg("+%Y%m%d%H%M")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    NaiveDateTime::parse_from_str(text.trim(), "%Y%m%d%H%M").ok()
}

fn hour_for_clock(hour: u32) -> &'static str {
    match hour {
        0..=4 => "matutinum",
        5..=7 => "laudes",
        8..=9 => "prima",
        10..=11 => "tertia",
        12..=14 => "sexta",
        15..=16 => "nona",
        17..=20 => "vesperae",
        _ => "completorium",
    }
}

use crate::officium::{HourLinkView, LineKind, LineView, OfficeBlockView, OfficeView};

/// Resolves a single language's Office into the view consumed by the
/// `Officium` component. The component calls this once per language (via the
/// `load_office` server function) and zips the results for side-by-side
/// display, so this makes exactly one backend call.
pub(crate) fn resolve_office_view(
    date: String,
    hour: String,
    language: String,
) -> Result<OfficeView, String> {
    use breviarium_data::{Breviarium, OfficeBlockContent, OfficeRequest};
    use chrono::{Duration, NaiveDate};

    let parsed_date = NaiveDate::parse_from_str(&date, "%Y%m%d")
        .map_err(|error| format!("invalid date `{date}`: {error}"))?;
    let parsed_hour =
        parse_data_hour(&hour).ok_or_else(|| format!("unknown Office hour `{hour}`"))?;

    let engine =
        Breviarium::embedded().map_err(|error| format!("failed to load embedded data: {error}"))?;
    let office = engine
        .resolve_office(OfficeRequest::new(parsed_date, parsed_hour).with_language(language.as_str()))
        .map_err(|error| format!("failed to resolve Office: {error}"))?;

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
            let title = block.title.clone().unwrap_or_else(|| fallback_title.clone());
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
        page_title,
        title,
        date_label: parsed_date.format("%B %-d, %Y").to_string(),
        hour_label: display_hour(parsed_hour).to_string(),
        profile_label: office.profile.clone(),
        previous_date_path: format!("/officium/{}/{}", previous_date.format("%Y%m%d"), hour_path),
        next_date_path: format!("/officium/{}/{}", next_date.format("%Y%m%d"), hour_path),
        hour_links: hour_links(&date_path, &hour_path),
        blocks,
        diagnostics,
    })
}

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

fn push_lines(lines: &mut Vec<LineView>, kind: LineKind, class: &str, text: &str) {
    // Each node is one block: multiline text (a hymn, a psalm's verses) stays
    // together and renders as one paragraph with internal line breaks (the `<p>`
    // uses `white-space: pre-line`), rather than one `<p>` per line.
    lines.push(LineView {
        kind,
        class: class.to_string(),
        text: text.to_string(),
    });
}

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
