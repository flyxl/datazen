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

/// Byte spans of the string literal inside every translation call on the line.
///
/// Matching the opening translation call as a token (not as a substring) keeps
/// `text(...)` under the reverse index: `text` is an identifier, so its trailing
/// opener is not a translation call.
fn t_arg_spans(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.';
    let mut spans = Vec::new();
    let mut i = 0;
    while i + 2 < bytes.len() {
        let starts_call = bytes[i] == b't'
            && bytes[i + 1] == b'('
            && bytes[i + 2] == b'"'
            && (i == 0 || !ident(bytes[i - 1]));
        if starts_call {
            let start = i + 2;
            let mut j = start + 1;
            while j < bytes.len() && bytes[j] != b'"' {
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            if j < bytes.len() {
                spans.push((start, j + 1));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    spans
}

/// The `//`-stripped string literals on one line, with the opening-quote offset.
fn literals(line: &str) -> Vec<(usize, String)> {
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
                j += if bytes[j] == b'\\' { 2 } else { 1 };
            }
            if j < bytes.len() {
                out.push((i, line[i + 1..j].to_owned()));
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
                out.push((i, line[i + 2..j].to_owned()));
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
            let t_spans = t_arg_spans(line);
            for (quote, literal) in literals(line) {
                if literal.is_empty() {
                    continue;
                }
                // Only the argument of a `t(...)` call is exempt; a display
                // literal on the same line still has to face the table.
                if t_spans
                    .iter()
                    .any(|(start, end)| *start <= quote && quote < *end)
                {
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
const PINNED: &[(&str, &str, usize)] = &[
    ("../views/workspace.rs", "chart.viewChart", 1),
    ("../views/workspace.rs", "chart.viewTable", 1),
    ("../views/workspace.rs", "common.cancel", 1),
    ("../views/workspace.rs", "connWin.data", 1),
    ("../views/workspace.rs", "connWin.fields", 1),
    ("../views/workspace.rs", "connWin.indexes", 1),
    ("../views/workspace.rs", "explain.title", 1),
    ("../views/workspace.rs", "main.ready", 1),
    ("../views/workspace.rs", "objectSearch.ddl", 1),
    ("../views/workspace.rs", "query.beginTx", 1),
    ("../views/workspace.rs", "query.commitTx", 1),
    ("../views/workspace.rs", "query.format", 1),
    ("../views/workspace.rs", "query.noFavorites", 1),
    ("../views/workspace.rs", "query.rollbackTx", 1),
    ("../views/workspace.rs", "query.run", 1),
    ("../views/workspace.rs", "query.running", 2),
    ("../views/workspace.rs", "resultFind.openLabel", 1),
    ("../views/workspace.rs", "select.noMatches", 1),
    ("../views/connections_home.rs", "common.copy", 1),
    (
        "../views/connections_home.rs",
        "common.importConnections",
        2,
    ),
    ("../views/connections_home.rs", "conn.connected", 1),
    ("../views/connections_home.rs", "conn.connecting", 2),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.connect",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.filter",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.noMatch",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.open",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.showAll",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.showLess",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.connections.showing",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.hero.subtitleConnections",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.hero.subtitleGroups",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.hero.subtitleTypes",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.selectConnectionTitle",
        1,
    ),
    (
        "../views/connections_home.rs",
        "connWin.home.status.offline",
        1,
    ),
    ("../views/connections_home.rs", "keymap.action.formatSql", 1),
    ("../views/navigation.rs", "common.newQuery", 1),
    ("../views/navigation.rs", "main.ctx.disconnect", 1),
    ("../views/navigation.rs", "query.objectTree.noObjects", 1),
    ("../views/navigation.rs", "query.objectTree.schema", 1),
    ("../views/navigation.rs", "select.noMatches", 3),
    ("../views/chrome.rs", "common.newQuery", 2),
    ("../views/chrome.rs", "conn.connected", 1),
    ("../views/structure.rs", "common.close", 1),
    ("../views/structure.rs", "filter.apply", 1),
    ("../views/structure.rs", "indexes.colName", 1),
    ("../views/structure.rs", "indexes.colUnique", 1),
    ("../views/structure.rs", "indexes.columns", 2),
    ("../views/structure.rs", "structView.fieldName", 1),
    ("../views/structure.rs", "structView.nullable", 1),
    ("../views/structure.rs", "structView.type", 1),
    ("../views/settings.rs", "common.back", 1),
    ("../views/settings.rs", "common.stop", 1),
    ("../views/settings.rs", "export.export", 2),
    ("../views/settings.rs", "filter.view.rename", 1),
    ("../views/settings.rs", "import.import", 1),
    ("../views/settings.rs", "onboarding.common.finish", 1),
    ("../views/settings.rs", "sync.next", 1),
    ("../views/tools_ops.rs", "common.cancel", 2),
    ("../views/tools_ops.rs", "common.confirm", 1),
    ("../views/tools_ops.rs", "common.search", 1),
    ("../views/tools_ops.rs", "valueSearch.status.running", 1),
    ("../views/tools_sync.rs", "common.preview", 1),
    ("../views/tools_sync.rs", "common.selectAll", 1),
    ("../views/tools_sync.rs", "filter.clear", 1),
    ("../views/tools_sync.rs", "sync.compare", 1),
    ("../views/tools_sync.rs", "sync.next", 2),
    ("../data_grid/view.rs", "common.loading", 1),
    ("../data_grid/view.rs", "query.gridReadOnly.tooltip", 1),
    ("../data_grid/view.rs", "tableData.commit", 1),
    ("../data_grid/view.rs", "tableData.rollback", 1),
    ("../sql_editor/view.rs", "resultFind.replaceNext", 1),
];

#[test]
fn every_wired_key_is_still_asked_for_in_its_file() {
    for (file, key, count) in PINNED {
        let source = FILES
            .iter()
            .find(|(path, _)| path == file)
            .map(|(_, source)| *source)
            .unwrap_or_else(|| panic!("{file} is not in FILES"));
        let asked = source.matches(&format!("t(\"{key}\")")).count();
        assert!(
            asked >= *count,
            "{file}: expected {count} t(\"{key}\") site(s), found {asked}; update this guard if the wiring moved"
        );
    }
}
