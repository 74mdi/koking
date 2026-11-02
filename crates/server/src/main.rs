use std::{env, net::SocketAddr, str::FromStr, time::Duration};

use axum::{
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use chrono::{Duration as ChronoDuration, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    FromRow, SqlitePool,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

const SESSION_DAYS: i64 = 30;
const MAX_MESSAGE_LEN: usize = 4_000;

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
}

#[derive(Clone, FromRow)]
struct AuthUser {
    id: String,
    username: String,
    display_name: String,
}

#[derive(Debug)]
struct ApiError(StatusCode, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorBody { error: self.1 })).into_response()
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

type ApiResult<T> = Result<T, ApiError>;

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    display_name: Option<String>,
    password: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct PublicUser {
    id: String,
    username: String,
    display_name: String,
}

#[derive(Serialize)]
struct AuthResponse {
    token: String,
    user: PublicUser,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

#[derive(Deserialize)]
struct CreateConversation {
    username: String,
}

#[derive(Serialize)]
struct Conversation {
    id: String,
    peer: PublicUser,
    last_message: Option<String>,
    updated_at: Option<String>,
}

#[derive(Deserialize)]
struct MessageQuery {
    after: Option<i64>,
}

#[derive(Deserialize)]
struct SendMessage {
    body: String,
}

#[derive(Serialize, FromRow)]
struct Message {
    id: i64,
    conversation_id: String,
    sender_id: String,
    sender_name: String,
    body: String,
    created_at: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = env::var("KOKING_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://koking.db?mode=rwc".to_string());
    let db_options = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let db = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(db_options)
        .await?;
    migrate(&db).await?;

    let cors = env::var("KOKING_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://127.0.0.1:8080,http://localhost:8080".into())
        .split(',')
        .filter_map(|origin| origin.trim().parse().ok())
        .collect::<Vec<_>>();
    let cors = CorsLayer::new()
        .allow_origin(cors)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    let state = AppState { db };
    let private = Router::new()
        .route("/api/me", get(me))
        .route("/api/logout", post(logout))
        .route("/api/users", get(search_users))
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/api/conversations/{id}/messages",
            get(list_messages).post(send_message),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));

    let app = Router::new()
        .route(
            "/health",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .merge(private)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let address: SocketAddr = env::var("KOKING_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "Koking API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn migrate(db: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE COLLATE NOCASE,
            display_name TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS sessions (
            token_hash TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS sessions_user_id ON sessions(user_id);
        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE TABLE IF NOT EXISTS participants (
            conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            PRIMARY KEY (conversation_id, user_id)
        );
        CREATE INDEX IF NOT EXISTS participants_user_id ON participants(user_id);
        CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            sender_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 4000),
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );
        CREATE INDEX IF NOT EXISTS messages_conversation_id_id ON messages(conversation_id, id);
        "#,
    )
    .execute(db)
    .await?;
    Ok(())
}

async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let username = input.username.trim();
    if !(3..=24).contains(&username.len())
        || !username
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        || input.password.len() < 10
        || input.password.len() > 256
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "username or password does not meet requirements",
        ));
    }
    let display_name = input.display_name.unwrap_or_else(|| username.to_string());
    let display_name = display_name.trim();
    if display_name.is_empty() || display_name.chars().count() > 48 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "display name must be 1 to 48 characters",
        ));
    }

    use argon2::{
        password_hash::{rand_core::OsRng as PasswordOsRng, PasswordHasher, SaltString},
        Argon2,
    };
    let salt = SaltString::generate(&mut PasswordOsRng);
    let password_hash = Argon2::default()
        .hash_password(input.password.as_bytes(), &salt)
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not secure password",
            )
        })?
        .to_string();
    let user = PublicUser {
        id: uuid::Uuid::new_v4().to_string(),
        username: username.to_string(),
        display_name: display_name.to_string(),
    };
    let result = sqlx::query(
        "INSERT INTO users (id, username, display_name, password_hash) VALUES (?, ?, ?, ?)",
    )
    .bind(&user.id)
    .bind(&user.username)
    .bind(&user.display_name)
    .bind(password_hash)
    .execute(&state.db)
    .await;
    match result {
        Ok(_) => create_session(&state.db, user).await.map(Json),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(ApiError(StatusCode::CONFLICT, "username is already taken"))
        }
        Err(_) => Err(ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create account",
        )),
    }
}

async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    if input.username.trim().len() > 24 || input.password.len() > 256 {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "invalid username or password",
        ));
    }
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT id, username, display_name, password_hash FROM users WHERE username = ?",
    )
    .bind(input.username.trim())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not sign in"))?
    .ok_or(ApiError(
        StatusCode::UNAUTHORIZED,
        "invalid username or password",
    ))?;
    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };
    let parsed = PasswordHash::new(&row.3).map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "account password record is invalid",
        )
    })?;
    if Argon2::default()
        .verify_password(input.password.as_bytes(), &parsed)
        .is_err()
    {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "invalid username or password",
        ));
    }
    create_session(
        &state.db,
        PublicUser {
            id: row.0,
            username: row.1,
            display_name: row.2,
        },
    )
    .await
    .map(Json)
}

async fn create_session(db: &SqlitePool, user: PublicUser) -> ApiResult<AuthResponse> {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let now = Utc::now();
    let _ = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(now.to_rfc3339())
        .execute(db)
        .await;
    let expires_at = (now + ChronoDuration::days(SESSION_DAYS)).to_rfc3339();
    sqlx::query("INSERT INTO sessions(token_hash, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(hash_token(&token))
        .bind(&user.id)
        .bind(expires_at)
        .execute(db)
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create session",
            )
        })?;
    Ok(AuthResponse { token, user })
}

async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> ApiResult<Response> {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "sign in required"))?;
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "invalid session"));
    }
    let user = sqlx::query_as::<_, AuthUser>(
        "SELECT users.id, users.username, users.display_name FROM sessions JOIN users ON users.id = sessions.user_id WHERE sessions.token_hash = ? AND sessions.expires_at > ?",
    )
    .bind(hash_token(token)).bind(Utc::now().to_rfc3339())
    .fetch_optional(&state.db).await
    .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not validate session"))?
    .ok_or(ApiError(StatusCode::UNAUTHORIZED, "session expired; sign in again"))?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

async fn me(Extension(user): Extension<AuthUser>) -> Json<PublicUser> {
    Json(PublicUser {
        id: user.id,
        username: user.username,
        display_name: user.display_name,
    })
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<StatusCode> {
    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash_token(token))
            .execute(&state.db)
            .await
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not sign out"))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn search_users(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<Vec<PublicUser>>> {
    let query = query.q.unwrap_or_default().trim().to_string();
    if query.len() < 2 {
        return Ok(Json(Vec::new()));
    }
    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
    let users = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, username, display_name FROM users WHERE id != ? AND (username LIKE ? ESCAPE '\\' OR display_name LIKE ? ESCAPE '\\') ORDER BY username LIMIT 20",
    )
    .bind(user.id).bind(&pattern).bind(&pattern).fetch_all(&state.db).await
    .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not search users"))?;
    Ok(Json(
        users
            .into_iter()
            .map(|(id, username, display_name)| PublicUser {
                id,
                username,
                display_name,
            })
            .collect(),
    ))
}

async fn create_conversation(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<CreateConversation>,
) -> ApiResult<Json<Conversation>> {
    let peer = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, username, display_name FROM users WHERE username = ?",
    )
    .bind(input.username.trim())
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not find user"))?
    .ok_or(ApiError(StatusCode::NOT_FOUND, "user not found"))?;
    if peer.0 == user.id {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "you cannot message yourself",
        ));
    }

    let existing = sqlx::query_scalar::<_, String>(
        "SELECT p1.conversation_id FROM participants p1 JOIN participants p2 ON p2.conversation_id = p1.conversation_id WHERE p1.user_id = ? AND p2.user_id = ? LIMIT 1",
    ).bind(&user.id).bind(&peer.0).fetch_optional(&state.db).await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not create conversation"))?;
    let id = if let Some(id) = existing {
        id
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        let mut tx = state.db.begin().await.map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create conversation",
            )
        })?;
        sqlx::query("INSERT INTO conversations(id) VALUES (?)")
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(|_| {
                ApiError(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not create conversation",
                )
            })?;
        sqlx::query("INSERT INTO participants(conversation_id, user_id) VALUES (?, ?), (?, ?)")
            .bind(&id)
            .bind(&user.id)
            .bind(&id)
            .bind(&peer.0)
            .execute(&mut *tx)
            .await
            .map_err(|_| {
                ApiError(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not add conversation participants",
                )
            })?;
        tx.commit().await.map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not create conversation",
            )
        })?;
        id
    };
    Ok(Json(Conversation {
        id,
        peer: PublicUser {
            id: peer.0,
            username: peer.1,
            display_name: peer.2,
        },
        last_message: None,
        updated_at: None,
    }))
}

async fn list_conversations(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> ApiResult<Json<Vec<Conversation>>> {
    let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, Option<String>)>(
        r#"SELECT c.id, u.id, u.username, u.display_name, m.body, m.created_at
           FROM participants mine
           JOIN conversations c ON c.id = mine.conversation_id
           JOIN participants other ON other.conversation_id = c.id AND other.user_id != mine.user_id
           JOIN users u ON u.id = other.user_id
           LEFT JOIN messages m ON m.id = (SELECT id FROM messages WHERE conversation_id = c.id ORDER BY id DESC LIMIT 1)
           WHERE mine.user_id = ? ORDER BY COALESCE(m.id, 0) DESC"#,
    ).bind(user.id).fetch_all(&state.db).await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not load conversations"))?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, peer_id, username, display_name, last_message, updated_at)| Conversation {
                    id,
                    peer: PublicUser {
                        id: peer_id,
                        username,
                        display_name,
                    },
                    last_message,
                    updated_at,
                },
            )
            .collect(),
    ))
}

async fn list_messages(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(query): Query<MessageQuery>,
) -> ApiResult<Json<Vec<Message>>> {
    ensure_participant(&state.db, &id, &user.id).await?;
    let after = query.after.unwrap_or(0).max(0);
    let messages = sqlx::query_as::<_, Message>(
        "SELECT m.id, m.conversation_id, m.sender_id, u.display_name AS sender_name, m.body, m.created_at FROM messages m JOIN users u ON u.id = m.sender_id WHERE m.conversation_id = ? AND m.id > ? ORDER BY m.id LIMIT 200",
    ).bind(id).bind(after).fetch_all(&state.db).await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not load messages"))?;
    Ok(Json(messages))
}

async fn send_message(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(input): Json<SendMessage>,
) -> ApiResult<Json<Message>> {
    let body = input.body.trim();
    if body.is_empty() || body.chars().count() > MAX_MESSAGE_LEN {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "message must be 1 to 4000 characters",
        ));
    }
    ensure_participant(&state.db, &id, &user.id).await?;
    let result =
        sqlx::query("INSERT INTO messages(conversation_id, sender_id, body) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(&user.id)
            .bind(body)
            .execute(&state.db)
            .await
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not send message"))?;
    let message = sqlx::query_as::<_, Message>(
        "SELECT m.id, m.conversation_id, m.sender_id, u.display_name AS sender_name, m.body, m.created_at FROM messages m JOIN users u ON u.id = m.sender_id WHERE m.id = ?",
    ).bind(result.last_insert_rowid()).fetch_one(&state.db).await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "could not read sent message"))?;
    Ok(Json(message))
}

async fn ensure_participant(
    db: &SqlitePool,
    conversation_id: &str,
    user_id: &str,
) -> ApiResult<()> {
    let allowed = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM participants WHERE conversation_id = ? AND user_id = ?)",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_one(db)
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not check conversation access",
        )
    })?;
    if allowed == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "conversation not found"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn schema_migrates_and_session_tokens_are_hashed() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&db).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('users','sessions','conversations','participants','messages')")
            .fetch_one(&db).await.unwrap();
        assert_eq!(count, 5);
        assert_ne!(hash_token("secret"), "secret");
        assert_eq!(hash_token("secret"), hash_token("secret"));
    }

    #[tokio::test]
    async fn accounts_direct_messages_and_access_control_work() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&db).await.unwrap();
        let state = AppState { db };

        let Json(alice) = register(
            State(state.clone()),
            Json(RegisterRequest {
                username: "alice".into(),
                display_name: Some("Alice".into()),
                password: "a-long-test-password".into(),
            }),
        )
        .await
        .unwrap();
        let Json(bob) = register(
            State(state.clone()),
            Json(RegisterRequest {
                username: "bob".into(),
                display_name: Some("Bob".into()),
                password: "another-test-password".into(),
            }),
        )
        .await
        .unwrap();

        let Json(login_response) = login(
            State(state.clone()),
            Json(LoginRequest {
                username: "alice".into(),
                password: "a-long-test-password".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(login_response.user.id, alice.user.id);

        let alice_user = AuthUser {
            id: alice.user.id.clone(),
            username: alice.user.username.clone(),
            display_name: alice.user.display_name.clone(),
        };
        let Json(conversation) = create_conversation(
            State(state.clone()),
            Extension(alice_user.clone()),
            Json(CreateConversation {
                username: "bob".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(conversation.peer.id, bob.user.id);

        let Json(duplicate) = create_conversation(
            State(state.clone()),
            Extension(alice_user.clone()),
            Json(CreateConversation {
                username: "bob".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(duplicate.id, conversation.id);

        let Json(sent) = send_message(
            Path(conversation.id.clone()),
            State(state.clone()),
            Extension(alice_user.clone()),
            Json(SendMessage {
                body: "  hello Bob  ".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(sent.body, "hello Bob");

        let Json(history) = list_messages(
            Path(conversation.id.clone()),
            State(state.clone()),
            Extension(alice_user),
            Query(MessageQuery { after: None }),
        )
        .await
        .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, sent.id);

        let denied = ensure_participant(&state.db, &conversation.id, "not-a-member").await;
        assert_eq!(denied.unwrap_err().0, StatusCode::NOT_FOUND);
    }
}
