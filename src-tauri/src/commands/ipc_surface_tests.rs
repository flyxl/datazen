//! Cross-module guards for the merged IPC surface.
//!
//! These assert the *contract* between the config / connection-import /
//! app-archive modules and the registered handler list, so they live here
//! rather than inside any single one of them.

use std::path::{Path, PathBuf};

/// Every file that hosts part of the merged config IPC surface.
const SOURCE: &str = concat!(
    include_str!("config.rs"),
    "\n",
    include_str!("connection_import/ipc.rs"),
    "\n",
    include_str!("app_archive.rs"),
    "\n",
    include_str!("encryption_key.rs"),
    "\n",
    include_str!("tunnel.rs"),
);
const BOOTSTRAP_RS: &str = include_str!("../bootstrap/run.rs");

fn command_params(command: &str) -> String {
    let needle = format!("pub async fn {command}(");
    let start = SOURCE
        .find(&needle)
        .unwrap_or_else(|| panic!("{command} not found"));
    let after = &SOURCE[start + needle.len()..];
    let end = after.find(')').expect("params close");
    after[..end].to_string()
}

#[test]
fn bootstrap_rs_registers_merged_commands_only() {
    assert!(BOOTSTRAP_RS.contains("commands::export_connections,"));
    assert!(BOOTSTRAP_RS.contains("commands::import_connections_with_dialog,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_settings,"));
    assert!(BOOTSTRAP_RS.contains("commands::save_settings,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_tunnels,"));
    assert!(BOOTSTRAP_RS.contains("commands::save_tunnel,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_tunnel_summaries,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_tunnel_usage,"));
    assert!(BOOTSTRAP_RS.contains("commands::test_tunnel,"));
}

/// The result grid's write-back gate must be reachable over IPC.
///
/// A command that exists but is not registered compiles, passes every unit
/// test and is simply absent at runtime — the front end then waits forever
/// for a verdict that never arrives, and the grid silently stays read-only
/// without saying why. The registration is therefore pinned here, by the same
/// `include_str!` discipline the other surface guards use.
#[test]
fn bootstrap_rs_registers_the_result_grid_write_back_commands() {
    assert!(BOOTSTRAP_RS.contains("crate::commands::resolve_result_origin,"));
    assert!(BOOTSTRAP_RS.contains("crate::commands::preview_result_cell_change,"));
}

#[test]
fn merged_commands_route_through_shared_resolve_override_path() {
    // Single mechanism: config.rs must reuse the error.rs helper (F3
    // pattern) instead of redefining a local copy, and every merged
    // command body must gate through it. Needles are assembled at runtime
    // so this test's own source never contains them.
    let resolve = "resolve";
    let override_path = "override_path";
    let local_def = format!("fn {resolve}_{override_path}(");
    assert!(
        !SOURCE.contains(&local_def),
        "config.rs must not redefine resolve_override_path"
    );
    let call = format!("{resolve}_{override_path}(override_path");
    let gated_bodies = SOURCE.matches(&call).count();
    // Core file + re-exported archive module both use the helper; count is soft.
    assert!(
        gated_bodies >= 3,
        "merged commands must gate their override branch; found {gated_bodies}"
    );
}

mod ipc_contract_guards {
    use super::*;

    #[test]
    fn merged_commands_take_override_path_not_raw_path_param() {
        for cmd in [
            "export_connections",
            "import_connections_preview",
            "import_connections_with_dialog",
            "export_app_data",
            "import_app_data",
        ] {
            let params = command_params(cmd);
            assert!(
                params.contains("override_path: Option<String>"),
                "`{cmd}` must expose the webdriver-only override; got: {params}"
            );
            assert!(
                !params.contains("path: String"),
                "raw `path` parameter must not return after the merge; got: {params}"
            );
        }
    }

    #[test]
    fn tunnel_commands_expose_metadata_only_params() {
        // The summary IPC must stay parameterless (no way to ask for secrets),
        // and the connectivity probe takes an explicit target rather than a
        // caller-supplied config blob.
        let summary = command_params("get_tunnel_summaries");
        assert_eq!(
            summary.trim().trim_end_matches(','),
            "state: State<'_, AppState>",
            "get_tunnel_summaries must take nothing but state; got: {summary}"
        );

        let usage = command_params("get_tunnel_usage");
        assert!(usage.contains("id: String"), "got: {usage}");
        assert!(!usage.contains("password"), "got: {usage}");

        let probe = command_params("test_tunnel");
        assert!(probe.contains("id: String"), "got: {probe}");
        assert!(probe.contains("target_host: String"), "got: {probe}");
        assert!(probe.contains("target_port: u16"), "got: {probe}");
    }

    #[test]
    fn stale_dialog_twins_are_gone_from_config_rs() {
        // Needle parts are assembled at runtime so this test's own source never
        // contains them (mirroring backup.rs guards).
        let fn_prefix = "pub async fn ";
        let with_dialog = "_with_";
        let dialog_kind = "dialog(";
        for name in ["export_connections", "export_app_data", "import_app_data"] {
            let variant = format!("{fn_prefix}{name}{with_dialog}{dialog_kind}");
            assert!(!SOURCE.contains(&variant), "stale dialog twin `{variant}`");
        }
    }
}

/// The `grid.*` codes are the **shared layer's** vocabulary.
///
/// The host maps a shared error onto [`super::error::CommandError`] and
/// nothing else: which code a failure is, and what it means, is decided in
/// `modules`. A rule code inside a command shell is therefore a
/// rule that lives in the wrong place — and it would be invisible to every
/// other check, because the shell still compiles and every unit test still
/// passes.
///
/// Two shapes are refused, because there are two ways to decide a code in the
/// shell:
///
/// 1. **writing the code** — a `grid.` literal, in any form;
/// 2. **naming the code** — a `ResultErrorCode` mention. The shell may ask the
///    shared layer which code a reason maps to
///    (`read_only_error_code(reason)`), but it may not pick one itself: the
///    choice of code is a rule.
///
/// The scan removes comments first (both `//` and `/* … */`), and it does so
/// with a scanner that knows about string literals — otherwise a `//` inside a
/// string (a URL, say) would cut the line short and hide a code behind it,
/// while a block comment that merely *mentions* a code would be reported as
/// one.
///
/// Test files are exempt: a test that *asserts* a code must name it.
#[test]
fn the_command_shell_carries_no_rule_codes() {
    let mut checked = 0usize;
    for path in command_sources() {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
        checked += 1;
        let code = without_comments(&source);
        for (number, line) in code.lines().enumerate() {
            assert!(
                !line.contains("grid."),
                "{}:{} carries a rule code; the `grid.*` vocabulary belongs to the shared layer",
                path.display(),
                number + 1
            );
            assert!(
                !line.contains("ResultErrorCode"),
                "{}:{} names a rule code; the shell must ask the shared layer which code a reason maps to",
                path.display(),
                number + 1
            );
        }
    }
    assert!(
        checked > 5,
        "guard ran vacuously: only {checked} command source(s) were scanned"
    );
}

/// The source with its comments removed.
///
/// The scanner knows about string literals, so a `//` inside a string does not
/// start a comment, and it knows about block comments, so a code mentioned in
/// a doc block is not a code. Raw strings (`r"…"`) are not tracked as such:
/// the only thing that matters here is that a *rule code* is never hidden
/// behind one.
fn without_comments(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut index = 0usize;
    let mut in_string = false;
    let mut in_block_comment = false;
    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        if in_block_comment {
            if current == '*' && next == Some('/') {
                in_block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if in_string {
            out.push(current);
            if current == '\\' {
                if let Some(escaped) = next {
                    out.push(escaped);
                    index += 2;
                } else {
                    index += 1;
                }
                continue;
            }
            if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if current == '/' && next == Some('/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if current == '/' && next == Some('*') {
            in_block_comment = true;
            index += 2;
            continue;
        }
        if current == '"' {
            in_string = true;
            out.push(current);
            index += 1;
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

/// The production sources of the IPC command shell.
///
/// Test files are skipped: they assert codes, so they have to name them.
fn command_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mut files = Vec::new();
    collect_command_sources(&root, &mut files);
    files.sort();
    files
}

fn collect_command_sources(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|err| panic!("cannot list {}: {err}", dir.display()));
    for entry in entries {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            // A `tests` directory holds assertions, not rules.
            if path.file_name().and_then(|name| name.to_str()) != Some("tests") {
                collect_command_sources(&path, files);
            }
            continue;
        }
        let is_rust = path.extension().and_then(|ext| ext.to_str()) == Some("rs");
        let is_test = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "tests.rs" || name.ends_with("_tests.rs"));
        if is_rust && !is_test {
            files.push(path);
        }
    }
}
