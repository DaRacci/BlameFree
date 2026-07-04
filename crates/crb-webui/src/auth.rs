//! OAuth authentication routes and cookie-based session management.
//!
//! Sessions use a random token stored in a cookie, with user data kept
//! in an in-memory store. This avoids external session crate dependencies
//! while remaining secure for a development dashboard.
//!
//! Routes (only registered when OAuth is configured):
//! - `GET /auth/login`     -> redirect to provider's OAuth authorization page
//! - `GET /auth/callback`  -> handle provider callback, create session
//! - `GET /auth/logout`    -> clear session
//! - `GET /auth/me`        -> return current user (or 401)

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Redirect};
use axum::routing::get;
use axum::Router;
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use rand::distributions::Alphanumeric;
use rand::Rng;
use reqwest::Client as HttpClient;
use serde::Deserialize;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::OAuthConfig;
use crate::server::AppState;

/// Name of the session cookie.
const SESSION_COOKIE_NAME: &str = "crb-session";

pub use crb_shared::AuthUser;

/// In-memory session store mapping session tokens to user data.
pub type SessionStore = Arc<RwLock<HashMap<String, AuthUser>>>;

/// Query parameters for the login endpoint.
#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    /// Optional provider override (defaults to the configured provider).
    #[serde(default)]
    pub provider: Option<String>,
}

/// Query parameters for the OAuth callback.
#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

/// Create a new session store.
pub fn new_session_store() -> SessionStore {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Build the OAuth router. Called only when OAuth is configured.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", get(logout))
        .route("/auth/me", get(me))
}

/// Helper: convert a String error into an axum-compatible (StatusCode, String).
fn err_tuple(msg: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, msg.into())
}

/// GET /auth/login — redirect the user to the OAuth provider.
async fn login(
    State(state): State<AppState>,
    Query(query): Query<LoginQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    let oauth = state
        .config
        .oauth
        .as_ref()
        .ok_or_else(|| err_tuple("OAuth not configured"))?;

    // Use the configured provider (or override from query param)
    let provider = query.provider.as_deref().unwrap_or(&oauth.provider);

    let client = build_oauth_client(oauth, provider).map_err(err_tuple)?;

    // Generate a random CSRF state token
    let csrf_token = CsrfToken::new(random_string(32));

    // Build the authorization URL with scopes
    let scopes: Vec<Scope> = oauth.scopes.iter().map(|s| Scope::new(s.clone())).collect();

    let (auth_url, _csrf) = client.authorize_url(|| csrf_token).add_scopes(scopes).url();

    // Encode the CSRF state in the redirect so we can verify on callback
    let redirect_url = format!("{}&state={}", auth_url, _csrf.secret());

    Ok(Redirect::to(&redirect_url))
}

/// GET /auth/callback — exchange authorization code for user info and create session.
async fn callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Result<(HeaderMap, StatusCode), (StatusCode, String)> {
    let oauth = state
        .config
        .oauth
        .as_ref()
        .ok_or_else(|| err_tuple("OAuth not configured"))?;

    // Exchange the authorization code for an access token
    let client = build_oauth_client(oauth, &oauth.provider).map_err(err_tuple)?;

    let token_response = client
        .exchange_code(AuthorizationCode::new(query.code.clone()))
        .request_async(async_http_client)
        .await
        .map_err(|e| err_tuple(format!("Token exchange failed: {e}")))?;

    let access_token = token_response.access_token().secret().to_string();

    // Fetch user info from the provider
    let user = fetch_user(&oauth.provider, &access_token).await?;

    // Create a new session
    let session_token = Uuid::new_v4().to_string();
    state
        .session_store
        .write()
        .await
        .insert(session_token.clone(), user);

    // Set the session cookie and Location header for redirect
    let cookie_value =
        format!("{SESSION_COOKIE_NAME}={session_token}; Path=/; HttpOnly; SameSite=Lax");

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        cookie_value
            .parse()
            .map_err(|_| err_tuple("Invalid cookie header"))?,
    );
    headers.insert(
        axum::http::header::LOCATION,
        "/".parse()
            .map_err(|_| err_tuple("Invalid location header"))?,
    );

    Ok((headers, StatusCode::FOUND))
}

/// GET /auth/logout — clear the session.
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    // Extract session token from cookie
    if let Some(token) = extract_session_cookie(&headers) {
        state.session_store.write().await.remove(&token);
    }

    // Clear the cookie
    let clear_cookie = format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");

    let mut resp_headers = HeaderMap::new();
    if let Ok(val) = clear_cookie.parse() {
        resp_headers.insert(axum::http::header::SET_COOKIE, val);
    }

    (resp_headers, Redirect::to("/"))
}

/// GET /auth/me — return authenticated user info, or 401 if not logged in.
async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthUser>, StatusCode> {
    let session_token = extract_session_cookie(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let store = state.session_store.read().await;
    store
        .get(&session_token)
        .cloned()
        .ok_or(StatusCode::UNAUTHORIZED)
        .map(Json)
}

// ─── Cookie Helpers ──────────────────────────────────────────────────────────

/// Extract the session token from the Cookie header.
fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")) {
            return Some(value.to_string());
        }
    }
    None
}

// ─── OAuth Client Construction ───────────────────────────────────────────────

/// Build an `oauth2::BasicClient` for the given provider.
fn build_oauth_client(config: &OAuthConfig, provider: &str) -> Result<BasicClient, String> {
    let (auth_url_str, token_url_str) = match provider {
        "github" => (
            "https://github.com/login/oauth/authorize",
            "https://github.com/login/oauth/access_token",
        ),
        "google" => (
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://oauth2.googleapis.com/token",
        ),
        "gitlab" => (
            "https://gitlab.com/oauth/authorize",
            "https://gitlab.com/oauth/token",
        ),
        other => return Err(format!("Unsupported OAuth provider: {other}")),
    };

    let auth_url =
        AuthUrl::new(auth_url_str.to_string()).map_err(|e| format!("Invalid auth URL: {e}"))?;
    let token_url =
        TokenUrl::new(token_url_str.to_string()).map_err(|e| format!("Invalid token URL: {e}"))?;

    let client = BasicClient::new(
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(
        RedirectUrl::new(config.redirect_url.clone())
            .map_err(|e| format!("Invalid redirect URL: {e}"))?,
    );

    Ok(client)
}

// ─── Provider User Info Fetching ─────────────────────────────────────────────

/// Fetch the authenticated user's profile from the OAuth provider.
async fn fetch_user(provider: &str, access_token: &str) -> Result<AuthUser, (StatusCode, String)> {
    let http_client = HttpClient::new();

    match provider {
        "github" => {
            let resp = http_client
                .get("https://api.github.com/user")
                .header("Authorization", format!("Bearer {access_token}"))
                .header("User-Agent", "crb-webui/0.1.0")
                .send()
                .await
                .map_err(|e| {
                    (
                        StatusCode::BAD_GATEWAY,
                        format!("Failed to fetch GitHub user: {e}"),
                    )
                })?;

            if !resp.status().is_success() {
                return Err((
                    StatusCode::BAD_GATEWAY,
                    format!("GitHub API returned {}", resp.status()),
                ));
            }

            let body: serde_json::Value = resp.json().await.map_err(|e| {
                (
                    StatusCode::BAD_GATEWAY,
                    format!("Failed to parse GitHub response: {e}"),
                )
            })?;

            Ok(AuthUser {
                id: body["id"].to_string(),
                login: body["login"].as_str().unwrap_or("unknown").to_string(),
                name: body["name"].as_str().map(String::from),
                email: body["email"].as_str().map(String::from),
                avatar_url: body["avatar_url"].as_str().map(String::from),
            })
        }
        "google" => {
            let resp = http_client
                .get("https://www.googleapis.com/oauth2/v2/userinfo")
                .header("Authorization", format!("Bearer {access_token}"))
                .send()
                .await
                .map_err(|e| {
                    (
                        StatusCode::BAD_GATEWAY,
                        format!("Failed to fetch Google user: {e}"),
                    )
                })?;

            if !resp.status().is_success() {
                return Err((
                    StatusCode::BAD_GATEWAY,
                    format!("Google API returned {}", resp.status()),
                ));
            }

            let body: serde_json::Value = resp.json().await.map_err(|e| {
                (
                    StatusCode::BAD_GATEWAY,
                    format!("Failed to parse Google response: {e}"),
                )
            })?;

            Ok(AuthUser {
                id: body["id"].to_string(),
                login: body["email"].as_str().unwrap_or("unknown").to_string(),
                name: body["name"].as_str().map(String::from),
                email: body["email"].as_str().map(String::from),
                avatar_url: body["picture"].as_str().map(String::from),
            })
        }
        "gitlab" => {
            let resp = http_client
                .get("https://gitlab.com/api/v4/user")
                .header("Authorization", format!("Bearer {access_token}"))
                .send()
                .await
                .map_err(|e| {
                    (
                        StatusCode::BAD_GATEWAY,
                        format!("Failed to fetch GitLab user: {e}"),
                    )
                })?;

            if !resp.status().is_success() {
                return Err((
                    StatusCode::BAD_GATEWAY,
                    format!("GitLab API returned {}", resp.status()),
                ));
            }

            let body: serde_json::Value = resp.json().await.map_err(|e| {
                (
                    StatusCode::BAD_GATEWAY,
                    format!("Failed to parse GitLab response: {e}"),
                )
            })?;

            Ok(AuthUser {
                id: body["id"].to_string(),
                login: body["username"].as_str().unwrap_or("unknown").to_string(),
                name: body["name"].as_str().map(String::from),
                email: body["email"].as_str().map(String::from),
                avatar_url: body["avatar_url"].as_str().map(String::from),
            })
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            format!("Unsupported OAuth provider: {other}"),
        )),
    }
}

/// Generate a random alphanumeric string of the given length.
fn random_string(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}
