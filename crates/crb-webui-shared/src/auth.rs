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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_user_default() {
        let user = AuthUser::default();
        insta::assert_debug_snapshot!(user);
    }

    #[test]
    fn test_auth_user_optional_fields_default() {
        // All option fields have #[serde(default)] and can be omitted
        let json = r#"{"id":"1","login":"user"}"#;
        let user: AuthUser = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(user);
    }

    #[test]
    fn test_auth_user_partial_optional_fields() {
        let json = r#"{"id":"2","login":"dev","name":"Developer","avatar_url":"https://example.com/avatar.png"}"#;
        let user: AuthUser = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(user);
    }
}
