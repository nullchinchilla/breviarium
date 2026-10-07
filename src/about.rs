use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn About() -> Element {
    rsx! {
        document::Title { "About Breviarium" }
        main { class: "container about",
            header {
                nav {
                    ul {
                        li { Link { to: Route::Home {}, "Breviarium" } }
                    }
                }
            }
            div { class: "about-columns",
            section { lang: "en",
            h1 { "About Breviarium" }
            p {
                "All of the data and source code is available on "
                a { href: "https://github.com/nullchinchilla/breviarium", "GitHub" }
                "."
            }
            p {
                "Latin and English data come from the "
                a { href: "https://www.divinumofficium.com/", "Divinum Officium project" }
                "."
            }
            p {
                "The Modern English translation uses the "
                a { href: "https://www.sacredbible.org/catholic/", "Catholic Public Domain Version" }
                " and our own translations of antiphons and collects."
            }
            p {
                "The ecumenical Chinese translation is based on the Chinese Union Version."
            }
            }
            section { lang: "zh-Hans",
                h1 { "关于 Breviarium" }
                p {
                    "所有数据和源代码均可在 "
                    a { href: "https://github.com/nullchinchilla/breviarium", "GitHub" }
                    " 上获取。"
                }
                p {
                    "拉丁文和英文数据来自 "
                    a { href: "https://www.divinumofficium.com/", "Divinum Officium 项目" }
                    "。"
                }
                p {
                    "现代英文译文采用 "
                    a { href: "https://www.sacredbible.org/catholic/", "Catholic Public Domain Version" }
                    "，对经和集祷经则由我们自行翻译。"
                }
                p { "普世中文译文以《和合本》为基础。" }
            }
            }
        }
    }
}
