use dioxus::prelude::*;

use crate::lang::Language;
use crate::Route;

#[component]
pub(crate) fn LanguagePicker(
    id: &'static str,
    language: String,
    onselect: EventHandler<Language>,
    office: Option<(String, String)>,
) -> Element {
    let navigator = use_navigator();

    rsx! {
        select {
            id,
            class: "language-picker",
            aria_label: "Translation language",
            value: language,
            onchange: move |event| {
                let Some(choice) = Language::from_code(&event.value()) else {
                    return;
                };
                onselect.call(choice);
                if let Some((date, hour)) = &office {
                    navigator.push(Route::Officium {
                        lang: choice.code().to_string(),
                        date: date.clone(),
                        hour: hour.clone(),
                    });
                }
            },
            if language == "la" {
                option { value: "la", "Latina" }
            }
            for choice in Language::ALL {
                option {
                    key: "{choice.code()}",
                    value: choice.code(),
                    "{choice.label()}"
                }
            }
        }
    }
}
