//! Guard the zh-CN table the rune views now read through `i18n::t`.
//!
//! The table is generated from the React locale packs at build time, so this
//! module does not pin every string (that would duplicate the packs). It pins
//! the wiring: the table is populated, the keys the views took over resolve to
//! real Chinese text instead of falling back to the key, and `common.save`
//! stays the reference translation. There is deliberately no "no Chinese
//! literal" rule here: later tracks move more strings over step by step, and a
//! blanket ban would block work that is still in flight.

use crate::i18n::{has, len, t, ZH_CN};

#[test]
fn the_zh_cn_table_is_generated_and_populated() {
    assert!(
        len() > 3_000,
        "the zh-CN table holds only {} entries",
        len()
    );
}

#[test]
fn keys_taken_over_by_the_rune_views_resolve() {
    // A sample of the keys the connection views now ask for. A typo or a pack
    // entry dropped in a later edit makes `t` fall back to the key itself, and
    // this assertion catches that here instead of showing a raw key in the UI.
    let keys = [
        "common.importConnections",
        "common.newConnection",
        "connShare.changeImportFile",
        "import.selectFile",
        "newConn.advanced",
        "newConn.connName",
        "newConn.database",
        "newConn.dbType",
        "newConn.password",
        "newConn.selectDbType",
        "newConn.sshHost",
        "newConn.sshPort",
        "newConn.sshUsername",
        "newConn.tunnel",
        "newConn.username",
        // Batch 2: settings sections, the rail, and the navigator title.
        "nav.databases",
        "nav.dashboard",
        "nav.extensions",
        "nav.settings",
        "nav.workflow",
        "nav.workspacePages",
        "settings.appearance",
        "settings.behavior",
        "settings.dataBrowsing",
        "settings.editor",
        "settings.general",
        "settings.logging",
        "settings.mcp.title",
        "settings.mcpClient.title",
        "settings.nav.group.app",
        "settings.nav.group.integration",
        "settings.prompts",
        "settings.title",
        "settings.tunnels.title",
        "common.aiAssistant",
        "query.favorites",
        "query.noFavorites",
        "backup.startBackup",
    ];
    for key in keys {
        assert_ne!(t(key), key, "{key} falls back to the key");
    }
}

#[test]
fn the_reference_translation_is_still_chinese() {
    assert_eq!(t("common.save"), "保存");
}

#[test]
fn an_unknown_key_falls_back_to_the_key() {
    // `t` never returns an empty string for a missing key: the UI shows the key
    // itself, which is how a missed pack entry becomes visible instead of a
    // silently blank label.
    let unknown = "datazen.i18n.guard.no_such_key";
    assert!(!has(unknown), "{unknown} unexpectedly exists in the packs");
    assert_eq!(t(unknown), unknown);
}

#[test]
fn the_zh_cn_table_has_no_undocumented_empty_values() {
    // `valueSearch.status.idle` is empty on purpose: it is a status line with
    // nothing to report yet (see `i18n.rs`). Everything else must carry text —
    // an empty value renders as a blank label or tooltip.
    let empties: Vec<&str> = ZH_CN
        .iter()
        .filter(|(_, value)| value.is_empty())
        .map(|(key, _)| *key)
        .collect();
    assert_eq!(empties, ["valueSearch.status.idle"]);
}

#[test]
fn the_two_connection_views_still_ask_for_the_keys_they_took_over() {
    // `every_key_the_ui_asks_for_exists` catches a typo in a call site, but not
    // a revert: hard-coding the label again removes the call and the scan stays
    // green (verified by mutation while auditing commit cb8e3f2b0). Pin the
    // keys in the two files this track converted.
    let connections = include_str!("../views/connections.rs");
    let modals = include_str!("../views/modals.rs");
    let files: [(&str, &str, &[&str]); 2] = [
        (
            "views/connections.rs",
            connections,
            &[
                "connShare.changeImportFile",
                "import.selectFile",
                "newConn.advanced",
                "newConn.connName",
                "newConn.database",
                "newConn.dbType",
                "newConn.password",
                "newConn.tunnel",
                "newConn.username",
                "newConn.sshHost",
                "newConn.sshPort",
                "newConn.sshUsername",
            ],
        ),
        (
            "views/modals.rs",
            modals,
            &[
                "common.importConnections",
                "common.newConnection",
                "newConn.selectDbType",
            ],
        ),
    ];
    for (file, source, keys) in files {
        for key in keys {
            let call = format!("t(\"{key}\")");
            assert!(
                source.contains(&call),
                "{file} no longer asks for {call}; update this guard if the wiring moved"
            );
        }
    }
}

#[test]
fn the_connection_form_renders_translated_labels() {
    // Render the real dialog and read the laid-out text: the connection name
    // label keeps its required marker next to the translated name, the database
    // field shows the pack text, and no key falls through to the screen.
    let mut runtime = super::runtime();
    runtime.state.open_connection_dialog();
    runtime.frame(0.0);
    assert!(
        runtime.find("connection-dialog").is_some(),
        "the connection dialog is laid out"
    );
    let texts: Vec<String> = runtime
        .elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect();
    for expected in ["连接名称 *", "数据库名"] {
        assert!(
            texts.iter().any(|text| text == expected),
            "the form does not show {expected:?}; it shows {texts:?}"
        );
    }
    assert!(
        !texts
            .iter()
            .any(|text| text.starts_with("newConn.") || text.starts_with("common.")),
        "a translation key fell through to the screen: {texts:?}"
    );
}

#[test]
fn the_settings_and_navigation_views_still_ask_for_their_keys() {
    // Pin the call sites, not just the table: reverting a label to its English
    // literal removes the `t(...)` call and the key scan stays green. The
    // settings nav reads its labels from `SETTINGS_SECTIONS`, so pin the keys
    // there and the two calls that resolve them.
    let settings = include_str!("../views/settings.rs");
    let settings_state = include_str!("../settings_state.rs");
    let types = include_str!("../types.rs");
    let navigation = include_str!("../views/navigation.rs");
    let files: [(&str, &str, &[&str]); 4] = [
        (
            "views/settings.rs",
            settings,
            &[
                "t(group_label)",
                "t(section.label_key)",
                "t(\"settings.general\")",
                "t(\"settings.appearance\")",
                "t(\"settings.dataBrowsing\")",
                "t(\"settings.editor\")",
                "t(\"settings.behavior\")",
                "t(\"settings.logging\")",
                "t(\"common.aiAssistant\")",
                "t(\"settings.prompts\")",
                "t(\"settings.mcp.title\")",
                "t(\"settings.mcpClient.title\")",
                "t(\"settings.tunnels.title\")",
                "t(\"settings.extensions.title\")",
                "t(\"settings.title\")",
                "t(\"query.favorites\")",
                "t(\"query.noFavorites\")",
                "t(\"common.save\")",
                "t(\"common.delete\")",
                "t(\"backup.startBackup\")",
            ],
        ),
        (
            "settings_state.rs",
            settings_state,
            &[
                "label_key: \"settings.general\"",
                "label_key: \"settings.appearance\"",
                "label_key: \"settings.dataBrowsing\"",
                "label_key: \"settings.editor\"",
                "label_key: \"settings.behavior\"",
                "label_key: \"settings.logging\"",
                "label_key: \"common.aiAssistant\"",
                "label_key: \"settings.prompts\"",
                "label_key: \"settings.mcp.title\"",
                "label_key: \"settings.mcpClient.title\"",
                "label_key: \"settings.tunnels.title\"",
                "label_key: \"settings.extensions.title\"",
                "\"settings.nav.group.app\"",
                "\"settings.nav.group.integration\"",
            ],
        ),
        (
            "types.rs",
            types,
            &[
                "t(\"nav.databases\")",
                "t(\"nav.workspacePages\")",
                "t(\"nav.settings\")",
                "t(\"nav.dashboard\")",
                "t(\"nav.workflow\")",
                "t(\"nav.extensions\")",
            ],
        ),
        (
            "views/navigation.rs",
            navigation,
            &["t(\"nav.databases\")"],
        ),
    ];
    for (file, source, needles) in files {
        for needle in needles {
            assert!(
                source.contains(needle),
                "{file} no longer contains {needle}; update this guard if the wiring moved"
            );
        }
    }
}
