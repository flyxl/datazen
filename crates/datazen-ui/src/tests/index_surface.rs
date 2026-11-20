//! T052: X-class determination for `packages/frontend/src/index.ts` — the
//! `@datazen/frontend` package entry re-export shell.
//!
//! `index.ts` is 55 lines of doc comment plus eight `export … from '…'`
//! statements. It owns no constants and no logic: it is the sanctioned light
//! surface (`.`) that the desktop/web hosts import, and its only job is to be
//! a single import point. Per the segment's X-class ruling there is no 1:1
//! Rune file to port — a barrel whose members already live elsewhere would
//! just create a second import point. Instead this module is the *receipt*:
//! it pins the React export list (so drift in the barrel is detected) and
//! asserts every export's Rune counterpart — an equivalent, or an honest
//! single-window / host-owned gap with its deferred task id.
//!
//! Mapping (React export → Rune equivalent):
//!
//! | React `index.ts` export | Rune side |
//! |---|---|
//! | `DESKTOP_CAPABILITIES` / `WEB_CAPABILITIES` | no capability constant in `datazen-ui`: the single UI form uses the full desktop-shaped `settings_state::SETTINGS_SECTIONS` directly; the form table itself is host-owned |
//! | `IS_DESKTOP_BUILD` | `#[cfg(target_arch = "wasm32")]` form gating in `lib.rs` (compile-time, not a runtime probe) + the `is_desktop` flag in the batch-export port |
//! | `export * from './platform/contracts'` | contract types reachable from `datazen_ui_api` root and its `types` face (`TablePage`/`AppSettings`/`McpHttpStatus`, …) |
//! | `createBrowserEventChannel` | GAP: Rune is single-window with no cross-window bus; seam documented as T437 in `app_root.rs` |
//! | wapp asset resolver suite | pure kernel in `datazen::wapps::protocol` (`parse_wapp_origin_path`, `safe_relative_path`, `is_valid_wapp_id`, `ORIGIN_ASSET_CSP`); web serving in `datazen-server::wapps_static` |
//! | `getWindowKind` / `WindowKind` | GAP: one window, no URL router; seam documented as T644 in `app_root.rs` |
//! | updater exports | GAP in `datazen-ui`: plugin-updater flows are app/host-owned; only the compile-time channel reasoning exists, on the host side |
//! | `AppSettings` / `McpHttpStatus` (type-only) | `datazen_ui_api::{AppSettings, McpHttpStatus}` and `datazen_ui_api::types::{…}` |
//!
//! The two "gap" rows are deliberately *not* fabricated into APIs: the tests
//! pin the seam docs and the absence of a competing port so a future task
//! cannot silently introduce a second, divergent event bus or window router.

use super::*;
use std::collections::BTreeSet;

const REACT_ENTRY: &str = include_str!("../../../../packages/frontend/src/index.ts");
const REACT_CAPABILITIES: &str =
    include_str!("../../../../packages/frontend/src/platform/capabilities.ts");
const REACT_ENV: &str = include_str!("../../../../packages/frontend/src/platform/env.ts");
const REACT_WAPP_ASSETS: &str =
    include_str!("../../../../packages/frontend/src/lib/wappAssetOrigin.ts");
const REACT_UPDATER: &str = include_str!("../../../../packages/frontend/src/lib/updater.ts");
const RUNE_LIB: &str = include_str!("../lib.rs");
const RUNE_APP_ROOT: &str = include_str!("../views/app_root.rs");
const RUNE_BATCH_EXPORT: &str =
    include_str!("../components/connection/batch_export_dialog.rs");
const WAPP_PROTOCOL: &str = include_str!("../../../../crates/datazen/src/wapps/protocol.rs");
const WAPP_STATIC: &str =
    include_str!("../../../datazen-server/src/http/wapps_static.rs");

/// The 26 named exports of `index.ts` (the `export * from './platform/contracts'`
/// line is a wildcard and is pinned separately). Sorted; the extraction below
/// must match exactly so adding/removing a barrel member fails this test.
const EXPECTED_ENTRY_EXPORTS: &[&str] = &[
    "AppSettings",
    "DESKTOP_CAPABILITIES",
    "DESKTOP_WAPP_ASSET_RESOLVER",
    "IS_DESKTOP_BUILD",
    "McpHttpStatus",
    "UpdateChannel",
    "WAPP_ASSET_CSP",
    "WAPP_ASSET_PATH_PREFIX",
    "WEB_CAPABILITIES",
    "WEB_WAPP_ASSET_RESOLVER",
    "WindowKind",
    "buildWappAssetSrc",
    "buildWappIconSrc",
    "configureWappAssetResolver",
    "createBrowserEventChannel",
    "currentVariant",
    "getUpdateChannel",
    "getWindowKind",
    "isSafeWappAssetPath",
    "isUpdaterSupported",
    "isValidWappId",
    "manifestBelongsToBuild",
    "parseWappAssetTarget",
    "readManifestVariant",
    "relaunchToApplyUpdate",
    "resetWappAssetResolver",
];

/// Collect every identifier inside an `export { … }` brace group, after the
/// file's leading doc comment. `type` and `as` are modifiers, not names. This
/// is a text-level surface pin (the barrel is plain re-export text), not a
/// TypeScript parser: an alias (`x as y`) would contribute both halves, which
/// is acceptable because index.ts uses no aliases.
fn react_export_names(src: &str) -> BTreeSet<String> {
    let body = src.split_once("*/").map(|(_, r)| r).unwrap_or(src);
    let mut names = BTreeSet::new();
    let mut in_brace = false;
    let mut buf = String::new();
    for ch in body.chars() {
        match ch {
            '{' => {
                in_brace = true;
                buf.clear();
            }
            '}' => {
                in_brace = false;
                for token in buf.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')) {
                    if !token.is_empty() && token != "type" && token != "as" {
                        names.insert(token.to_string());
                    }
                }
                buf.clear();
            }
            _ if in_brace => buf.push(ch),
            _ => {}
        }
    }
    names
}

/// Flatten a Rust `&str` const that uses `\`-continued source lines into the
/// single string the compiler sees, so the CSP can be compared byte-for-byte
/// with the React literal.
fn rust_const_str(src: &str, name: &str) -> String {
    let marker = format!("const {name}: &str = ");
    let rest = src
        .split_once(&marker)
        .map(|(_, r)| r)
        .unwrap_or_default();
    let start = rest.find('"').map(|i| i + 1).unwrap_or(0);
    let after_open = &rest[start..];
    let end = after_open.find('"').unwrap_or(after_open.len());
    after_open[..end]
        .lines()
        .map(str::trim)
        .map(|line| line.strip_suffix('\\').unwrap_or(line))
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn react_entry_public_surface_is_exactly_the_sanctioned_set() {
    let actual = react_export_names(REACT_ENTRY);
    let expected: BTreeSet<String> =
        EXPECTED_ENTRY_EXPORTS.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        actual, expected,
        "index.ts's re-export surface drifted: the X-class receipt must be refreshed"
    );

    // The one wildcard line is the `platform/contracts` type surface; keeping
    // it a wildcard (not a named list) is why the api crate's `types` face is
    // the single contract vocabulary on the Rune side.
    assert!(
        REACT_ENTRY.contains("export * from './platform/contracts';"),
        "the contracts wildcard re-export must stay the one whole-family surface"
    );

    // The doc comment's sanctioned entries are the reason this file exists.
    for entry in [".", "./app", "./shell", "./styles.css", "./testing/*"] {
        assert!(
            REACT_ENTRY.contains(&format!("`{entry}`")),
            "the sanctioned entry {entry} must stay documented"
        );
    }
}

#[test]
fn capability_split_has_no_rune_fork_and_rune_carries_the_desktop_superset() {
    // Pin the React capability table's values: desktop = full set + host
    // updater + host version; web = a strict 5-section subset + no updater +
    // build-constant version. These literals are the contract the Rune side
    // deliberately collapses into one desktop-shaped form.
    assert!(REACT_CAPABILITIES.contains("export const DESKTOP_CAPABILITIES"), "desktop table");
    assert!(REACT_CAPABILITIES.contains("export const WEB_CAPABILITIES"), "web table");
    assert!(REACT_CAPABILITIES.contains("settingsSections: ALL_SETTINGS_SECTIONS"), "full set");
    assert!(REACT_CAPABILITIES.contains("updater: true"), "desktop updater on");
    assert!(REACT_CAPABILITIES.contains("versionSource: 'host-app'"), "desktop version source");
    assert!(REACT_CAPABILITIES.contains("updater: false"), "web updater off");
    assert!(REACT_CAPABILITIES.contains("versionSource: 'build-constant'"), "web version source");
    let web_sections: BTreeSet<&str> = ["general", "appearance", "dataBrowsing", "editor", "behavior"]
        .into_iter()
        .collect();

    // Rune's single UI form carries the full 12-section desktop-shaped table
    // (React's `ALL_SETTINGS_SECTIONS`), with the 5 web sections as a subset.
    let rune_sections: BTreeSet<&str> = crate::settings_state::SETTINGS_SECTIONS
        .iter()
        .map(|def| def.id)
        .collect();
    assert_eq!(rune_sections.len(), 12, "Rune ships the full section table");
    assert!(rune_sections.len() > web_sections.len(), "desktop table is the superset");
    for id in &web_sections {
        assert!(rune_sections.contains(id), "web section {id} is in the Rune superset");
    }

    // No capability constant is forked inside the single-form UI crate: the
    // form table lives in the host assemblies, never in `datazen-ui`.
    let rune_code: String = RUNE_LIB
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !rune_code.contains("DESKTOP_CAPABILITIES") && !rune_code.contains("WEB_CAPABILITIES"),
        "datazen-ui must not fork a capability table for a single form"
    );
}

#[test]
fn is_desktop_build_is_a_compile_time_form_flag_in_both_assemblies() {
    // React: a build-time `import.meta.env` constant with an explicit ban on
    // runtime `__TAURI_INTERNALS__` probing (form-split §4.2).
    assert!(REACT_ENV.contains("export const IS_DESKTOP_BUILD"), "the constant exists");
    assert!(REACT_ENV.contains("TAURI_ENV_PLATFORM"), "built from the injected env flag");
    assert!(
        REACT_ENV.contains("`__TAURI_INTERNALS__` probing is")
            && REACT_ENV.contains("banned (form-split"),
        "runtime form sniffing must stay explicitly banned in the source doc"
    );

    // Rune: the same compile-time property, expressed as `cfg(target_arch)`
    // form gating in the crate root — the web-only `start` entry is compiled
    // only for wasm, while the native launch surface is the default re-export.
    assert!(
        RUNE_LIB.contains("#[cfg(target_arch = \"wasm32\")]\npub use app::start;"),
        "the web entry must be a compile-time-gated re-export"
    );
    assert!(
        RUNE_LIB.contains("pub use app::{app_with_adapter, run, run_with_adapter};"),
        "the native launch surface stays a public crate-root re-export"
    );

    // The observable consumer of React's flag is the batch-export format/cancel
    // decision; the Rune port carries that decision as an `is_desktop` flag
    // documented against React's constant.
    assert!(
        RUNE_BATCH_EXPORT.contains("React's `IS_DESKTOP_BUILD`"),
        "the batch-export port must name the flag it inherits"
    );
}

#[test]
fn platform_contract_types_are_reachable_from_the_api_face() {
    // The React wildcard `export * from './platform/contracts'` is mirrored by
    // the api crate's single contract vocabulary: the families' members are
    // reachable at the crate root and under the `types` face. Compile-time
    // identity pins (no value needed) are the observation channel.
    fn pin<T>() {}
    pin::<datazen_ui_api::TablePage>();
    pin::<datazen_ui_api::types::AppSettings>();
    pin::<datazen_ui_api::types::McpHttpStatus>();
    pin::<datazen_ui_api::AppSettings>();
    pin::<datazen_ui_api::McpHttpStatus>();

    // The read-page contract's fixed refusal is the observable behavior the
    // contracts family guarantees to a host — the same pin the barrel audit
    // uses, kept here so this X-class receipt is self-contained.
    let mut unwired = datazen_ui_api::UnimplementedAdapter;
    let err = datazen_ui_api::DesktopAdapter::read_table_page(
        &mut unwired,
        "demo.table",
        0,
        20,
        None,
        None,
    );
    assert_eq!(
        err.err().as_deref(),
        Some(datazen_ui_api::DESKTOP_NOT_IMPLEMENTED),
        "the read-page contract surfaces the fixed error"
    );
}

#[test]
fn browser_event_channel_is_an_honest_single_window_gap() {
    // React's `createBrowserEventChannel` is the cross-window host-broadcast
    // rail. Rune is single-window, with no such rail; the app-root port records
    // the gap and its future task id instead of inventing a second bus.
    assert!(
        RUNE_APP_ROOT.contains("lib/crossWindowBus.ts") && RUNE_APP_ROOT.contains("T437"),
        "the cross-window gap must stay documented with its task id"
    );
    assert!(
        RUNE_APP_ROOT.contains("single-window with no event bus"),
        "the single-window ruling must stay explicit"
    );
    assert!(
        !RUNE_LIB.contains("create_browser_event_channel")
            && !RUNE_LIB.contains("cross_window_bus"),
        "no competing event-bus module may appear in the Rune crate root"
    );

    // The observable substitute is the one-window root: the first frame is the
    // main composition, so there is no second window to broadcast to.
    let mut rt = runtime();
    rt.frame(0.0);
    assert!(
        rt.find("datazen-root").is_some(),
        "the single main window is composed without an event bus"
    );
}

#[test]
fn wapp_asset_resolver_kernel_is_cross_language_pinned() {
    // React's asset-origin constants and grammar.
    assert!(REACT_WAPP_ASSETS.contains("WAPP_ASSET_PATH_PREFIX = '/wapps/'"), "path prefix");
    for name in [
        "isValidWappId",
        "isSafeWappAssetPath",
        "parseWappAssetTarget",
        "DESKTOP_WAPP_ASSET_RESOLVER",
        "WEB_WAPP_ASSET_RESOLVER",
        "buildWappAssetSrc",
        "buildWappIconSrc",
        "configureWappAssetResolver",
        "resetWappAssetResolver",
    ] {
        assert!(
            REACT_WAPP_ASSETS.contains(&format!("export function {name}"))
                || REACT_WAPP_ASSETS.contains(&format!("export const {name}")),
            "React wapp asset export missing: {name}"
        );
    }

    // The Rust kernel owns the same grammar/path safety in one place, shared
    // by both hosts (desktop `datazen://`, web `/wapps/`), so no Rune-side
    // resolver registry is needed.
    for symbol in [
        "pub const ORIGIN_ASSET_CSP",
        "pub const ASSET_CSP",
        "pub fn parse_wapp_origin_path",
        "pub fn parse_datazen_uri",
        "pub fn safe_relative_path",
        "is_valid_wapp_id",
    ] {
        assert!(
            WAPP_PROTOCOL.contains(symbol),
            "Rune kernel symbol missing: {symbol}"
        );
    }
    assert!(
        WAPP_PROTOCOL.contains("const WAPP_ASSET_PATH_PREFIX: &str = \"/wapps/\""),
        "the Rust path prefix must match React's byte-for-byte"
    );

    // CSP cross-language literal pin: the two React string pieces joined are
    // exactly the Rust `ORIGIN_ASSET_CSP` (web form) after line continuations.
    const CSP_PART_1: &str =
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; ";
    const CSP_PART_2: &str =
        "img-src 'self' data:; font-src 'self'; connect-src 'none'";
    assert!(REACT_WAPP_ASSETS.contains(CSP_PART_1), "React CSP piece 1");
    assert!(REACT_WAPP_ASSETS.contains(CSP_PART_2), "React CSP piece 2");
    assert_eq!(
        rust_const_str(WAPP_PROTOCOL, "ORIGIN_ASSET_CSP"),
        format!("{CSP_PART_1}{CSP_PART_2}"),
        "the web CSP must match across React and the Rust kernel"
    );
    assert!(
        WAPP_STATIC.contains("\"/wapps/{*rest}\""),
        "the web origin must actually serve the resolver's path prefix"
    );
}

#[test]
fn window_kind_routing_is_an_honest_single_window_seam() {
    // React's `getWindowKind` reads a URL `?window=` param and routes across
    // six windows. Rune's host opens one main window; the seam doc names T644
    // and the unused other arms instead of a divergent router.
    assert!(
        RUNE_APP_ROOT.contains("lib/windowKind.ts") && RUNE_APP_ROOT.contains("T644"),
        "the window-kind gap must stay documented with its task id"
    );
    assert!(
        RUNE_APP_ROOT.contains("getWindowKind()"),
        "the React routing source must stay named in the seam table"
    );

    // Observable: the root composes the main window directly — there is no
    // kind branch in the tree to take.
    let mut rt = runtime();
    rt.frame(0.0);
    for key in ["datazen-root", "datazen-titlebar", "datazen-body", "datazen-page"] {
        assert!(rt.find(key).is_some(), "main-window key present: {key}");
    }
}

#[test]
fn updater_exports_are_host_owned_not_in_the_rune_ui() {
    // The React updater module's own doc says channel/variant reasoning is
    // pure build constants while check/download/relaunch flows are app-owned.
    assert!(
        REACT_UPDATER.contains("app-owned") && REACT_UPDATER.contains("HostSettingsPort"),
        "the updater's host ownership must stay documented"
    );
    for name in [
        "currentVariant",
        "getUpdateChannel",
        "isUpdaterSupported",
        "manifestBelongsToBuild",
        "readManifestVariant",
        "relaunchToApplyUpdate",
    ] {
        assert!(
            REACT_UPDATER.contains(&format!("export function {name}")),
            "React updater export missing: {name}"
        );
    }

    // No updater module is folded into the Rune UI crate: the native host owns
    // the plugin-updater flows, and the single shared UI never imports them.
    assert!(
        !RUNE_LIB.contains("mod updater") && !RUNE_LIB.contains("pub mod updater"),
        "the app-owned updater must not be re-created inside datazen-ui"
    );
}

#[test]
fn the_type_only_reexports_land_on_the_api_types_face() {
    // `export type { AppSettings, McpHttpStatus } from './types/index'` is the
    // only type-only line in the barrel. Both names are rooted in the api
    // crate (already pinned above); here we pin that they are type members of
    // the `types` face rather than runtime values — a `pin::<T>()` compiles for
    // both, so assert the module paths exist via the family's surface test.
    fn pin<T>() {}
    pin::<datazen_ui_api::types::AppSettings>();
    pin::<datazen_ui_api::types::McpHttpStatus>();
    assert!(
        REACT_ENTRY.contains("export type { AppSettings, McpHttpStatus } from './types/index';"),
        "the type-only re-export line must stay in the barrel"
    );
}
