//! OAuth authentication routes and cookie-based session management.
//!
//! Sessions use a random token stored in a cookie, with user data kept in an in-memory store.
//! This avoids external session crate dependencies while remaining secure for a development dashboard.
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use axum::http::{StatusCode, header};
use riv_webui_shared::auth::AuthUser;
use oauth2::basic::BasicClient;
use oauth2::{AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
use reqwest::Client as HttpClient;
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

/// Build an [`oauth2::BasicClient`] for the given provider.
pub fn build_oauth_client(
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
