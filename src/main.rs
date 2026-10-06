use dioxus::prelude::*;

mod clock;
mod home;
mod lang;
mod language_picker;
mod load_office;
mod officium;
use home::Home;
use officium::Officium;

const PICO_CSS: &str = "https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
// `asset!` paths are package-root relative and take no leading slash. The macro
// resolves to whatever URL the bundler emits; hardcoding a path like
// "/styles.css" silently 404s, since the CLI serves the asset directory under
// `/assets/` with hashed names.
const CUSTOM_CSS: Asset = asset!("assets/styles.css");
const JUNICODE_ROMAN: Asset = asset!("assets/fonts/JunicodeVF-Roman.woff2");
const JUNICODE_ITALIC: Asset = asset!("assets/fonts/JunicodeVF-Italic.woff2");

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
    #[route("/:lang/officium/:date/:hour")]
    Officium {
        lang: String,
        date: String,
        hour: String,
    },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
fn App() -> Element {
    rsx! {
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Stylesheet { href: PICO_CSS }
        document::Stylesheet { href: CUSTOM_CSS }
        // CSS url() references do not register files with the asset bundler.
        // Resolve the fonts through asset! so their emitted URLs stay correct.
        document::Style {
            r#"
                @font-face {{
                    font-family: "JunicodeVF";
                    font-style: normal;
                    font-weight: 300 700;
                    font-display: swap;
                    src: url("{JUNICODE_ROMAN}") format("woff2");
                }}
                @font-face {{
                    font-family: "JunicodeVF";
                    font-style: italic;
                    font-weight: 300 700;
                    font-display: swap;
                    src: url("{JUNICODE_ITALIC}") format("woff2");
                }}
            "#
        }
        Router::<Route> {}
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
                p {
                    Link { to: Route::Home {}, "Go to the home page" }
                }
            }
        }
    }
}

#[cfg(feature = "server")]
fn launch_server() -> ! {
    use dioxus::prelude::{DioxusRouterExt, ServeConfig};
    use dioxus::server::axum::Router;

    dioxus::serve(|| async { Ok(Router::new().serve_dioxus_application(ServeConfig::new(), App)) })
}
