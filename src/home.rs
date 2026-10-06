use dioxus::prelude::*;

use crate::clock::{client_now, hour_for_clock};
use crate::lang::{Language, STORAGE_KEY};
use crate::officium::hour::Hour;
use crate::Route;

/// Redirects using the browser clock and saved language after hydration.
/// The server renders a plain Lauds link for readers without JavaScript.
#[component]
pub(crate) fn Home() -> Element {
    let now = use_loader(client_now)?;
    let navigator = use_navigator();

    use_effect(move || {
        spawn(async move {
            let script = format!(
                "const d = new Date();
                 let stored = null;
                 try {{ stored = localStorage.getItem('{STORAGE_KEY}'); }} catch (e) {{}}
                 return {{
                     year: d.getFullYear(),
                     month: d.getMonth() + 1,
                     day: d.getDate(),
                     hour: d.getHours(),
                     lang: stored,
                 }};"
            );
            let Ok(value) = document::eval(&script).await else {
                return;
            };
            let part = |key: &str| value.get(key).and_then(|number| number.as_u64());
            let (Some(year), Some(month), Some(day), Some(clock)) =
                (part("year"), part("month"), part("day"), part("hour"))
            else {
                return;
            };
            let language = value
                .get("lang")
                .and_then(|lang| lang.as_str())
                .and_then(Language::from_code)
                .unwrap_or(Language::DEFAULT);
            navigator.replace(Route::Officium {
                lang: language.code().to_string(),
                date: format!("{year:04}{month:02}{day:02}"),
                hour: hour_for_clock(clock as u32).path().to_string(),
            });
        });
    });

    rsx! {
        document::Title { "Breviarium" }
        main {
            a {
                href: Route::Officium {
                    lang: Language::DEFAULT.code().to_string(),
                    date: now().date,
                    hour: Hour::Laudes.path().to_string(),
                }.to_string(),
                "Laudes"
            }
        }
    }
}
