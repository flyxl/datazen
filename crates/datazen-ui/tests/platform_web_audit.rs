//! Independent T045 audit of the coder's X-class ruling.
//!
//! Two claims are pinned independently of the coder's own tests:
//!   1. `packages/frontend/src/platform/web/index.ts` is genuinely a pure
//!      re-export barrel — after stripping comments, its ONLY statement is
//!      `export { ... } from './stub'`; no constant, function or arrow exists.
//!      The guard is also run against synthetic source with logic injected, to
//!      prove it discriminates (not a vacuous self-fulfilling check).
//!   2. The Rune "native adapter injection" the ruling leans on is real: an
//!      unwired AppState errors with DESKTOP_NOT_IMPLEMENTED, but injecting a
//!      wired adapter through the same port flips the sync result to a real
//!      answer with no error.

mod common;
use common::*;
use datazen_ui::AppState;
use datazen_ui_api::DESKTOP_NOT_IMPLEMENTED;

/// The real React barrel, embedded verbatim.
const INDEX_TS: &str = include_str!("../../../packages/frontend/src/platform/web/index.ts");

/// Strip block (`/* .. */`) and line (`// ..`) comments from `src`.
fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let mut in_block = false;
    let mut in_line = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_block {
            if c == '*' && i + 1 < bytes.len() && bytes[i + 1] as char == '/' {
                in_block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_line {
            if c == '\n' {
                in_line = false;
                out.push('\n');
            }
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '*' {
            in_block = true;
            i += 2;
            continue;
        }
        if c == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '/' {
            in_line = true;
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// True only when `src` is a pure single re-export from `./stub`: no other
/// statement, no constant/function/class/arrow, one trailing semicolon.
fn only_reexports_from_stub(src: &str) -> bool {
    let code = strip_comments(src);
    let code = code.trim();
    code.starts_with("export {")
        && code.ends_with("from './stub';")
        && code.matches(';').count() == 1
        && !code.contains("const ")
        && !code.contains("let ")
        && !code.contains("var ")
        && !code.contains("function ")
        && !code.contains("class ")
        && !code.contains("import ")
        && !code.contains("=>")
}

/// X-class evidence: the actual 14-line file carries no logic to port.
#[test]
fn index_ts_is_a_pure_reexport_barrel() {
    assert!(
        only_reexports_from_stub(INDEX_TS),
        "platform/web/index.ts must be a pure re-export barrel, got:\n{}",
        strip_comments(INDEX_TS)
    );
}

/// Reverse verification: the guard must reject a barrel with logic added, so
/// the ruling would flip to WRONG if a coder smuggled functionality in here.
#[test]
fn barrel_guard_rejects_injected_logic() {
    let with_const = format!("{}\nconst smuggled = 42;\n", INDEX_TS);
    assert!(!only_reexports_from_stub(&with_const), "a const slipped past the barrel guard");

    let with_fn = format!("{}\nexport function smuggled() {{ return 1; }}\n", INDEX_TS);
    assert!(!only_reexports_from_stub(&with_fn), "a function slipped past the barrel guard");

    let with_arrow = format!("{}\nconst f = () => 1;\n", INDEX_TS);
    assert!(!only_reexports_from_stub(&with_arrow), "an arrow slipped past the barrel guard");
}

/// Injection is real, not claimed: the same port that errors on the default
/// unwired adapter returns a real list once a host adapter is installed.
#[test]
fn injected_adapter_changes_sync_result() {
    use common::SnippetHostAdapter;

    let mut state = AppState::default();
    state.sync_desktop_connections();
    assert_eq!(
        state.desktop_error.as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "default AppState must start unwired and surface the fixed error"
    );
    assert!(state.desktop_connections.is_none());

    state.desktop = Box::new(SnippetHostAdapter { snippets: Vec::new() });
    state.sync_desktop_connections();
    assert!(
        state.desktop_connections.is_some(),
        "the injected adapter's list must reach the state via the port"
    );
    assert!(
        state.desktop_error.is_none(),
        "a wired adapter clears the not-implemented error"
    );
}
