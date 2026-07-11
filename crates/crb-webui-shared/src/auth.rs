use serde::{Deserialize, Serialize};

/// Authenticated user information stored in the session.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthUser {
    /// GitHub user ID.
    pub id: String,

    /// GitHub login name.
    pub login: String,

    /// Display name, if available.
    #[serde(default)]
    pub name: Option<String>,

    /// Email address, if available.
    #[serde(default)]
    pub email: Option<String>,

    /// URL to the user's avatar image.
    #[serde(default)]
    pub avatar_url: Option<String>,
}
