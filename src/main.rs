use dioxus::prelude::*;

mod officium;
use officium::Officium;

#[cfg(feature = "server")]
mod server;

const PICO_CSS: &str = "https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
const CUSTOM_CSS: &str = "/styles.css";

fn main() {
    #[cfg(feature = "server")]
    server::launch();

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
