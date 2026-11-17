//! T0-03 batch 3: the sweep guard.
//!
//! Two independent checks:
//! 1. A reverse index over the generated zh-CN table: no literal in the migrated
//!    views may silently equal a translated value while bypassing `t()`. A
//!    literal that really is an identifier/fixture/runtime probe goes into
//!    [`ALLOW`] with a reason.
//! 2. A call-site pin: each key this batch wired must still be asked for in the
//!    file that carries it, so a later revert to a hard-coded string fails here.

use crate::i18n::ZH_CN;

/// The migrated files, addressed by the same relative path the test uses.
const FILES: &[(&str, &str)] = &[
    (
        "../views/connections.rs",
        include_str!("../views/connections.rs"),
    ),
    ("../views/modals.rs", include_str!("../views/modals.rs")),
    (
        "../views/workspace.rs",
        include_str!("../views/workspace.rs"),
    ),
    (
        "../views/connections_home.rs",
        include_str!("../views/connections_home.rs"),
    ),
    (
        "../views/navigation.rs",
        include_str!("../views/navigation.rs"),
    ),
    ("../views/chrome.rs", include_str!("../views/chrome.rs")),
    (
        "../views/structure.rs",
        include_str!("../views/structure.rs"),
    ),
    ("../views/settings.rs", include_str!("../views/settings.rs")),
    (
        "../views/tools_ops.rs",
        include_str!("../views/tools_ops.rs"),
    ),
    (
        "../views/tools_sync.rs",
        include_str!("../views/tools_sync.rs"),
    ),
    ("../data_grid/view.rs", include_str!("../data_grid/view.rs")),
    (
        "../sql_editor/view.rs",
        include_str!("../sql_editor/view.rs"),
    ),
    ("../ui/nav.rs", include_str!("../ui/nav.rs")),
];

/// (file fragment + literal, reason). Every entry is a string that equals a
/// zh-CN table value but must stay literal in this batch.
const ALLOW: &[(&str, &str)] = &[
    (
        "views/connections.rs:失败",
        "matches the runtime notice/progress text, not a displayed label; a notice from the host is not UI copy",
    ),
];

fn literals(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // A `//` outside a string starts a comment.
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            break;
        }
        if bytes[i] == b'"' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'"' {
                if bytes[j] == b'\\' {
                    j += 2;
                } else {
                    j += 1;
                }
            }
            if j < bytes.len() {
                out.push(line[i + 1..j].to_owned());
                i = j + 1;
                continue;
            }
        }
        // Raw string: r"…", r#"…"#, r##"…"## (the literal body follows the quotes).
        if bytes[i] == b'r' && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] != b'"' {
                j += 1;
            }
            if j < bytes.len() {
                out.push(line[i + 2..j].to_owned());
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn allowed(file: &str, literal: &str) -> Option<&'static str> {
    ALLOW
        .iter()
        .find(|(needle, _)| {
            let (needle_file, needle_literal) = needle.split_once(':').unwrap_or((needle, ""));
            file.contains(needle_file) && literal == needle_literal
        })
        .map(|(_, reason)| *reason)
}

#[test]
fn no_migrated_view_bypasses_the_translation_table() {
    assert!(ZH_CN.len() > 3_000, "the table is missing: {}", ZH_CN.len());
    let mut failures = Vec::new();
    for (file, source) in FILES {
        for (index, line) in source.lines().enumerate() {
            // A `t(...)` call marks the migrated form; anything else is a suspect.
            if line.contains("t(\"") {
                continue;
            }
            for literal in literals(line) {
                if literal.is_empty() {
                    continue;
                }
                if !ZH_CN.iter().any(|(_, value)| *value == literal) {
                    continue;
                }
                if allowed(file, &literal).is_some() {
                    continue;
                }
                failures.push(format!(
                    "{}:{}: \"{}\" 可用 key \"{}\"",
                    file,
                    index + 1,
                    literal,
                    ZH_CN
                        .iter()
                        .find(|(_, value)| *value == literal)
                        .map(|(key, _)| *key)
                        .unwrap_or("?")
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} unwrapped literal(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Every key this batch wired, pinned in the file that must still ask for it.
const PINNED: &[(&str, &str)] = &[
    ("../views/workspace.rs", "connWin.data"),
    ("../views/workspace.rs", "connWin.indexes"),
    ("../views/workspace.rs", "query.running"),
    ("../views/workspace.rs", "query.run"),
    ("../views/workspace.rs", "common.cancel"),
    ("../views/workspace.rs", "query.format"),
    ("../views/workspace.rs", "query.explainError"),
    ("../views/workspace.rs", "query.beginTx"),
    ("../views/workspace.rs", "query.commitTx"),
    ("../views/workspace.rs", "query.rollbackTx"),
    ("../views/workspace.rs", "query.noFavorites"),
    ("../views/workspace.rs", "chart.viewTable"),
    ("../views/workspace.rs", "chart.viewChart"),
    ("../views/workspace.rs", "select.noMatches"),
    ("../views/workspace.rs", "resultFind.openLabel"),
    ("../views/workspace.rs", "main.ready"),
    ("../views/connections_home.rs", "conn.connecting"),
    (
        "../views/connections_home.rs",
        "connWin.home.selectConnectionTitle",
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.filter",
    ),
    ("../views/connections_home.rs", "common.importConnections"),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.open",
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.connect",
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.showLess",
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.showAll",
    ),
    ("../views/connections_home.rs", "common.copy"),
    ("../views/connections_home.rs", "keymap.action.formatSql"),
    ("../views/connections_home.rs", "conn.connected"),
    (
        "../views/connections_home.rs",
        "connWin.home.status.offline",
    ),
    ("../views/navigation.rs", "select.noMatches"),
    ("../views/navigation.rs", "query.objectTree.schema"),
    ("../views/navigation.rs", "query.objectTree.noObjects"),
    ("../views/navigation.rs", "main.ctx.disconnect"),
    ("../views/chrome.rs", "conn.connected"),
    ("../views/structure.rs", "common.close"),
    ("../views/structure.rs", "structView.fieldName"),
    ("../views/structure.rs", "structView.type"),
    ("../views/structure.rs", "structView.nullable"),
    ("../views/structure.rs", "filter.apply"),
    ("../views/structure.rs", "indexes.colName"),
    ("../views/structure.rs", "indexes.columns"),
    ("../views/structure.rs", "indexes.colUnique"),
    ("../views/settings.rs", "filter.view.rename"),
    ("../views/settings.rs", "common.stop"),
    ("../views/settings.rs", "import.import"),
    ("../views/settings.rs", "sync.next"),
    ("../views/settings.rs", "onboarding.common.finish"),
    ("../views/settings.rs", "common.back"),
    ("../views/settings.rs", "export.export"),
    ("../views/tools_ops.rs", "common.search"),
    ("../views/tools_ops.rs", "common.cancel"),
    ("../views/tools_ops.rs", "valueSearch.status.running"),
    ("../views/tools_ops.rs", "common.confirm"),
    ("../views/tools_sync.rs", "sync.compare"),
    ("../views/tools_sync.rs", "sync.next"),
    ("../views/tools_sync.rs", "filter.clear"),
    ("../views/tools_sync.rs", "common.preview"),
    ("../data_grid/view.rs", "query.gridReadOnly.tooltip"),
    ("../data_grid/view.rs", "common.loading"),
    ("../data_grid/view.rs", "tableData.commit"),
    ("../data_grid/view.rs", "tableData.rollback"),
    ("../sql_editor/view.rs", "resultFind.replaceNext"),
];

#[test]
fn every_wired_key_is_still_asked_for_in_its_file() {
    for (file, key) in PINNED {
        let source = FILES
            .iter()
            .find(|(path, _)| path == file)
            .map(|(_, source)| *source)
            .unwrap_or_else(|| panic!("{file} is not in FILES"));
        assert!(
            source.contains(&format!("t(\"{key}\")")),
            "{file} no longer asks for t(\"{key}\"); update this guard if the wiring moved"
        );
    }
}
