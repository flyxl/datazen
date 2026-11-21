use std::collections::HashMap;

use crate::types::wapp::WAPP_API_VERSION;

pub const THEME_TOKENS: [&str; 34] = [
    "--c-surface",
    "--c-surface-alt",
    "--c-surface-raised",
    "--c-surface-inset",
    "--c-edge",
    "--c-edge-hi",
    "--c-fg",
    "--c-fg-secondary",
    "--c-fg-muted",
    "--c-accent",
    "--c-accent-2",
    "--c-accent-deep",
    "--c-accent-dim",
    "--c-accent-ring",
    "--c-on-accent",
    "--c-success",
    "--c-warning",
    "--c-danger",
    "--c-query-run",
    "--c-titlebar",
    "--c-titlebar-fg",
    "--c-titlebar-fg-muted",
    "--c-titlebar-hover",
    "--dt-null",
    "--dt-bool",
    "--dt-number",
    "--dt-datetime",
    "--dt-json",
    "--dt-text",
    "--dt-binary",
    "--dt-dirty-border",
    "--dt-dirty-bg",
    "--dt-dirty-mark",
    "--dt-deleted-fg",
];

pub const THEME_SNAPSHOT_VERSION: u32 = WAPP_API_VERSION;

pub const THEME_TOKENS_DOM_GAP: [&str; 2] = [
    "document.documentElement.classList.contains('dark')",
    "getComputedStyle(document.documentElement).getPropertyValue",
];

pub struct ThemeSnapshotInput {
    pub dark: bool,
    pub style: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeSnapshot {
    pub v: u32,
    pub dark: bool,
    pub tokens: HashMap<String, String>,
}

pub fn build_theme_snapshot(input: &ThemeSnapshotInput) -> ThemeSnapshot {
    let mut tokens = HashMap::with_capacity(THEME_TOKENS.len());
    for name in THEME_TOKENS.iter() {
        let value = input
            .style
            .get(*name)
            .map(|value| value.trim())
            .unwrap_or("");
        tokens.insert((*name).to_owned(), value.to_owned());
    }
    ThemeSnapshot {
        v: THEME_SNAPSHOT_VERSION,
        dark: input.dark,
        tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_catalog_is_exactly_the_contract_list() {
        assert_eq!(THEME_TOKENS.len(), 34);
        assert_eq!(THEME_TOKENS[0], "--c-surface");
        assert_eq!(THEME_TOKENS[5], "--c-edge-hi");
        assert_eq!(THEME_TOKENS[23], "--dt-null");
        assert_eq!(THEME_TOKENS[33], "--dt-deleted-fg");
        let mut unique = std::collections::HashSet::new();
        for token in THEME_TOKENS.iter() {
            assert!(token.starts_with("--c-") || token.starts_with("--dt-"));
            assert!(unique.insert(*token), "{token} is duplicated");
        }
        assert_eq!(unique.len(), 34);
    }

    #[test]
    fn snapshot_version_matches_the_wapp_api_version() {
        assert_eq!(THEME_SNAPSHOT_VERSION, WAPP_API_VERSION);
        assert_eq!(THEME_SNAPSHOT_VERSION, 2);
    }

    #[test]
    fn snapshot_reads_every_token_trimmed_and_ignores_extras() {
        let mut style = HashMap::new();
        style.insert("--c-surface".to_owned(), "  #0b0e14  ".to_owned());
        style.insert("--dt-dirty-mark".to_owned(), "\t rgba(1,2,3) \n".to_owned());
        style.insert("--not-a-token".to_owned(), "ignored".to_owned());
        let snapshot = build_theme_snapshot(&ThemeSnapshotInput { dark: true, style });
        assert_eq!(snapshot.v, 2);
        assert!(snapshot.dark);
        assert_eq!(snapshot.tokens.len(), 34);
        assert_eq!(snapshot.tokens["--c-surface"], "#0b0e14");
        assert_eq!(snapshot.tokens["--dt-dirty-mark"], "rgba(1,2,3)");
        assert_eq!(snapshot.tokens["--c-accent"], "");
        assert!(!snapshot.tokens.contains_key("--not-a-token"));
    }

    #[test]
    fn light_snapshot_keeps_the_dark_flag_false() {
        let snapshot = build_theme_snapshot(&ThemeSnapshotInput {
            dark: false,
            style: HashMap::new(),
        });
        assert!(!snapshot.dark);
        assert!(snapshot.tokens.values().all(|value| value.is_empty()));
        assert_eq!(THEME_TOKENS_DOM_GAP.len(), 2);
        assert_eq!(crate::THEME_TOKENS_DOM_GAP.len(), 2);
    }
}
