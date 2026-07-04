use serde::{Deserialize, Serialize};

/// Authenticated user information stored in the session.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthUser {
    pub id: String,
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}
