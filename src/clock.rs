//! Approximating the reader's local date and hour.
//!
//! The home page opens on today's Office, so it needs to know what "today" and
//! "now" are for the reader. Two sources cooperate: the server approximates the
//! clock from the request's origin for the fallback Lauds link, and the browser
//! uses its real local clock to redirect after hydration (see [`crate::home`]).

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::officium::hour::Hour;

/// The reader's approximate local date and Office hour.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct ClientNow {
    /// The local date as a `YYYYMMDD` path segment.
    pub(crate) date: String,
    /// The canonical path segment of the hour being said around now.
    pub(crate) hour: String,
}

/// Approximates the reader's clock from the request headers, falling back to
/// the server's own clock when the origin can't be placed.
#[server]
pub(crate) async fn client_now() -> Result<ClientNow, ServerFnError> {
    use chrono::{Local, Timelike};
    use dioxus::fullstack::{FullstackContext, HeaderMap};

    let headers = FullstackContext::extract::<HeaderMap, _>().await?;
    let now = approximate_client_now(&headers).unwrap_or_else(|| Local::now().naive_local());

    Ok(ClientNow {
        date: now.date().format("%Y%m%d").to_string(),
        hour: hour_for_clock(now.hour()).path().to_string(),
    })
}

/// The Office hour customarily said at the given wall-clock hour.
///
/// Deliberately not server-only: the browser-side correction buckets its own
/// clock through this same function, so the boundaries live in exactly one
/// place rather than being restated in JavaScript.
pub(crate) fn hour_for_clock(hour: u32) -> Hour {
    match hour {
        0..=4 => Hour::Matutinum,
        5..=7 => Hour::Laudes,
        8..=9 => Hour::Prima,
        10..=11 => Hour::Tertia,
        12..=14 => Hour::Sexta,
        15..=16 => Hour::Nona,
        17..=20 => Hour::Vesperae,
        _ => Hour::Completorium,
    }
}

#[cfg(feature = "server")]
fn approximate_client_now(
    headers: &dioxus::server::axum::http::HeaderMap,
) -> Option<chrono::NaiveDateTime> {
    let ip = client_ip(headers)?;
    let country = geoip_country(ip)?;
    let timezone = timezone_for_country(&country)?;
    date_in_timezone(&timezone)
}

#[cfg(feature = "server")]
fn client_ip(headers: &dioxus::server::axum::http::HeaderMap) -> Option<std::net::IpAddr> {
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

#[cfg(feature = "server")]
fn parse_ip_token(token: &str) -> Option<std::net::IpAddr> {
    use std::net::{IpAddr, SocketAddr};

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

#[cfg(feature = "server")]
fn geoip_country(ip: std::net::IpAddr) -> Option<String> {
    let binary = if ip.is_ipv6() {
        "geoiplookup6"
    } else {
        "geoiplookup"
    };
    let output = std::process::Command::new(binary)
        .arg(ip.to_string())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_geoip_country(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(feature = "server")]
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

#[cfg(feature = "server")]
fn timezone_for_country(country: &str) -> Option<String> {
    timezone_for_country_in("/usr/share/zoneinfo/zone1970.tab", country)
        .or_else(|| timezone_for_country_in("/usr/share/zoneinfo/zone.tab", country))
}

#[cfg(feature = "server")]
fn timezone_for_country_in(path: &str, country: &str) -> Option<String> {
    let table = std::fs::read_to_string(path).ok()?;
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

#[cfg(feature = "server")]
fn date_in_timezone(timezone: &str) -> Option<chrono::NaiveDateTime> {
    let output = std::process::Command::new("date")
        .env("TZ", timezone)
        .arg("+%Y%m%d%H%M")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    chrono::NaiveDateTime::parse_from_str(text.trim(), "%Y%m%d%H%M").ok()
}

#[cfg(test)]
mod tests {
    use super::hour_for_clock;
    use crate::officium::hour::Hour;

    #[test]
    fn hour_for_clock_covers_every_boundary() {
        let cases = [
            (0, Hour::Matutinum),
            (4, Hour::Matutinum),
            (5, Hour::Laudes),
            (7, Hour::Laudes),
            (8, Hour::Prima),
            (9, Hour::Prima),
            (10, Hour::Tertia),
            (11, Hour::Tertia),
            (12, Hour::Sexta),
            (14, Hour::Sexta),
            (15, Hour::Nona),
            (16, Hour::Nona),
            (17, Hour::Vesperae),
            (20, Hour::Vesperae),
            (21, Hour::Completorium),
            (23, Hour::Completorium),
        ];
        for (clock, expected) in cases {
            assert_eq!(hour_for_clock(clock), expected, "at {clock}:00");
        }
    }
}
