use super::*;

fn search(window: &str) -> String {
    format!("?window={window}")
}

#[test]
fn window_kind_wire_literals_are_pinned() {
    assert_eq!(WindowKind::Main.as_str(), "main");
    assert_eq!(WindowKind::Onboarding.as_str(), "onboarding");
    assert_eq!(WindowKind::DataSync.as_str(), "data-sync");
    assert_eq!(WindowKind::DataTransfer.as_str(), "data-transfer");
    assert_eq!(WindowKind::SchemaDiff.as_str(), "schema-diff");
    assert_eq!(WindowKind::Backup.as_str(), "backup");
}

#[test]
fn legacy_aliases_are_pinned() {
    assert_eq!(
        LEGACY_MAIN_ALIASES,
        [
            "connection",
            "workflow",
            "dashboard",
            "settings",
            "docs",
            "new-connection"
        ]
    );
}

#[test]
fn explicit_window_values_map_to_their_kinds() {
    assert_eq!(resolve_window_kind(&search("onboarding")), WindowKind::Onboarding);
    assert_eq!(resolve_window_kind(&search("data-sync")), WindowKind::DataSync);
    assert_eq!(
        resolve_window_kind(&search("data-transfer")),
        WindowKind::DataTransfer
    );
    assert_eq!(resolve_window_kind(&search("schema-diff")), WindowKind::SchemaDiff);
    assert_eq!(resolve_window_kind(&search("backup")), WindowKind::Backup);
}

#[test]
fn legacy_unknown_missing_and_empty_values_all_fall_back_to_main() {
    for legacy in LEGACY_MAIN_ALIASES {
        assert_eq!(resolve_window_kind(&search(legacy)), WindowKind::Main, "{legacy}");
    }
    for other in ["unknown", "", "Onboarding", "data_sync"] {
        assert_eq!(resolve_window_kind(&search(other)), WindowKind::Main, "{other}");
    }
    assert_eq!(resolve_window_kind(""), WindowKind::Main);
    assert_eq!(resolve_window_kind("?other=1"), WindowKind::Main);
}

#[test]
fn get_url_param_decodes_and_returns_the_first_match() {
    assert_eq!(
        get_url_param("window", "?window=data-sync&window=backup"),
        Some("data-sync".to_owned())
    );
    assert_eq!(
        get_url_param("q", "?a=1&q=a%20b&z=3"),
        Some("a b".to_owned())
    );
    assert_eq!(get_url_param("q", "q=a+b"), Some("a b".to_owned()));
    assert_eq!(get_url_param("q", "?q=%zz"), Some("%zz".to_owned()));
    assert_eq!(get_url_param("q", "?q"), Some(String::new()));
    assert_eq!(get_url_param("q", "?other=1"), None);
    assert_eq!(get_url_param("q", ""), None);
}

#[test]
fn the_kind_is_cached_until_explicitly_reset() {
    reset_window_kind_cache();
    assert_eq!(get_window_kind(&search("backup")), WindowKind::Backup);
    assert_eq!(get_window_kind(&search("onboarding")), WindowKind::Backup);
    assert_eq!(get_window_kind("?window=main"), WindowKind::Backup);
    reset_window_kind_cache();
    assert_eq!(get_window_kind(&search("onboarding")), WindowKind::Onboarding);
    reset_window_kind_cache();
}

#[test]
fn crate_root_reexports_the_window_kind_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::WindowKind>();
    assert_eq!(crate::WindowKind::Main.as_str(), "main");
    assert_eq!(
        crate::resolve_window_kind(&search("backup")),
        crate::WindowKind::Backup
    );
    assert_eq!(crate::get_url_param("window", &search("backup")), Some("backup".to_owned()));
    assert_eq!(crate::LEGACY_MAIN_ALIASES.len(), 6);
}
