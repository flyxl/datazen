pub const WINDOW_CAPABILITY_LABEL_SAMPLES: [&str; 6] = [
    "main",
    "data-sync-singleton",
    "data-transfer-singleton",
    "schema-diff-singleton",
    "backup-singleton",
    "backup-restore-singleton",
];

pub const PENDING_CONNECTION_KEY: &str = "datazen:pending-connection";
pub const POST_ONBOARDING_SAMPLE_KEY: &str = "datazen:post-onboarding-sample";

pub const MAIN_SHELL_MENU_IDS: [&str; 13] = [
    "open-settings",
    "new-connection",
    "workflow",
    "dashboard",
    "export-config",
    "import-config",
    "export-connections",
    "import-connections-file",
    "import-connections-dbx",
    "import-connections-navicat",
    "import-connections-datagrip",
    "import-connections-dbeaver",
    "import-connections-tableplus",
];

pub fn is_main_shell_menu_id(id: &str) -> bool {
    MAIN_SHELL_MENU_IDS.contains(&id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostOnboardingSample {
    pub connection_name: String,
    pub sql: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_labels_and_storage_keys_are_pinned() {
        assert_eq!(WINDOW_CAPABILITY_LABEL_SAMPLES.len(), 6);
        assert_eq!(WINDOW_CAPABILITY_LABEL_SAMPLES[0], "main");
        assert_eq!(
            WINDOW_CAPABILITY_LABEL_SAMPLES[5],
            "backup-restore-singleton"
        );
        assert_eq!(PENDING_CONNECTION_KEY, "datazen:pending-connection");
        assert_eq!(POST_ONBOARDING_SAMPLE_KEY, "datazen:post-onboarding-sample");
    }

    #[test]
    fn only_declared_menu_ids_belong_to_the_main_shell() {
        assert_eq!(MAIN_SHELL_MENU_IDS.len(), 13);
        assert!(is_main_shell_menu_id("open-settings"));
        assert!(is_main_shell_menu_id("import-connections-tableplus"));
        assert!(!is_main_shell_menu_id("menu:workflow"));
        assert!(!is_main_shell_menu_id("unknown"));
    }
}
