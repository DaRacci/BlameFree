use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "seaorm-storage", derive(crb_macros::FlattenedStruct))]
pub struct RemoteRepositoryMeta {
    /// The owner of the repository.
    pub owner: String,

    /// The name of the repository.
    pub name: String,

    /// The platform of the repository.
    pub platform: VCSPlatform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "seaorm-storage", derive(crb_macros::FlattenedStruct))]
pub struct GitRepositoryMeta {
    /// The path to the repository root on the local filesystem.
    pub repo_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum VCSPlatform {
    GitHub,
    Codeberg,
}

impl Default for VCSPlatform {
    fn default() -> Self {
        VCSPlatform::GitHub
    }
}

impl FromStr for VCSPlatform {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "GitHub" | "github" | "Github" => Ok(VCSPlatform::GitHub),
            "Codeberg" | "codeberg" => Ok(VCSPlatform::Codeberg),
            _ => Err(format!("unknown VCS platform: {s}")),
        }
    }
}

impl std::fmt::Display for VCSPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VCSPlatform::GitHub => write!(f, "GitHub"),
            VCSPlatform::Codeberg => write!(f, "Codeberg"),
        }
    }
}

impl RemoteRepositoryMeta {
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
        let repo_meta = RemoteRepositoryMeta {
            owner: "octocat".to_string(),
            name: "Hello-World".to_string(),
            platform: VCSPlatform::GitHub,
        };

        insta::assert_snapshot!(repo_meta.get_url(), @"github.com/octocat/Hello-World");
    }

    #[test]
    fn test_vcs_platform_from_str() {
        assert_eq!(
            "GitHub".parse::<VCSPlatform>().unwrap(),
            VCSPlatform::GitHub
        );
        assert_eq!(
            "github".parse::<VCSPlatform>().unwrap(),
            VCSPlatform::GitHub
        );
        assert_eq!(
            "Codeberg".parse::<VCSPlatform>().unwrap(),
            VCSPlatform::Codeberg
        );
        assert!("unknown".parse::<VCSPlatform>().is_err());
    }

    #[test]
    fn test_vcs_platform_display() {
        assert_eq!(VCSPlatform::GitHub.to_string(), "GitHub");
        assert_eq!(VCSPlatform::Codeberg.to_string(), "Codeberg");
    }
}
