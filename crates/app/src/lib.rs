use dioxus::prelude::*;

const CSS: &str = include_str!("../style.css");

#[component]
fn WorkspaceBar() -> Element {
    rsx! {
        header { class: "workspace-bar",
            a { class: "brand-lockup", href: "#", aria_label: "Koking home",
                span { class: "brand-mark", "k" }
                span { class: "brand-name", "Koking" }
            }
            div { class: "workspace-search",
                span { class: "search-mark", aria_hidden: "true", "⌕" }
                input {
                    class: "search-input",
                    r#type: "search",
                    placeholder: "Search conversations",
                    aria_label: "Search conversations",
                }
                kbd { "⌘ K" }
            }
            div { class: "workspace-profile", title: "Hamdi's profile",
                span { class: "profile-initial", "H" }
            }
        }
    }
}

#[component]
fn NavigationRail() -> Element {
    rsx! {
        nav { class: "navigation-rail", aria_label: "Main navigation",
            a { class: "rail-item selected", href: "#", aria_label: "Chats", aria_current: "page",
                span { class: "rail-glyph", aria_hidden: "true", "◌" }
            }
            a { class: "rail-item", href: "#", aria_label: "People",
                span { class: "rail-glyph", aria_hidden: "true", "♙" }
            }
            div { class: "rail-spacer" }
            button { class: "rail-item profile-rail", r#type: "button", aria_label: "Profile for Hamdi",
                span { class: "profile-initial", "H" }
            }
        }
    }
}

#[component]
fn ConversationList(on_select: EventHandler<()>) -> Element {
    let mut query = use_signal(String::new);
    let term = query.read().to_lowercase();
    let conversations = [
        ("Sarah", "S", "Are we still on for coffee?", "9:42", true),
        ("Omar", "O", "Sent the project notes", "Yesterday", false),
        ("Lina", "L", "That sounds great ✨", "Mon", false),
    ];

    rsx! {
        aside { class: "conversation-pane", aria_label: "Conversations",
            header { class: "conversation-heading",
                div {
                    p { class: "eyebrow", "MESSAGES" }
                    h1 { "Inbox" }
                }
                button { class: "new-chat-button", r#type: "button", aria_label: "New message", title: "New message", "+" }
            }
            label { class: "conversation-search",
                span { class: "search-mark", aria_hidden: "true", "⌕" }
                input {
                    r#type: "search",
                    placeholder: "Find a conversation",
                    aria_label: "Find a conversation",
                    value: "{query}",
                    oninput: move |event| query.set(event.value()),
                }
            }
            h2 { class: "list-heading", "Recent" }
            div { class: "conversation-list",
                for (name, initial, preview, time, selected) in conversations {
                    if term.is_empty() || name.to_lowercase().contains(term.as_str()) {
                        button {
                            class: if selected { "conversation-row selected" } else { "conversation-row" },
                            r#type: "button",
                            aria_current: if selected { "true" } else { "false" },
                            onclick: move |_| on_select.call(()),
                            div { class: "avatar", aria_hidden: "true", "{initial}" }
                            div { class: "conversation-copy",
                                div { class: "conversation-line",
                                    span { class: "conversation-name", "{name}" }
                                    time { class: "conversation-time", "{time}" }
                                }
                                p { class: "conversation-preview", "{preview}" }
                            }
                        }
                    }
                }
            }
            footer { class: "list-footer",
                span { class: "online-indicator", aria_hidden: "true" }
                span { "You're online" }
            }
        }
    }
}

#[component]
fn MessageBubble(text: &'static str, time: &'static str, outgoing: bool) -> Element {
    rsx! {
        div { class: if outgoing { "message-row outgoing" } else { "message-row incoming" },
            article { class: if outgoing { "message-bubble sent" } else { "message-bubble received" },
                p { "{text}" }
                div { class: "message-meta",
                    time { "{time}" }
                    if outgoing { span { class: "read-status", " · Read" } }
                }
            }
        }
    }
}

#[component]
fn Chat(on_back: EventHandler<()>) -> Element {
    let mut draft = use_signal(String::new);
    let mut sent_messages = use_signal(|| Vec::<String>::new());

    rsx! {
        main { class: "chat-panel", aria_label: "Conversation with Sarah",
            header { class: "chat-header",
                button { class: "mobile-back", r#type: "button", aria_label: "Back to conversations", onclick: move |_| on_back.call(()), "‹" }
                div { class: "avatar contact-avatar", aria_hidden: "true", "S" }
                div { class: "contact-details",
                    h2 { "Sarah" }
                    p { class: "contact-status",
                        span { class: "online-indicator", aria_hidden: "true" }
                        "Active now"
                    }
                }
                div { class: "header-context", "Direct message" }
            }

            section { class: "message-history", aria_label: "Messages", aria_live: "polite",
                div { class: "message-content",
                    div { class: "date-divider", span {}, time { "Today" }, span {} }
                    MessageBubble { text: "Hey, did you see the update?", time: "9:38", outgoing: false }
                    MessageBubble { text: "Yeah, the new layout feels much calmer.", time: "9:40", outgoing: true }
                    MessageBubble { text: "I like how the conversation list is easier to scan now.", time: "9:41", outgoing: false }
                    MessageBubble { text: "Same here. I can find things much faster.", time: "9:42", outgoing: true }
                    for (index, message) in sent_messages.read().iter().enumerate() {
                        div { key: "sent-{index}", class: "message-row outgoing",
                            article { class: "message-bubble sent",
                                p { "{message}" }
                                div { class: "message-meta", time { "Now" }, span { class: "read-status", " · Sent" } }
                            }
                        }
                    }
                }
            }

            footer { class: "composer-region",
                form {
                    class: "composer",
                    aria_label: "Send a message to Sarah",
                    onsubmit: move |event: FormEvent| {
                        event.prevent_default();
                        let text = draft.read().trim().to_string();
                        if !text.is_empty() {
                            sent_messages.write().push(text);
                            draft.set(String::new());
                        }
                    },
                    textarea {
                        class: "composer-input",
                        rows: "1",
                        placeholder: "Message Sarah",
                        aria_label: "Message Sarah",
                        value: "{draft}",
                        oninput: move |event| draft.set(event.value()),
                    }
                    button {
                        class: "send-button",
                        r#type: "submit",
                        aria_label: "Send message",
                        disabled: draft.read().trim().is_empty(),
                        "Send"
                    }
                }
                p { class: "composer-hint", "Press Send to share a message" }
            }
        }
    }
}

#[component]
pub fn App() -> Element {
    let mut mobile_chat_open = use_signal(|| false);
    rsx! {
        style { {CSS} }
        div { class: if *mobile_chat_open.read() { "app-shell mobile-chat-open" } else { "app-shell" },
            WorkspaceBar {}
            div { class: "workspace-body",
                NavigationRail {}
                ConversationList { on_select: move |_| mobile_chat_open.set(true) }
                Chat { on_back: move |_| mobile_chat_open.set(false) }
            }
        }
    }
}
