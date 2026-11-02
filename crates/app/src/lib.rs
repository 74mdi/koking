use dioxus::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

const CSS: &str = include_str!("../style.css");
const DEFAULT_API_URL: &str = match option_env!("KOKING_API_URL") {
    Some(url) => url,
    None => "http://127.0.0.1:3000",
};
const SESSION_KEY: &str = "koking.session";
const API_KEY: &str = "koking.api";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct PublicUser {
    id: String,
    username: String,
    display_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AuthResponse {
    token: String,
    user: PublicUser,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Conversation {
    id: String,
    peer: PublicUser,
    last_message: Option<String>,
    updated_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct ChatMessage {
    id: i64,
    conversation_id: String,
    sender_id: String,
    sender_name: String,
    body: String,
    created_at: String,
}

#[derive(Serialize)]
struct LoginBody<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct RegisterBody<'a> {
    username: &'a str,
    display_name: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct UsernameBody<'a> {
    username: &'a str,
}

#[derive(Serialize)]
struct MessageBody<'a> {
    body: &'a str,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    error: String,
}

async fn decode_response<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, String> {
    if !response.status().is_success() {
        let status = response.status();
        return match response.json::<ApiErrorBody>().await {
            Ok(body) => Err(body.error),
            Err(_) => Err(format!("Server returned {status}")),
        };
    }
    response
        .json::<T>()
        .await
        .map_err(|_| "The server returned an invalid response".to_string())
}

fn api_endpoint(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

async fn get_json<T: DeserializeOwned>(base: &str, token: &str, path: &str) -> Result<T, String> {
    let response = reqwest::Client::new()
        .get(api_endpoint(base, path))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| "Could not connect to the Koking server".to_string())?;
    decode_response(response).await
}

async fn post_json<T: DeserializeOwned, B: Serialize>(
    base: &str,
    token: Option<&str>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let mut request = reqwest::Client::new()
        .post(api_endpoint(base, path))
        .json(body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|_| "Could not connect to the Koking server".to_string())?;
    decode_response(response).await
}

async fn post_empty<B: Serialize>(
    base: &str,
    token: &str,
    path: &str,
    body: &B,
) -> Result<(), String> {
    let response = reqwest::Client::new()
        .post(api_endpoint(base, path))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .map_err(|_| "Could not connect to the Koking server".to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        let status = response.status();
        match response.json::<ApiErrorBody>().await {
            Ok(body) => Err(body.error),
            Err(_) => Err(format!("Server returned {status}")),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn storage_get(key: &str) -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(key)
        .ok()?
}

#[cfg(not(target_arch = "wasm32"))]
fn storage_get(_key: &str) -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
fn storage_set(key: &str, value: &str) {
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.set_item(key, value);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn storage_set(_key: &str, _value: &str) {}

#[cfg(target_arch = "wasm32")]
fn storage_remove(key: &str) {
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.remove_item(key);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn storage_remove(_key: &str) {}

#[cfg(target_arch = "wasm32")]
async fn poll_delay() {
    gloo_timers::future::TimeoutFuture::new(2_500).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn poll_delay() {
    futures_timer::Delay::new(std::time::Duration::from_millis(2_500)).await;
}

#[component]
pub fn App() -> Element {
    let saved_session = storage_get(SESSION_KEY)
        .and_then(|value| serde_json::from_str::<AuthResponse>(&value).ok());
    let mut token = use_signal(|| saved_session.as_ref().map(|session| session.token.clone()));
    let mut user = use_signal(|| saved_session.map(|session| session.user));
    let mut api_url =
        use_signal(|| storage_get(API_KEY).unwrap_or_else(|| DEFAULT_API_URL.to_string()));
    let mut auth_mode_register = use_signal(|| false);
    let mut username = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut auth_error = use_signal(String::new);
    let mut is_busy = use_signal(|| false);
    let mut conversations = use_signal(Vec::<Conversation>::new);
    let mut active = use_signal(|| None::<Conversation>);
    let mut messages = use_signal(Vec::<ChatMessage>::new);
    let mut draft = use_signal(String::new);
    let mut mobile_chat_open = use_signal(|| false);
    let mut new_chat_open = use_signal(|| false);
    let mut user_search = use_signal(String::new);
    let mut user_results = use_signal(Vec::<PublicUser>::new);
    let mut user_search_busy = use_signal(|| false);
    let mut user_search_error = use_signal(String::new);
    let mut search_generation = use_signal(|| 0u32);

    {
        let mut token_signal = token;
        let mut user_signal = user;
        let api_signal = api_url;
        let mut conversations_signal = conversations;
        let mut active_signal = active;
        use_effect(move || {
            let session_token = token_signal.read().clone();
            let base = api_signal.read().trim().to_string();
            if let Some(session_token) = session_token {
                spawn(async move {
                    let result = get_json::<PublicUser>(&base, &session_token, "/api/me").await;
                    match result {
                        Ok(profile) => user_signal.set(Some(profile)),
                        Err(_) => {
                            token_signal.set(None);
                            user_signal.set(None);
                            storage_remove(SESSION_KEY);
                            return;
                        }
                    }
                    if let Ok(list) =
                        get_json::<Vec<Conversation>>(&base, &session_token, "/api/conversations")
                            .await
                    {
                        if active_signal.read().is_none() {
                            active_signal.set(list.first().cloned());
                        }
                        conversations_signal.set(list);
                    }
                });
            }
        });
    }

    {
        let token_signal = token;
        let api_signal = api_url;
        let active_signal = active;
        let mut messages_signal = messages;
        use_effect(move || {
            let Some(session_token) = token_signal.read().clone() else {
                return;
            };
            let Some(conversation) = active_signal.read().clone() else {
                messages_signal.set(Vec::new());
                return;
            };
            let base = api_signal.read().trim().to_string();
            spawn(async move {
                loop {
                    if let Ok(list) = get_json::<Vec<ChatMessage>>(
                        &base,
                        &session_token,
                        &format!("/api/conversations/{}/messages", conversation.id),
                    )
                    .await
                    {
                        messages_signal.set(list);
                    }
                    poll_delay().await;
                    if token_signal.read().as_deref() != Some(session_token.as_str())
                        || active_signal.read().as_ref().map(|item| item.id.as_str())
                            != Some(conversation.id.as_str())
                    {
                        break;
                    }
                }
            });
        });
    }

    let signed_in = token.read().is_some();
    let profile_initial = user
        .read()
        .as_ref()
        .and_then(|profile| profile.display_name.chars().next())
        .unwrap_or('K');
    let profile_username = user
        .read()
        .as_ref()
        .map(|profile| profile.username.clone())
        .unwrap_or_else(|| "user".to_string());
    let filtered_conversations = {
        let term = user_search.read().to_lowercase();
        conversations
            .read()
            .iter()
            .filter(|conversation| {
                term.is_empty()
                    || conversation
                        .peer
                        .display_name
                        .to_lowercase()
                        .contains(&term)
                    || conversation.peer.username.to_lowercase().contains(&term)
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut submit_auth = move || {
        if *is_busy.read() {
            return;
        }
        is_busy.set(true);
        auth_error.set(String::new());
        let base = api_url.read().trim().to_string();
        let name = username.read().trim().to_string();
        let display = display_name.read().trim().to_string();
        let secret = password.read().clone();
        let register = *auth_mode_register.read();
        let mut token_out = token;
        let mut user_out = user;
        let mut busy_out = is_busy;
        let mut error_out = auth_error;
        let mut password_out = password;
        let mut display_name_out = display_name;
        let mut register_mode_out = auth_mode_register;
        spawn(async move {
            let response = if register {
                post_json::<AuthResponse, _>(
                    &base,
                    None,
                    "/api/register",
                    &RegisterBody {
                        username: &name,
                        display_name: &display,
                        password: &secret,
                    },
                )
                .await
            } else {
                post_json::<AuthResponse, _>(
                    &base,
                    None,
                    "/api/login",
                    &LoginBody {
                        username: &name,
                        password: &secret,
                    },
                )
                .await
            };
            match response {
                Ok(session) => {
                    storage_set(
                        SESSION_KEY,
                        &serde_json::to_string(&session).unwrap_or_default(),
                    );
                    storage_set(API_KEY, &base);
                    token_out.set(Some(session.token));
                    user_out.set(Some(session.user));
                    password_out.set(String::new());
                    display_name_out.set(String::new());
                    register_mode_out.set(false);
                }
                Err(error) => error_out.set(error),
            }
            busy_out.set(false);
        });
    };

    rsx! {
        style { {CSS} }
        if !signed_in {
            main { class: "auth-shell",
                div { class: "auth-glow", aria_hidden: "true" }
                section { class: "auth-card",
                    div { class: "auth-brand",
                        span { class: "brand-mark", "k" }
                        span { class: "brand-name", "Koking" }
                    }
                    p { class: "eyebrow", "PRIVATE MESSAGING" }
                    h1 { if *auth_mode_register.read() { "Create your account" } else { "Welcome back" } }
                    p { class: "auth-subtitle", if *auth_mode_register.read() { "Choose a username and connect to your Koking server." } else { "Sign in to your self-hosted Koking server." } }
                    div { class: "auth-form",
                        if *auth_mode_register.read() {
                            label { "Display name"
                                input { r#type: "text", placeholder: "Your name", autocomplete: "name", value: "{display_name}", oninput: move |event| display_name.set(event.value()) }
                            }
                        }
                        label { "Username"
                            input { r#type: "text", placeholder: "your_username", autocomplete: "username", minlength: "3", maxlength: "24", required: true, value: "{username}", oninput: move |event| username.set(event.value()) }
                        }
                        label { "Password"
                            input { r#type: "password", placeholder: "At least 10 characters", autocomplete: if *auth_mode_register.read() { "new-password" } else { "current-password" }, minlength: if *auth_mode_register.read() { "10" } else { "1" }, required: true, value: "{password}", oninput: move |event| password.set(event.value()) }
                        }
                        label { "Server URL"
                            input { r#type: "url", placeholder: "http://127.0.0.1:3000", value: "{api_url}", required: true, oninput: move |event| api_url.set(event.value().trim_end_matches('/').to_string()) }
                        }
                        if !auth_error.read().is_empty() { p { class: "auth-error", role: "alert", "{auth_error}" } }
                        button { class: "auth-submit", r#type: "button", disabled: *is_busy.read(), onclick: move |_| submit_auth(), if *is_busy.read() { "Connecting…" } else if *auth_mode_register.read() { "Create account" } else { "Sign in" } }
                    }
                    button { class: "auth-switch", r#type: "button", onclick: move |_| { auth_error.set(String::new()); let next_mode = !*auth_mode_register.read(); auth_mode_register.set(next_mode); }, if *auth_mode_register.read() { "Already have an account? Sign in" } else { "New to this server? Create an account" } }
                    p { class: "auth-footnote", "Your account and messages live on the server you choose." }
                }
            }
        } else {
            div { class: if *mobile_chat_open.read() { "app-shell mobile-chat-open" } else { "app-shell" },
                header { class: "workspace-bar",
                    a { class: "brand-lockup", href: "#", aria_label: "Koking home",
                        span { class: "brand-mark", "k" }
                        span { class: "brand-name", "Koking" }
                    }
                    label { class: "workspace-search",
                        span { class: "search-mark", aria_hidden: "true", "⌕" }
                        input { class: "search-input", r#type: "search", placeholder: "Search conversations", aria_label: "Search conversations", value: "{user_search}", oninput: move |event| user_search.set(event.value()) }
                        kbd { "⌘ K" }
                    }
                    div { class: "workspace-profile",
                        button { class: "profile-initial", r#type: "button", title: "Sign out", aria_label: "Sign out", onclick: move |_| {
                            if let Some(current_token) = token.read().clone() {
                                let base = api_url.read().trim().to_string();
                                spawn(async move { let _ = post_empty(&base, &current_token, "/api/logout", &serde_json::json!({})).await; });
                            }
                            storage_remove(SESSION_KEY);
                            token.set(None); user.set(None); conversations.set(Vec::new()); active.set(None); messages.set(Vec::new());
                            username.set(String::new()); password.set(String::new()); display_name.set(String::new()); auth_mode_register.set(false);
                        }, "{profile_initial}" }
                    }
                }
                div { class: "workspace-body",
                    nav { class: "navigation-rail", aria_label: "Main navigation",
                        button { class: "rail-item selected", r#type: "button", aria_label: "Chats", aria_pressed: "true", span { class: "rail-glyph", aria_hidden: "true", "◌" } }
                                            button { class: "rail-item", r#type: "button", aria_label: "New conversation", onclick: move |_| { user_search.set(String::new()); user_results.set(Vec::new()); new_chat_open.set(true); }, span { class: "rail-glyph", aria_hidden: "true", "＋" } }
                        div { class: "rail-spacer" }
                        span { class: "profile-initial", aria_hidden: "true", "{profile_initial}" }
                    }
                    aside { class: "conversation-pane", aria_label: "Conversations",
                        header { class: "conversation-heading",
                            div { p { class: "eyebrow", "MESSAGES" } h1 { "Inbox" } }
                                            button { class: "new-chat-button", r#type: "button", aria_label: "New message", title: "New message", onclick: move |_| { user_search.set(String::new()); user_results.set(Vec::new()); new_chat_open.set(true); }, "+" }
                        }
                        label { class: "conversation-search",
                            span { class: "search-mark", aria_hidden: "true", "⌕" }
                            input { r#type: "search", placeholder: "Find a conversation", aria_label: "Find a conversation", value: "{user_search}", oninput: move |event| user_search.set(event.value()) }
                        }
                        h2 { class: "list-heading", "Recent" }
                        div { class: "conversation-list",
                            if filtered_conversations.is_empty() {
                                p { class: "empty-state", if conversations.read().is_empty() { "Start a new conversation to get chatting." } else { "No conversations match your search." } }
                            }
                            for conversation in filtered_conversations {
                                {
                                    let is_selected = active.read().as_ref().map(|item| item.id.as_str()) == Some(conversation.id.as_str());
                                    let conv_for_click = conversation.clone();
                                    let last_message = conversation.last_message.clone().unwrap_or_else(|| format!("@{}", conversation.peer.username));
                                    let peer = conversation.peer.clone();
                                    rsx! {
                                        button {
                                            key: "{conversation.id}", class: if is_selected { "conversation-row selected" } else { "conversation-row" },
                                            r#type: "button", aria_pressed: "{is_selected}",
                                            onclick: move |_| { active.set(Some(conv_for_click.clone())); mobile_chat_open.set(true); },
                                            div { class: "avatar", aria_hidden: "true", "{peer.display_name.chars().next().unwrap_or('K')}" }
                                            div { class: "conversation-copy",
                                                div { class: "conversation-line", span { class: "conversation-name", "{peer.display_name}" } }
                                                p { class: "conversation-preview", "{last_message}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        footer { class: "list-footer", span { class: "online-indicator", aria_hidden: "true" } span { "Connected as @{profile_username}" } }
                    }
                    main { class: "chat-panel",
                        {
                            if let Some(conversation) = active.read().clone() {
                                let peer = conversation.peer.clone();
                                rsx! {
                                    header { class: "chat-header",
                                        button { class: "mobile-back", r#type: "button", aria_label: "Back to conversations", onclick: move |_| mobile_chat_open.set(false), "‹" }
                                        div { class: "avatar contact-avatar", aria_hidden: "true", "{peer.display_name.chars().next().unwrap_or('K')}" }
                                        div { class: "contact-details", h2 { "{peer.display_name}" } p { class: "contact-status", span { class: "online-indicator", aria_hidden: "true" } "On this Koking server" } }
                                        div { class: "header-context", "@{peer.username}" }
                                    }
                                    section { class: "message-history", aria_label: "Messages", aria_live: "polite",
                                        div { class: "message-content",
                                            if messages.read().is_empty() { div { class: "empty-chat", p { "This is the beginning of your conversation." } } }
                                            for message in messages.read().iter() {
                                                {
                                                    let outgoing = user.read().as_ref().map(|profile| profile.id.as_str()) == Some(message.sender_id.as_str());
                                                    let message_body = message.body.clone();
                                                    let sender_name = message.sender_name.clone();
                                                    let created_at = message.created_at.clone();
                                                    rsx! {
                                                        div { key: "{message.id}", class: if outgoing { "message-row outgoing" } else { "message-row incoming" },
                                                            article { class: if outgoing { "message-bubble sent" } else { "message-bubble received" },
                                                                if !outgoing { span { class: "message-sender", "{sender_name}" } }
                                                                p { "{message_body}" }
                                                                div { class: "message-meta", time { "{created_at}" } }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    footer { class: "composer-region",
                                        div {
                                            class: "composer", aria_label: "Send a message",
                                            textarea { class: "composer-input", rows: "1", placeholder: "Message @{peer.username}", aria_label: "Message", value: "{draft}", oninput: move |event| draft.set(event.value()) }
                                            button {
                                                class: "send-button",
                                                r#type: "button",
                                                disabled: draft.read().trim().is_empty(),
                                                onclick: move |_| {
                                                let body = draft.read().trim().to_string();
                                                if body.is_empty() { return; }
                                                let Some(session_token) = token.read().clone() else { return; };
                                                let base = api_url.read().trim().to_string();
                                                let conversation_id = conversation.id.clone();
                                                let mut messages_out = messages;
                                                let mut draft_out = draft;
                                                let mut error_out = auth_error;
                                                spawn(async move {
                                                    let path = format!("/api/conversations/{conversation_id}/messages");
                                                    match post_json::<ChatMessage, _>(&base, Some(&session_token), &path, &MessageBody { body: &body }).await {
                                                        Ok(message) => { messages_out.write().push(message); draft_out.set(String::new()); error_out.set(String::new()); }
                                                        Err(error) => error_out.set(error),
                                                    }
                                                });
                                                },
                                                "Send"
                                            }
                                        }
                                        p { class: "composer-hint", if auth_error.read().is_empty() { "Messages are stored by your Koking server" } else { "{auth_error}" } }
                                    }
                                }
                            } else {
                                rsx! { section { class: "welcome-panel", div { class: "welcome-mark", "k" } h2 { "Your conversations" } p { "Choose a chat or start a new one." } button { class: "auth-submit welcome-button", r#type: "button", onclick: move |_| { user_search.set(String::new()); user_results.set(Vec::new()); new_chat_open.set(true); }, "Start a conversation" } } }
                            }
                        }
                    }
                }
                if *new_chat_open.read() {
                    div { class: "modal-scrim", role: "presentation", onclick: move |_| new_chat_open.set(false),
                        section { class: "new-chat-modal", role: "dialog", aria_modal: "true", aria_label: "Start a conversation", onclick: move |event| event.stop_propagation(),
                            button { class: "modal-close", r#type: "button", aria_label: "Close", onclick: move |_| new_chat_open.set(false), "×" }
                            p { class: "eyebrow", "NEW MESSAGE" }
                            h2 { "Find someone" }
                            p { class: "auth-subtitle", "Search usernames and names on this server." }
                            input { class: "new-chat-search", r#type: "search", placeholder: "Search people…", aria_label: "Search people", value: "{user_search}", oninput: move |event| {
                                let query = event.value();
                                user_search.set(query.clone());
                                user_results.set(Vec::new());
                                user_search_error.set(String::new());
                                let generation = search_generation.with_mut(|current| { *current += 1; *current });
                                if query.trim().chars().count() < 2 { user_search_busy.set(false); return; }
                                user_search_busy.set(true);
                                let base = api_url.read().trim().to_string();
                                let Some(session_token) = token.read().clone() else { return; };
                                let mut results_out = user_results;
                                let generation_signal = search_generation;
                                let encoded = url::form_urlencoded::Serializer::new(String::new()).append_pair("q", query.trim()).finish();
                                spawn(async move {
                                    poll_delay().await;
                                    if *generation_signal.read() != generation { return; }
                                    match get_json::<Vec<PublicUser>>(&base, &session_token, &format!("/api/users?{encoded}")).await {
                                        Ok(results) if *generation_signal.read() == generation => { results_out.set(results); user_search_busy.set(false); }
                                        Err(error) if *generation_signal.read() == generation => { user_search_error.set(error); user_search_busy.set(false); }
                                        _ => {}
                                    }
                                });
                            } }
                            div { class: "user-results",
                                if !user_search_error.read().is_empty() { p { class: "auth-error", role: "alert", "{user_search_error}" } }
                                else if *user_search_busy.read() { p { class: "empty-state", "Searching this server…" } }
                                else if user_search.read().trim().chars().count() >= 2 && user_results.read().is_empty() { p { class: "empty-state", "No people found." } }
                                for person in user_results.read().iter() {
                                    {
                                        let person = person.clone();
                                        let name = person.display_name.clone();
                                        let handle = person.username.clone();
                                        rsx! {
                                            button { key: "{person.id}", class: "user-result", r#type: "button", onclick: move |_| {
                                                let base = api_url.read().trim().to_string();
                                                let Some(session_token) = token.read().clone() else { return; };
                                                let username = person.username.clone();
                                                let mut active_out = active;
                                                let mut conversations_out = conversations;
                                                let mut open_out = new_chat_open;
                                                let mut mobile_out = mobile_chat_open;
                                                spawn(async move {
                                                    match post_json::<Conversation, _>(&base, Some(&session_token), "/api/conversations", &UsernameBody { username: &username }).await {
                                                        Ok(conversation) => {
                                                            active_out.set(Some(conversation.clone()));
                                                            let mut list = conversations_out.read().clone();
                                                            if !list.iter().any(|item| item.id == conversation.id) { list.insert(0, conversation); }
                                                            conversations_out.set(list);
                                                            open_out.set(false); mobile_out.set(true); user_search.set(String::new()); user_results.set(Vec::new());
                                                        }
                                                        Err(error) => auth_error.set(error),
                                                    }
                                                });
                                            },
                                                div { class: "avatar", aria_hidden: "true", "{name.chars().next().unwrap_or('K')}" }
                                                div { class: "user-result-copy", strong { "{name}" } span { "@{handle}" } }
                                                span { class: "user-result-arrow", "→" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
