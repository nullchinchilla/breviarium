use dioxus::prelude::*;

mod load_office;
mod officium;
use officium::Officium;

const PICO_CSS: &str = "https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
const CUSTOM_CSS: &str = "/styles.css";

fn main() {
    #[cfg(feature = "server")]
    launch_server();

    #[cfg(not(feature = "server"))]
    dioxus::launch(App);
}

#[derive(Clone, Debug, PartialEq, Routable)]
enum Route {
    #[route("/")]
    Home {},
    #[route("/officium/:date/:hour")]
    Officium { date: String, hour: String },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
fn App() -> Element {
    rsx! {
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Stylesheet { href: PICO_CSS }
        document::Stylesheet { href: CUSTOM_CSS }
        Router::<Route> {}
    }
}

#[component]
fn Home() -> Element {
    rsx! {
        main { class: "container",
            article {
                header { h1 { "Breviarium" } }
                p { "Redirecting to the current Office hour." }
            }
        }
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    let path = segments.join("/");
    rsx! {
        document::Title { "Not found" }
        main { class: "container",
            article {
                header { h1 { "Not found" } }
                p { "No Office route matches /{path}." }
                p { a { href: "/", "Go to the current Office" } }
            }
        }
    }
}

#[cfg(feature = "server")]
fn launch_server() -> ! {
    use dioxus::prelude::{DioxusRouterExt, ServeConfig};
    use dioxus::server::axum::{routing::get, Router};

    dioxus::serve(|| async {
        Ok(Router::new()
            .route("/", get(home_redirect))
            .serve_dioxus_application(ServeConfig::new(), App))
    })
}

#[cfg(feature = "server")]
async fn home_redirect(
    headers: dioxus::server::axum::http::HeaderMap,
) -> dioxus::server::axum::response::Response {
    use chrono::{Local, Timelike};
    use dioxus::server::axum::{
        http::{header::LOCATION, HeaderValue, StatusCode},
        response::IntoResponse,
    };

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

#[cfg(feature = "server")]
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
