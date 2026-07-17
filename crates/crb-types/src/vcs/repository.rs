use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryMeta {
    /// The owner of the repository.
    pub owner: String,

    /// The name of the repository.
    pub name: String,

    /// The platform of the repository.
    pub platform: VCSPlatform,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum VCSPlatform {
    GitHub,
    Codeberg,
}

impl RepositoryMeta {
    pub fn get_url(&self) -> String {
        match self.platform {
            VCSPlatform::GitHub => format!("github.com/{}/{}", self.owner, self.name),
            VCSPlatform::Codeberg => format!("codeberg.org/{}/{}", self.owner, self.name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_url() {
        let repo_meta = RepositoryMeta {
            owner: "octocat".to_string(),
            name: "Hello-World".to_string(),
            platform: VCSPlatform::GitHub,
        };

        insta::assert_snapshot!(repo_meta.get_url(), @"github.com/octocat/Hello-World");
    }
}
