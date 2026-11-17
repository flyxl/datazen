//! Independent audit for the T0-03 batch 3 translation sweep (`62c4e00ae`).
//!
//! The batch's own guard (`crates/datazen-ui/src/tests/i18n_sweep.rs`) skips every
//! line whose text contains a translation-call opener. That substring is also a
//! tail of `text(`, so display copy drawn through `text(...)` bypasses its
//! reverse index entirely. This audit parses the literals itself and skips only
//! the literal that really is an argument of a translation call, so `text(…)`
//! stays under the guard.
//!
//! The ignored tests below record the batch-3 gaps that still need a fix; they
//! are turned on when the coder lands it.

use crate::i18n::ZH_CN;

/// The migrated files, addressed by the same relative path the sweep uses.
const VIEWS: &[(&str, &str)] = &[
    ("../views/connections.rs", include_str!("../views/connections.rs")),
    ("../views/modals.rs", include_str!("../views/modals.rs")),
    ("../views/workspace.rs", include_str!("../views/workspace.rs")),
    (
        "../views/connections_home.rs",
        include_str!("../views/connections_home.rs"),
    ),
    (
        "../views/navigation.rs",
        include_str!("../views/navigation.rs"),
    ),
    ("../views/chrome.rs", include_str!("../views/chrome.rs")),
    ("../views/structure.rs", include_str!("../views/structure.rs")),
    ("../views/settings.rs", include_str!("../views/settings.rs")),
    ("../views/tools_ops.rs", include_str!("../views/tools_ops.rs")),
    ("../views/tools_sync.rs", include_str!("../views/tools_sync.rs")),
    ("../data_grid/view.rs", include_str!("../data_grid/view.rs")),
    (
        "../sql_editor/view.rs",
        include_str!("../sql_editor/view.rs"),
    ),
    ("../ui/nav.rs", include_str!("../ui/nav.rs")),
];

fn source(file: &str) -> &'static str {
    VIEWS
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, source)| *source)
        .unwrap_or_else(|| panic!("{file} is not one of the swept files"))
}

/// The `//`-stripped string literals on one line, with the quote offset.
fn literals(line: &str) -> Vec<(usize, String)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
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
                out.push((i, line[i + 1..j].to_owned()));
                i = j + 1;
                continue;
            }
            break;
        }
        if bytes[i] == b'r' && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] != b'"' {
                j += 1;
            }
            if j < bytes.len() {
                out.push((i, line[i + 2..j].to_owned()));
                i = j + 1;
                continue;
            }
            break;
        }
        i += 1;
    }
    out
}

/// The identifier of the call whose first argument starts at `quote`.
fn call_name(line: &str, quote: usize) -> Option<&str> {
    let before = line[..quote].trim_end();
    let head = before.strip_suffix('(')?.trim_end();
    let start = head
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .map_or(0, |index| index + 1);
    head[start..].rsplit('.').next()
}

/// `views/connections.rs:201` matches the runtime notice text, not a label.
fn allowed(file: &str, literal: &str) -> bool {
    file.contains("views/connections.rs") && literal == "失败"
}

#[test]
fn no_display_literal_bypasses_the_translation_table() {
    assert!(ZH_CN.len() > 3_000, "the table is missing: {}", ZH_CN.len());
    let mut failures = Vec::new();
    for (file, body) in VIEWS {
        for (index, line) in body.lines().enumerate() {
            for (quote, literal) in literals(line) {
                if literal.is_empty() {
                    continue;
                }
                let call = call_name(line, quote);
                if matches!(call, Some("t" | "key" | "role" | "path" | "scroll" | "icon")) {
                    continue;
                }
                if allowed(file, &literal) {
                    continue;
                }
                if let Some((key, _)) = ZH_CN.iter().find(|(_, value)| *value == literal.as_str()) {
                    failures.push(format!(
                        "{file}:{}: \"{literal}\" 可用 key \"{key}\"",
                        index + 1
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} display literal(s) bypass the translation table:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// `(file, key, how many times the batch-3 commit asks for it at that site)`.
///
/// The batch's own pin list asserts `source.contains("t(\"key\")")` once per
/// key, so a partial revert of a key that is used at several places survives.
/// Counting the sites closes that hole; `objectSearch.ddl` was missing from the
/// batch's list altogether.
const PINNED: &[(&str, &str, usize)] = &[
    ("../data_grid/view.rs", "common.loading", 1),
    ("../data_grid/view.rs", "query.gridReadOnly.tooltip", 1),
    ("../data_grid/view.rs", "tableData.commit", 1),
    ("../data_grid/view.rs", "tableData.rollback", 1),
    ("../sql_editor/view.rs", "resultFind.replaceNext", 1),
    ("../views/chrome.rs", "conn.connected", 1),
    ("../views/connections_home.rs", "common.copy", 1),
    ("../views/connections_home.rs", "common.importConnections", 2),
    ("../views/connections_home.rs", "conn.connected", 1),
    ("../views/connections_home.rs", "conn.connecting", 2),
    ("../views/connections_home.rs", "connWin.home.connections.connect", 1),
    ("../views/connections_home.rs", "connWin.home.connections.filter", 1),
    ("../views/connections_home.rs", "connWin.home.connections.open", 1),
    ("../views/connections_home.rs", "connWin.home.connections.showAll", 1),
    ("../views/connections_home.rs", "connWin.home.connections.showLess", 1),
    ("../views/connections_home.rs", "connWin.home.selectConnectionTitle", 1),
    ("../views/connections_home.rs", "connWin.home.status.offline", 1),
    ("../views/connections_home.rs", "keymap.action.formatSql", 1),
    ("../views/navigation.rs", "main.ctx.disconnect", 1),
    ("../views/navigation.rs", "query.objectTree.noObjects", 1),
    ("../views/navigation.rs", "query.objectTree.schema", 1),
    ("../views/navigation.rs", "select.noMatches", 3),
    ("../views/settings.rs", "common.back", 1),
    ("../views/settings.rs", "common.stop", 1),
    ("../views/settings.rs", "export.export", 2),
    ("../views/settings.rs", "filter.view.rename", 1),
    ("../views/settings.rs", "import.import", 1),
    ("../views/settings.rs", "onboarding.common.finish", 1),
    ("../views/settings.rs", "sync.next", 1),
    ("../views/structure.rs", "common.close", 1),
    ("../views/structure.rs", "filter.apply", 1),
    ("../views/structure.rs", "indexes.colName", 1),
    ("../views/structure.rs", "indexes.colUnique", 1),
    ("../views/structure.rs", "indexes.columns", 2),
    ("../views/structure.rs", "structView.fieldName", 1),
    ("../views/structure.rs", "structView.nullable", 1),
    ("../views/structure.rs", "structView.type", 1),
    ("../views/tools_ops.rs", "common.cancel", 2),
    ("../views/tools_ops.rs", "common.confirm", 1),
    ("../views/tools_ops.rs", "common.search", 1),
    ("../views/tools_ops.rs", "valueSearch.status.running", 1),
    ("../views/tools_sync.rs", "common.preview", 1),
    ("../views/tools_sync.rs", "filter.clear", 1),
    ("../views/tools_sync.rs", "sync.compare", 1),
    ("../views/tools_sync.rs", "sync.next", 2),
    ("../views/workspace.rs", "chart.viewChart", 1),
    ("../views/workspace.rs", "chart.viewTable", 1),
    ("../views/workspace.rs", "common.cancel", 1),
    ("../views/workspace.rs", "connWin.data", 1),
    ("../views/workspace.rs", "connWin.indexes", 1),
    ("../views/workspace.rs", "main.ready", 1),
    ("../views/workspace.rs", "objectSearch.ddl", 1),
    ("../views/workspace.rs", "query.beginTx", 1),
    ("../views/workspace.rs", "query.commitTx", 1),
    ("../views/workspace.rs", "query.explainError", 1),
    ("../views/workspace.rs", "query.format", 1),
    ("../views/workspace.rs", "query.noFavorites", 1),
    ("../views/workspace.rs", "query.rollbackTx", 1),
    ("../views/workspace.rs", "query.run", 1),
    ("../views/workspace.rs", "query.running", 2),
    ("../views/workspace.rs", "resultFind.openLabel", 1),
    ("../views/workspace.rs", "select.noMatches", 1),
];

#[test]
fn every_batch_three_call_site_keeps_its_translation() {
    let mut failures = Vec::new();
    for (file, key, count) in PINNED {
        let asked = source(file).matches(&format!("t(\"{key}\")")).count();
        if asked < *count {
            failures.push(format!(
                "{file}: expected {count} t(\"{key}\") site(s), found {asked}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "batch-3 call sites lost their translation:\n{}",
        failures.join("\n")
    );
}

#[test]
#[ignore = "DEFECT 62c4e00ae: the SQL toolbar Explain button asks for query.explainError (the AI error-panel action) although the toolbar copy is explain.title = EXPLAIN 分析"]
fn the_explain_button_uses_the_query_toolbar_key() {
    assert!(
        source("../views/workspace.rs").contains("t(\"explain.title\")"),
        "views/workspace.rs still asks for the error-panel key"
    );
}

#[test]
#[ignore = "DEFECT 62c4e00ae: connections_home still renders 'Showing 1 of 2', 'No matches for …' and 'N connections · M groups · K engine types' in English although connWin.home.connections.showing / noMatch / hero.subtitle* exist at the same positions"]
fn the_home_counts_and_empty_states_use_the_home_keys() {
    let home = source("../views/connections_home.rs");
    for key in [
        "connWin.home.connections.showing",
        "connWin.home.connections.noMatch",
        "connWin.home.hero.subtitleGroups",
    ] {
        assert!(
            home.contains(&format!("t(\"{key}\")")),
            "views/connections_home.rs does not ask for {key}"
        );
    }
}

#[test]
#[ignore = "DEFECT 62c4e00ae: the status bar translates Connected but leaves the Disconnected branch as an English literal, and no conn.disconnected key exists"]
fn the_status_bar_speaks_one_language() {
    assert!(
        !source("../views/chrome.rs").contains("\"Disconnected\""),
        "views/chrome.rs still hard-codes Disconnected next to the translated Connected"
    );
}

#[test]
#[ignore = "DEFECT 62c4e00ae: the Columns view tab is still the literal \"Columns\" although connWin.fields = Columns/列 sits beside connWin.data and connWin.indexes"]
fn the_columns_tab_uses_the_connection_key() {
    assert!(
        source("../views/workspace.rs").contains("t(\"connWin.fields\")"),
        "views/workspace.rs Columns tab is not wired"
    );
}

#[test]
#[ignore = "DEFECT 62c4e00ae: snippet Update, sync Select all and the New query button keep English literals although common.save, common.selectAll and common.newQuery carry the same surfaces"]
fn existing_generic_keys_are_taken_over() {
    assert!(
        !source("../views/settings.rs").contains("\"Update\""),
        "views/settings.rs still renders the literal Update"
    );
    assert!(
        !source("../views/tools_sync.rs").contains("\"Select all\""),
        "views/tools_sync.rs still renders the literal Select all"
    );
    assert!(
        source("../views/chrome.rs").contains("t(\"common.newQuery\")"),
        "views/chrome.rs New query button is not wired"
    );
}
