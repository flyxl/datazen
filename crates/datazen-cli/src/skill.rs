//! Official Agent Skill installation.
//!
//! The skill is guidance, not authority: it tells an agent how to call the
//! CLI, and never grants writes or asks for credentials.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{CliError, E_CONFIG};

pub const SKILL_NAME: &str = "datazen";
pub const SKILL_MARKDOWN: &str = include_str!("../assets/datazen/SKILL.md");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    pub status: &'static str,
    pub path: String,
}

pub fn skill_path(home: &Path) -> PathBuf {
    home.join(".agents")
        .join("skills")
        .join(SKILL_NAME)
        .join("SKILL.md")
}

/// Install into `$HOME/.agents/skills/datazen`. An existing user file is never
/// overwritten: the command reports `alreadyInstalled` and leaves it intact.
pub fn install(home: &Path) -> Result<InstallOutcome, CliError> {
    let path = skill_path(home);
    if path.exists() {
        return Ok(InstallOutcome {
            status: "alreadyInstalled",
            path: path.display().to_string(),
        });
    }
    let parent = path
        .parent()
        .ok_or_else(|| CliError::new(E_CONFIG, "invalid skill installation path"))?;
    std::fs::create_dir_all(parent).map_err(|error| {
        CliError::new(E_CONFIG, format!("cannot create skill directory: {error}"))
    })?;
    std::fs::write(&path, SKILL_MARKDOWN)
        .map_err(|error| CliError::new(E_CONFIG, format!("cannot write skill file: {error}")))?;
    Ok(InstallOutcome {
        status: "installed",
        path: path.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_once_and_never_overwrites_a_user_file() {
        let home = tempfile::tempdir().unwrap();
        let first = install(home.path()).unwrap();
        assert_eq!(first.status, "installed");
        let path = skill_path(home.path());
        assert!(path.is_file());

        std::fs::write(&path, "user-owned skill").unwrap();
        let second = install(home.path()).unwrap();
        assert_eq!(second.status, "alreadyInstalled");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "user-owned skill");
    }

    #[test]
    fn bundled_skill_has_frontmatter_and_read_only_guidance() {
        assert!(SKILL_MARKDOWN.starts_with("---\n"));
        assert!(SKILL_MARKDOWN.contains("name: datazen"));
        assert!(SKILL_MARKDOWN.contains("--allow-writes"));
        assert!(SKILL_MARKDOWN.contains("production"));
        assert!(!SKILL_MARKDOWN.to_ascii_lowercase().contains("password:"));
    }
}
