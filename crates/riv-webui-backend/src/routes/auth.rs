use crate::{
    auth::{CallbackQuery, LoginQuery, SESSION_COOKIE_NAME, build_oauth_client_urls, fetch_user},
    routes_register,
    server::AppState,
};
use axum::http::{StatusCode, header};
use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect},
};
use mti::prelude::{MagicTypeIdExt, V7};
use oauth2::{
    AuthorizationCode, CsrfToken, Scope, TokenResponse, basic::BasicClient, reqwest as oauth2_http,
};
use riv_shared::string::random_string;
use riv_stor::traits::Store;
use riv_webui_shared::{
    auth::AuthUser,
    routes::{AUTH_CALLBACK, AUTH_LOGIN, AUTH_LOGOUT, AUTH_ME},
};

routes_register! {
  get AUTH_ME => me,
  put AUTH_LOGIN => login,
  get AUTH_CALLBACK => callback,
  delete AUTH_LOGOUT => logout,
}

/// Redirect the user to the OAuth provider.
pub async fn login<S>(
    State(state): State<AppState<S>>,
    Query(query): Query<LoginQuery>,
) -> Result<Redirect, (StatusCode, String)>
where
    S: Store + Send + Sync + 'static,
{
    let oauth = state
        .config
        .oauth
        .as_ref()
        .ok_or_else(|| err_tuple("OAuth not configured"))?;

    let provider = query.provider.as_ref().unwrap_or(&oauth.provider);
    let (client_id, client_secret, auth_url, token_url, redirect_url) =
        build_oauth_client_urls(oauth, provider).map_err(|e| err_tuple(e.to_string()))?;
    let client = BasicClient::new(client_id)
        .set_client_secret(client_secret)
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_redirect_uri(redirect_url);
    let csrf_token = CsrfToken::new(random_string(32));
    let scopes: Vec<Scope> = oauth.scopes.iter().map(|s| Scope::new(s.clone())).collect();
    let (auth_url, _csrf) = client.authorize_url(|| csrf_token).add_scopes(scopes).url();
    let redirect_url = format!("{}&state={}", auth_url, _csrf.secret());

    Ok(Redirect::to(&redirect_url))
}

/// Exchange authorization code for user info and create session.
pub async fn callback<S>(
    State(state): State<AppState<S>>,
    Query(query): Query<CallbackQuery>,
) -> Result<(HeaderMap, StatusCode), (StatusCode, String)>
where
    S: Store + Send + Sync + 'static,
{
    let oauth = state
        .config
        .oauth
        .as_ref()
        .ok_or_else(|| err_tuple("OAuth not configured"))?;

    let (client_id, client_secret, auth_url, token_url, redirect_url) =
        build_oauth_client_urls(oauth, &oauth.provider).map_err(|e| err_tuple(e.to_string()))?;
    let client = BasicClient::new(client_id)
        .set_client_secret(client_secret)
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_redirect_uri(redirect_url);
    let token_response = client
        .exchange_code(AuthorizationCode::new(query.code.clone()))
        .request_async(&oauth2_http::Client::new())
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
pub async fn logout<S>(State(state): State<AppState<S>>, headers: HeaderMap) -> impl IntoResponse
where
    S: Store + Send + Sync + 'static,
{
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
pub async fn me<S>(
    State(state): State<AppState<S>>,
    headers: HeaderMap,
) -> Result<Json<AuthUser>, StatusCode>
where
    S: Store + Send + Sync + 'static,
{
    let session_token = extract_session_cookie(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let store = state.session_store.read().await;
    store
        .get(&session_token)
        .cloned()
        .ok_or(StatusCode::UNAUTHORIZED)
        .map(Json)
}

/// Convert a String error into an axum-compatible (StatusCode, String).
fn err_tuple(msg: impl Into<String>) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, msg.into())
}

/// Extract the session token from the Cookie header.
fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")) {
            if !value.trim().is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::COOKIE;
    use std::assert_matches;

    #[test]
    fn test_extract_session_cookie_valid() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("{SESSION_COOKIE_NAME}=abc123; other=val")
                .parse()
                .unwrap(),
        );
        let result = extract_session_cookie(&headers);
        assert_matches!(result, Some(token) if token == "abc123");
    }

    #[test]
    fn test_extract_session_cookie_missing() {
        let headers = HeaderMap::new();
        let result = extract_session_cookie(&headers);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_session_cookie_malformed() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, "other-session=abc123".parse().unwrap());
        let result = extract_session_cookie(&headers);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_session_cookie_empty_value() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, format!("{SESSION_COOKIE_NAME}=").parse().unwrap());
        let result = extract_session_cookie(&headers);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_session_cookie_trailing_semicolon() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("{SESSION_COOKIE_NAME}=xyz;").parse().unwrap(),
        );
        let result = extract_session_cookie(&headers);
        assert_eq!(result, Some("xyz".to_string()));
    }

    #[test]
    fn test_random_string_length() {
        const LENGTHS: [usize; 3] = [0, u8::MAX as usize, u16::MAX as usize];
        for &length in &LENGTHS {
            let s = random_string(length);
            assert_eq!(s.len(), length);
        }
    }
}
