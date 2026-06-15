use dioxus::prelude::*;

#[cfg(feature = "server")]
mod server;

const PICO_CSS: &str = "https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut server_hello = use_action(greet_from_server);

    let response = match server_hello.value().as_ref() {
        Some(Ok(message)) => message.read().clone(),
        Some(Err(error)) => format!("Server error: {error}"),
        None => "Click the button to call a Dioxus server function.".to_string(),
    };

    rsx! {
        document::Meta { name: "color-scheme", content: "light dark" }
        document::Stylesheet { href: PICO_CSS }
        document::Stylesheet { href: MAIN_CSS }

        main { class: "container app-shell",
            article { class: "hello-card",
                header {
                    p { class: "eyebrow", "Dioxus Fullstack + Pico CSS" }
                    h1 { "Hello from Breviarium" }
                }

                p {
                    "This page is rendered with Dioxus, styled by Pico's semantic defaults, and backed by a Rust server function."
                }

                button {
                    onclick: move |_| server_hello.call("world".to_string()),
                    "Ask the server"
                }

                footer {
                    code {
                        "{response}"
                    }
                }
            }
        }
    }
}

#[get("/api/hello/:name")]
async fn greet_from_server(name: String) -> Result<String> {
    #[cfg(feature = "server")]
    {
        Ok(server::greetings::hello(&name))
    }

    #[cfg(not(feature = "server"))]
    {
        let _ = name;
        unreachable!("server functions are executed by the server runtime")
    }
}
