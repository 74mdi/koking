use dioxus::prelude::*;

const CSS: &str = include_str!("../style.css");

#[component]
fn Sidebar() -> Element {
    rsx! {
        div { class: "sidebar glass",
            div { class: "brand glass-inner",
                div { class: "logo-blob" }
                div { class: "app-title", "Signal Alt" }
                div { class: "app-sub", "Private messaging" }
            }
            nav { class: "nav",
                a { class: "nav-item active", href: "#", span { class: "icon", "💬" }, span { "Chats" } }
                a { class: "nav-item", href: "#", span { class: "icon", "📞" }, span { "Calls" } }
                a { class: "nav-item", href: "#", span { class: "icon", "👤" }, span { "Contacts" } }
                a { class: "nav-item", href: "#", span { class: "icon", "⚙️" }, span { "Settings" } }
            }
            div { class: "spacer" }
            div { class: "account glass-inner small",
                div { class: "avatar", "H" }
                div { class: "meta",
                    span { "You" }
                    span { class: "status", "● online" }
                }
            }
        }
    }
}

#[component]
fn Chat() -> Element {
    rsx! {
        div { class: "chat-shell glass",
            div { class: "chat-header glass-inner",
                div { class: "back", "‹" }
                div { class: "contact",
                    div { class: "avatar", "S" }
                    div { class: "name", "Sarah" }
                    div { class: "sub", "online • end-to-end encrypted" }
                }
                div { class: "actions",
                    button { class: "icon-btn glass", "📞" }
                    button { class: "icon-btn glass", "🎥" }
                    button { class: "icon-btn glass", "ℹ️" }
                }
            }
            div { class: "messages",
                div { class: "msg bubble them", "Hey, did you see the update?" }
                div { class: "msg bubble me", "Yeah, looks clean. Liquid glass is sick." }
                div { class: "msg bubble them", "Material 3 expressive too, right?" }
                div { class: "msg bubble me", "Exactly. Smooth on web, PC, and Android." }
            }
            div { class: "composer glass-inner",
                input { class: "input", placeholder: "Type a message…" }
                button { class: "send glass", "↑" }
            }
        }
    }
}

#[component]
pub fn App() -> Element {
    rsx! {
        style { {CSS} }
        div { class: "app-shell",
            Sidebar {}
            Chat {}
        }
    }
}
