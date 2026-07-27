//! OAuth authentication routes and cookie-based session management.
//!
//! Sessions use a random token stored in a cookie, with user data kept in an in-memory store.
//! This avoids external session crate dependencies while remaining secure for a development dashboard.
use std::collections::HashMap;
use std::sync::Arc;

use axum::http::{StatusCode, header};
use oauth2::{AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use reqwest::Client as HttpClient;
use riv_webui_shared::auth::AuthUser;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumDiscriminants, IntoStaticStr, VariantArray};
use tokio::sync::RwLock;

use crate::config::OAuthConfig;

/// Name of the session cookie.
pub(crate) const SESSION_COOKIE_NAME: &str = "riv-session";

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

    #[allow(unused)]
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, IntoStaticStr, EnumDiscriminants, Display)]
#[strum_discriminants(derive(VariantArray))]
#[serde(rename_all = "lowercase")]
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
    pub fn auth_parameters(&self) -> (&str, &str, &str, &str, &str) {
        match self {
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
                id_field,
                login_field,
                name_field,
                email_field,
                avatar_url_field,
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
            format!("riv-webui/{}", env!("CARGO_PKG_VERSION")),
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
pub async fn fetch_user(
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

/// Build URLs and return components needed to construct an oauth2 client.
///
/// Callers must chain `.set_auth_uri(auth_url).set_token_uri(token_url).set_redirect_uri(redirect_url)`
/// on the returned client.
pub fn build_oauth_client_urls(
    config: &OAuthConfig,
    provider: &OAuthProvider,
) -> anyhow::Result<(ClientId, ClientSecret, AuthUrl, TokenUrl, RedirectUrl)> {
    let auth_url = AuthUrl::new(provider.auth_url())
        .map_err(|e| anyhow::anyhow!("Invalid authorization URL: {e}"))?;
    let token_url = TokenUrl::new(provider.token_url())
        .map_err(|e| anyhow::anyhow!("Invalid token URL: {e}"))?;
    let redirect_url = RedirectUrl::new(config.redirect_url.clone())
        .map_err(|e| anyhow::anyhow!("Invalid redirect URL: {e}"))?;

    let client_id = ClientId::new(config.client_id.clone());
    let client_secret = ClientSecret::new(config.client_secret.clone());

    Ok((client_id, client_secret, auth_url, token_url, redirect_url))
}
