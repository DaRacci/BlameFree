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

use anyhow::{Context, anyhow};
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Json, Redirect};
use axum::routing::get;
use crb_webui_shared::auth::AuthUser;
use crb_webui_shared::routes;
use mti::prelude::{MagicTypeIdExt, V7};
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use rand::Rng;
use rand::distributions::Alphanumeric;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumDiscriminants, IntoStaticStr, VariantArray};
use tokio::sync::RwLock;

use crate::config::OAuthConfig;
use crate::server::AppState;

/// Name of the session cookie.
const SESSION_COOKIE_NAME: &str = "riv-session";

/// In-memory session store mapping session tokens to user data.
pub type SessionStore = Arc<RwLock<HashMap<String, AuthUser>>>;

/// Query parameters for the login endpoint.
#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    /// Optional provider override
    #[serde(default)]
    pub provider: Option<OAuthProvider>,
}

/// Query parameters for the OAuth callback.
#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoStaticStr, EnumDiscriminants, Display)]
#[strum_discriminants(derive(VariantArray))]
pub enum OAuthProvider {
    GitHub,
    Google,
    GitLab,
    Custom {
        auth_url: String,
        token_url: String,
        user_url: String,
        id_field: String,
        login_field: String,
        name_field: String,
        email_field: String,
        avatar_url_field: String,
    },
}

impl OAuthProvider {
    /// Return the OAuth parameter names for the given provider.
    ///
    /// These are used to extract user information from the provider's API response.
    ///
    /// The order of the returned tuple is: (id, login, name, email, avatar_url).
    pub fn auth_parameters(
        &self,
    ) -> (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    ) {
        match provider {
            OAuthProvider::GitHub => ("id", "login", "name", "email", "avatar_url"),
            OAuthProvider::Google => ("id", "email", "name", "email", "picture"),
            OAuthProvider::GitLab => ("id", "username", "name", "email", "avatar_url"),
            OAuthProvider::Custom {
                id_field,
                login_field,
                name_field,
                email_field,
                avatar_url_field,
                ..
            } => (
                &id_field,
                &login_field,
                &name_field,
                &email_field,
                &avatar_url_field,
            ),
        }
    }

    pub fn base_url(&self) -> String {
        const GITHUB_BASE_URL: &str = "api.github.com";
        const GOOGLE_BASE_URL: &str = "www.googleapis.com";
        const GITLAB_BASE_URL: &str = "gitlab.com";

        match self {
            OAuthProvider::GitHub => GITHUB_BASE_URL,
            OAuthProvider::Google => GOOGLE_BASE_URL,
            OAuthProvider::GitLab => GITLAB_BASE_URL,
            OAuthProvider::Custom { .. } => "",
        }
        .to_string()
    }

    pub fn auth_url(&self) -> String {
        const GITHUB_AUTH_URL: &str = "login/oauth/authorize";
        const GOOGLE_AUTH_URL: &str = "o/oauth2/v2/auth";
        const GITLAB_AUTH_URL: &str = "oauth/authorize";

        let base = self.base_url();
        let path = match self {
            OAuthProvider::GitHub => GITHUB_AUTH_URL,
            OAuthProvider::Google => GOOGLE_AUTH_URL,
            OAuthProvider::GitLab => GITLAB_AUTH_URL,
            OAuthProvider::Custom { auth_url, .. } => auth_url,
        };

        match base.is_empty() {
            true => path.to_string(),
            false => format!("https://{}/{}", base, path),
        }
    }

    pub fn token_url(&self) -> String {
        const GITHUB_TOKEN_URL: &str = "login/oauth/access_token";
        const GOOGLE_TOKEN_URL: &str = "oauth2/v4/token";
        const GITLAB_TOKEN_URL: &str = "oauth/token";

        let base = self.base_url();
        let path = match self {
            OAuthProvider::GitHub => GITHUB_TOKEN_URL,
            OAuthProvider::Google => GOOGLE_TOKEN_URL,
            OAuthProvider::GitLab => GITLAB_TOKEN_URL,
            OAuthProvider::Custom { token_url, .. } => token_url,
        };

        match base.is_empty() {
            true => path.to_string(),
            false => format!("https://{}/{}", base, path),
        }
    }

    pub fn callback_url(&self) -> String {
        const GITHUB_CALLBACK_URL: &str = "auth/callback";
        const GOOGLE_CALLBACK_URL: &str = "auth/callback";
        const GITLAB_CALLBACK_URL: &str = "auth/callback";

        let base = self.base_url();
        let path = match self {
            OAuthProvider::GitHub => GITHUB_CALLBACK_URL,
            OAuthProvider::Google => GOOGLE_CALLBACK_URL,
            OAuthProvider::GitLab => GITLAB_CALLBACK_URL,
            OAuthProvider::Custom { auth_url, .. } => auth_url,
        };

        match base.is_empty() {
            true => path.to_string(),
            false => format!("https://{}/{}", base, path),
        }
    }

    pub fn user_url(&self) -> String {
        const GITHUB_PATH: &str = "user";
        const GOOGLE_PATH: &str = "oauth2/v2/userinfo";
        const GITLAB_PATH: &str = "api/v4/user";

        let base = self.base_url();
        let path = match self {
            OAuthProvider::GitHub => GITHUB_PATH,
            OAuthProvider::Google => GOOGLE_PATH,
            OAuthProvider::GitLab => GITLAB_PATH,
            OAuthProvider::Custom { user_url, .. } => user_url,
        };

        match base.is_empty() {
            true => path.to_string(),
            false => format!("https://{}/{}", base, path),
        }
    }
}

/// Create a new session store.
pub fn new_session_store() -> SessionStore {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Convert a String error into an axum-compatible (StatusCode, String).
fn err_tuple(msg: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, msg.into())
}

/// Redirect the user to the OAuth provider.
pub async fn login(
    State(state): State<AppState>,
    Query(query): Query<LoginQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    let oauth = state
        .config
        .oauth
        .as_ref()
        .ok_or_else(|| err_tuple("OAuth not configured"))?;

    let provider = query.provider.as_ref().unwrap_or(&oauth.provider);
    let client = build_oauth_client(oauth, provider).map_err(err_tuple)?;
    let csrf_token = CsrfToken::new(random_string(32));
    let scopes: Vec<Scope> = oauth.scopes.iter().map(|s| Scope::new(s.clone())).collect();
    let (auth_url, _csrf) = client.authorize_url(|| csrf_token).add_scopes(scopes).url();
    let redirect_url = format!("{}&state={}", auth_url, _csrf.secret());

    Ok(Redirect::to(&redirect_url))
}

/// Exchange authorization code for user info and create session.
pub async fn callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Result<(HeaderMap, StatusCode), (StatusCode, String)> {
    let oauth = state
        .config
        .oauth
        .as_ref()
        .ok_or_else(|| err_tuple("OAuth not configured"))?;

    let client = build_oauth_client(oauth, &oauth.provider).map_err(err_tuple)?;
    let token_response = client
        .exchange_code(AuthorizationCode::new(query.code.clone()))
        .request_async(async_http_client)
        .await
        .map_err(|e| err_tuple(format!("Token exchange failed: {e}")))?;

    let access_token = token_response.access_token().secret().to_string();
    let user = fetch_user(&oauth.provider, &access_token).await?;
    let session_token = "session".create_type_id::<V7>().to_string();

    state
        .session_store
        .write()
        .await
        .insert(session_token.clone(), user);

    let cookie_value =
        format!("{SESSION_COOKIE_NAME}={session_token}; Path=/; HttpOnly; SameSite=Lax");

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        cookie_value
            .parse()
            .map_err(|_| err_tuple("Invalid cookie header"))?,
    );
    headers.insert(
        header::LOCATION,
        "/".parse()
            .map_err(|_| err_tuple("Invalid location header"))?,
    );

    Ok((headers, StatusCode::FOUND))
}

/// Clear the session.
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(token) = extract_session_cookie(&headers) {
        state.session_store.write().await.remove(&token);
    }

    let clear_cookie = format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    let mut resp_headers = HeaderMap::new();
    if let Ok(val) = clear_cookie.parse() {
        resp_headers.insert(header::SET_COOKIE, val);
    }

    (resp_headers, Redirect::to("/"))
}

/// Return authenticated user info, or 401 if not logged in.
pub async fn me(
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

/// Extract the session token from the Cookie header.
fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")) {
            return Some(value.to_string());
        }
    }
    None
}

/// Build an [`oauth2::BasicClient`] for the given provider.
fn build_oauth_client(
    config: &OAuthConfig,
    provider: &OAuthProvider,
) -> anyhow::Result<BasicClient> {
    let auth_url = provider.auth_url();
    let token_url = provider.token_url();

    let auth_url = AuthUrl::new(auth_url.to_string()).context("Invalid authorization URL")?;
    let token_url = TokenUrl::new(token_url.to_string()).context("Invalid token URL")?;
    let redirect_url = RedirectUrl::new(config.redirect_url.clone())
        .context(format!("Invalid redirect URL: {}", config.redirect_url))?;

    let client = BasicClient::new(
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(redirect_url);

    Ok(client)
}

/// Fetch JSON from an OAuth provider endpoint,
/// checking for success and returning the parsed [`serde_json::Value`].
async fn fetch_oauth_json(
    url: &str,
    access_token: &str,
    provider: &OAuthProvider,
) -> Result<serde_json::Value, (StatusCode, String)> {
    let http_client = HttpClient::new();
    let resp = http_client
        .get(url)
        .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
        .header(
            header::USER_AGENT,
            format!("crb-webui/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Failed to fetch {provider} user: {e}"),
            )
        })?;

    if !resp.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("{provider} API returned {}", resp.status()),
        ));
    }

    resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to parse {provider} response: {e}"),
        )
    })
}

/// Fetch the authenticated user's profile from the OAuth provider.
async fn fetch_user(
    provider: &OAuthProvider,
    access_token: &str,
) -> Result<AuthUser, (StatusCode, String)> {
    let provider_name = provider.into();
    let url = provider.user_url();
    let body = fetch_oauth_json(&url, access_token, provider_name).await?;
    let (id, login, name, email, avatar_url) = provider.auth_parameters();

    Ok(AuthUser {
        id: body[id].to_string(),
        login: body[login].as_str().unwrap_or("unknown").to_string(),
        name: body[name].as_str().map(String::from),
        email: body[email].as_str().map(String::from),
        avatar_url: body[avatar_url].as_str().map(String::from),
    })
}

/// Generate a random alphanumeric string of the given length.
fn random_string(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::COOKIE;

    #[test]
    fn test_extract_session_cookie_valid() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, "crb-session=abc123; other=val".parse().unwrap());
        insta::assert_debug_snapshot!(extract_session_cookie(&headers));
    }

    #[test]
    fn test_extract_session_cookie_valid_first_position() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, "crb-session=token123".parse().unwrap());
        insta::assert_debug_snapshot!(extract_session_cookie(&headers));
    }

    #[test]
    fn test_extract_session_cookie_missing() {
        let headers = HeaderMap::new();
        insta::assert_debug_snapshot!(extract_session_cookie(&headers));
    }

    #[test]
    fn test_extract_session_cookie_malformed() {
        let mut headers = HeaderMap::new();
        // Wrong cookie name
        headers.insert(COOKIE, "other-session=abc123".parse().unwrap());
        insta::assert_debug_snapshot!(extract_session_cookie(&headers));
    }

    #[test]
    fn test_extract_session_cookie_empty_value() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, "crb-session=".parse().unwrap());
        insta::assert_debug_snapshot!(extract_session_cookie(&headers));
    }

    #[test]
    fn test_extract_session_cookie_trailing_semicolon() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, "crb-session=xyz;".parse().unwrap());
        insta::assert_debug_snapshot!(extract_session_cookie(&headers));
    }

    #[test]
    fn test_extract_session_cookie_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            "foo=bar; crb-session=hello; baz=qux".parse().unwrap(),
        );
        insta::assert_debug_snapshot!(extract_session_cookie(&headers));
    }

    #[test]
    fn test_build_oauth_client_github() {
        let config = OAuthConfig {
            provider: OAuthProvider::GitHub,
            client_id: "test-client-id".to_string(),
            client_secret: "test-client-secret".to_string(),
            redirect_url: "http://localhost:8080/auth/callback".to_string(),
            scopes: vec!["read:user".to_string()],
        };
        let client = build_oauth_client(&config, &OAuthProvider::GitHub);
        insta::assert_debug_snapshot!(client.is_ok());
    }

    #[test]
    fn test_build_oauth_client_google() {
        let config = OAuthConfig {
            provider: OAuthProvider::Google,
            client_id: "test-client-id".to_string(),
            client_secret: "test-client-secret".to_string(),
            redirect_url: "http://localhost:8080/auth/callback".to_string(),
            scopes: vec!["openid".to_string()],
        };
        let client = build_oauth_client(&config, &OAuthProvider::Google);
        insta::assert_debug_snapshot!(client.is_ok());
    }

    #[test]
    fn test_build_oauth_client_gitlab() {
        let config = OAuthConfig {
            provider: OAuthProvider::GitLab,
            client_id: "test-client-id".to_string(),
            client_secret: "test-client-secret".to_string(),
            redirect_url: "http://localhost:8080/auth/callback".to_string(),
            scopes: vec!["read_user".to_string()],
        };
        let client = build_oauth_client(&config, &OAuthProvider::GitLab);
        insta::assert_debug_snapshot!(client.is_ok());
    }

    #[test]
    fn test_build_oauth_client_invalid_redirect_url() {
        let config = OAuthConfig {
            provider: OAuthProvider::GitHub,
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            redirect_url: String::new(), // empty URL → invalid
            scopes: vec![],
        };
        let result = build_oauth_client(&config, &OAuthProvider::GitHub);
        insta::assert_debug_snapshot!(result.is_err());
    }

    #[test]
    fn test_random_string_length() {
        let s = random_string(32);
        insta::assert_debug_snapshot!(s.len());
    }

    #[test]
    fn test_random_string_zero_length() {
        let s = random_string(0);
        insta::assert_debug_snapshot!(s.len());
    }

    #[test]
    fn test_random_string_alphanumeric() {
        let s = random_string(100);
        insta::assert_debug_snapshot!(s.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
