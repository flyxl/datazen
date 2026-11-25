//! T094 — `packages/frontend/src/platform/web/stub.ts` (198 lines) parity ruling.
//!
//! ## X-class ruling (no Rune body to port)
//!
//! `stub.ts` is **web-platform assembly code**: it binds browser-only APIs
//! (`navigator.clipboard`, `window.open`, `Notification`,
//! `document.fullscreenElement`, `BroadcastChannel`, Blob export) into React's
//! `PlatformBindings`. The Rune UI is a single cross-platform crate
//! (`datazen-ui`); it has no parallel web assembly and no browser-platform
//! port. Creating a fake `platform/web.rs` with empty/throwing structs would be
//! a fake file, so **no product code is added here**. This module only pins the
//! observable Rune facts that back the ruling and records the per-method
//! mapping honestly.
//!
//! T045 (`tests/platform_web.rs`) already covers `platform/web/index.ts` (a pure
//! re-export barrel) and the 1:1 mapping of its five factories. This module
//! extends that ruling to the ten factories/behaviors *inside* `stub.ts` and to
//! each `PlatformServices` method.
//!
//! ## Mapping: React `stub.ts` factory / method → Rune counterpart
//!
//! | React factory / method | Rune counterpart, or honest absence |
//! | --- | --- |
//! | `createWebBackendTransportStub` — every `call` throws `ApiError('CapabilityUnsupported', …)`, no web backend exists | No second (web) assembly in Rune. One cross-platform `run()` / `app_with_adapter()` binds one `DesktopAdapter`; the unwired adapter's ports answer `Err(DESKTOP_NOT_IMPLEMENTED)` — an explicit gap, never a silent empty answer. The real browser transport lives in `packages/web/src/platform/web/backendTransport.ts` (HTTP/SSE), **not** in the UI crate. |
//! | `createWebPlatformServices.saveTextWithDialog` → `false` (inert dialog port) | No browser download UI in the cross-platform crate; native save goes through rune's `Tx::file_dialog`. Web production half: `packages/web/src/platform/web/platformServices.ts`. |
//! | `saveBinaryWithDialog` → `false` | Same as above; no Rune web half. |
//! | `openTextWithDialog` → `null` | Same as above; no Rune web half (native opens through `Tx::file_dialog`). |
//! | `openDirectoryWithDialog` → `null` (no browser directory-path semantics) | No directory-path port in Rune; web shell binds the cancel answer. |
//! | `writeClipboard(text)` → `navigator.clipboard.writeText` | Rune's native copy is `Tx::copy` (a11y clipboard bridge); there is no browser `navigator.clipboard` port in `datazen-ui`. |
//! | `writeClipboardHtml({html, altText})` → `ClipboardItem` with plain-text fallback | **No equivalent**: Rune has no HTML-clipboard port at all — not even a native one. Web shell owns the browser half. |
//! | `readClipboard()` → `navigator.clipboard.readText` | **No equivalent**: Rune has no read-clipboard port. Web shell owns the browser half. |
//! | `openExternal({url, path})` → `window.open(url, '_blank', 'noopener')`; local path is desktop-only | Rune has no `window.open` port; native link handling is a host concern. Web shell binds the browser half (`platformServices.ts:188`). |
//! | `toggleFullscreen()` → `document.documentElement.requestFullscreen()` / `exitFullscreen()` | No fullscreen port in Rune; web shell binds it (`platformServices.ts:195`, `windowRouter.ts:35`). |
//! | `showNotification({title, body})` → `Notification` API, permission-aware | No notification port in Rune; web shell binds it (`platformServices.ts:201+`). |
//! | `pickPath()` → `null` (no native path picker in a tab) | Rune native path picking is `Tx::file_dialog`; there is no web form in this crate, so no cancel-answer port to bind. |
//! | `getOsPlatform()` → user-agent sniff (`macos`/`windows`/`linux`/`unknown`) | Rune gets the OS at compile time (`std::env::consts::OS`); no browser user-agent port. Web shell owns the UA half. |
//! | `createWebWindowRouter` — no child windows; `window.focus()`, `document.fullscreenElement`, `window.close()` | Rune is a single-window app with a self-drawn titlebar (`app.rs:TITLEBAR_HEIGHT`), no child-window topology and no OS chrome port. Web shell binds the browser router (`windowRouter.ts`). |
//! | `createWebEventChannel` → `createBrowserEventChannel()` (BroadcastChannel across tabs) | Rune's event loop is in-process `Tx`/wake; there is no cross-window BroadcastChannel port. Web shell binds the browser rail (`assembly.ts:159`). |
//! | `assembleWebPlatformStub` — `setPlatformServices` + `configurePlatform` (services/windows/events/hostSettings/tablesExporter/capabilities) | No second assembly. `run()` / `run_with_adapter()` / `app_with_adapter()` construct the one cross-platform app; `AppState` starts on the unwired adapter and fails closed. Blob export is a web-only half owned by the web shell. |
//! | `WEB_CAPABILITIES` (`PlatformCapabilities`: 5 settings sections, `updater: false`, `versionSource: 'build-constant'`) | **No Rust equivalent exists** — see the GAP note below. |
//!
//! Note: the task brief calls `createWebPlatformServices` "11 methods"; the
//! source has **12** (`saveTextWithDialog`, `saveBinaryWithDialog`,
//! `openTextWithDialog`, `openDirectoryWithDialog`, `writeClipboard`,
//! `writeClipboardHtml`, `readClipboard`, `openExternal`, `toggleFullscreen`,
//! `showNotification`, `pickPath`, `getOsPlatform`). All 12 are mapped above.
//!
//! ## GAP (leftover, not papered over)
//!
//! React's `WEB_CAPABILITIES` is a **compile-time, fail-closed capability
//! table**: disabled capabilities are structurally unbound (`updater: false`
//! → `null`) or bound to an all-throw `CapabilityUnsupported` port
//! (`unsupportedCapabilityPort`), never a permissive empty answer. Rune has
//! **no `PlatformCapabilities` / `WEB_CAPABILITIES` equivalent**: gating is
//! implicit in which port is wired, and every unwired port fails closed with
//! the coarser `DESKTOP_NOT_IMPLEMENTED`. That is safe but not table-driven —
//! there is no place to express "this form serves five settings sections and
//! no updater". Proposed owner: the Rune platform/architecture track; add a
//! capability table to `datazen-ui-api` and thread it through `AppState` only
//! if/when a wasm browser form actually needs section/updater gating. No test
//! here pretends that table exists.
//!
//! ## Why the web-shell assertions are compile-time/path checks
//!
//! `datazen-ui` cannot import or call TypeScript browser APIs at runtime (no
//! wasm-bindgen browser bridge in this cross-platform crate), so a runtime
//! assertion that the browser transport is "real" is impossible from here.
//! `include_str!` is therefore the strongest available channel: it fails
//! *compilation* if the file is moved or deleted, and the content assertions
//! prove the file is a genuine browser-native transport (not an empty stub).
//! Nothing here self-reports through a helper we also control.

use datazen_ui::AppState;
use datazen_ui_api::{
    DesktopAdapter, UnimplementedAdapter, DESKTOP_NOT_IMPLEMENTED,
};
use rune::App;

/// Extends T045's fn-pointer pin to the whole stub ruling: one cross-platform
/// `run()`, native adapter injection via `app_with_adapter`, and the native
/// `run_with_adapter` on non-wasm. These are compile-time bindings — a second
/// web assembly appearing would not satisfy this surface.
#[test]
fn one_cross_platform_assembly_entry_with_native_adapter_injection() {
    let _run: fn() = datazen_ui::run;
    let _app_with_adapter: fn(Box<dyn DesktopAdapter>) -> App<AppState> = datazen_ui::app_with_adapter;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _run_with_adapter: fn(Box<dyn DesktopAdapter>) = datazen_ui::run_with_adapter;
    }
}

/// The exact error value/message shape an unwired port answers with. Pinning
/// the literal (not just the constant) is what makes a silent-empty mutation
/// of the constant go red rather than quietly passing with a different string.
#[test]
fn unwired_error_constant_has_the_exact_fail_closed_message() {
    assert_eq!(
        DESKTOP_NOT_IMPLEMENTED,
        "not implemented in the desktop adapter yet",
        "the fixed error is displayed verbatim; it must not become a silent empty answer"
    );
}

/// The Rune counterpart of `createWebBackendTransportStub`'s all-throw `call`:
/// `UnimplementedAdapter`'s three required ports each return the exact fixed
/// error, never an empty collection / zero / empty page. A representative set
/// spans connections, table reads and cell commits.
#[test]
fn unwired_adapter_errors_on_representative_required_ports() {
    let mut adapter = UnimplementedAdapter;

    assert_eq!(
        adapter.list_connections().err().as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "list_connections must fail closed, not answer Ok(vec![])"
    );
    assert_eq!(
        adapter
            .read_table_page("t", 1, 100, None, None)
            .err()
            .as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "read_table_page must fail closed, not answer an empty page"
    );
    assert_eq!(
        adapter.commit_cells("t", &[]).err().as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "commit_cells must fail closed, not answer Ok(0)"
    );
}

/// Stronger no-silent-empty proof: an adapter whose three *required* ports
/// answer `Ok(empty)` still gets `DESKTOP_NOT_IMPLEMENTED` from every default
/// port. The fail-closed behavior therefore lives in the trait's defaults (the
/// same seam a web/native host would inherit), not in a helper we wrote.
#[test]
fn default_ports_fail_closed_even_when_required_ports_answer_empty() {
    struct EmptyRequiredPortsAdapter;

    impl DesktopAdapter for EmptyRequiredPortsAdapter {
        fn list_connections(
            &mut self,
        ) -> Result<Vec<datazen_ui_api::ConnectionProfile>, String> {
            Ok(Vec::new())
        }

        fn read_table_page(
            &mut self,
            _table: &str,
            _page: u32,
            _page_size: u32,
            _sort: Option<&datazen_ui_api::ColumnSort>,
            _filter: Option<&datazen_ui_api::ColumnFilter>,
        ) -> Result<datazen_ui_api::TablePage, String> {
            Ok(datazen_ui_api::TablePage {
                table: _table.into(),
                columns: Vec::new(),
                rows: Vec::new(),
                page: _page,
                page_size: _page_size,
                total_rows: 0,
                sort: None,
                filter: None,
                primary_key: Vec::new(),
                read_only: None,
            })
        }

        fn commit_cells(
            &mut self,
            _table: &str,
            _edits: &[(u32, String, String)],
        ) -> Result<u32, String> {
            Ok(0)
        }
    }

    let mut adapter = EmptyRequiredPortsAdapter;
    assert_eq!(
        adapter.favorites().err().as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "favorites default port must fail closed even though list_connections answered Ok(empty)"
    );
    assert_eq!(
        adapter.snippets().err().as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "snippets default port must fail closed"
    );
    assert_eq!(
        adapter
            .start_export("t", "csv", "all", None)
            .err()
            .as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "start_export default port must fail closed"
    );
    assert_eq!(
        adapter.widget_bars(1).err().as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "widget_bars default port must fail closed"
    );
    assert_eq!(
        adapter.run_workflow("w").err().as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "run_workflow default port must fail closed"
    );
}

/// The browser-native halves are owned by the web shell, not by Rune. The task
/// brief names `crates/datazen-web/src/platform/web/`; that crate does not
/// exist — the real home is `packages/web/src/platform/web/` (see AGENTS.md:
/// `packages/web/` is the browser shell). `include_str!` makes the path check
/// compile-time; the content checks prove these are real transports.
#[test]
fn browser_native_transports_live_in_the_web_shell() {
    const TRANSPORT: &str = include_str!(
        "../../../packages/web/src/platform/web/backendTransport.ts"
    );
    const SERVICES: &str = include_str!(
        "../../../packages/web/src/platform/web/platformServices.ts"
    );
    const ROUTER: &str =
        include_str!("../../../packages/web/src/platform/web/windowRouter.ts");
    const ASSEMBLY: &str =
        include_str!("../../../packages/web/src/platform/web/assembly.ts");

    assert!(
        TRANSPORT.contains("createWebBackendTransport"),
        "the web shell must own the real browser backend transport"
    );
    assert!(
        TRANSPORT.contains("CapabilityUnsupported"),
        "out-of-ledger web calls still fail closed in the real transport"
    );
    assert!(
        TRANSPORT.contains("/api/v1"),
        "the transport binds the HTTP surface, not a fake stub"
    );
    assert!(
        SERVICES.contains("navigator.clipboard"),
        "the browser clipboard half lives in the web shell"
    );
    assert!(
        SERVICES.contains("window.open") && SERVICES.contains("fullscreenElement"),
        "window.open / fullscreen are web-shell bindings, not Rune ports"
    );
    assert!(
        ROUTER.contains("fullscreenElement"),
        "the browser window router lives in the web shell"
    );
    assert!(
        ASSEMBLY.contains("configurePlatform") && ASSEMBLY.contains("WEB_CAPABILITIES"),
        "the web assembly binds ports and the capability table"
    );
}

/// React's capability table is fail-closed (no silently-permissive bits). Rune
/// has no Rust equivalent — this test pins the React-side guarantee from the
/// real source so the GAP above is measured, not asserted from a helper.
#[test]
fn web_capability_table_is_fail_closed_and_has_no_rune_equivalent() {
    const CAPABILITIES: &str =
        include_str!("../../../packages/frontend/src/platform/capabilities.ts");
    const BINDINGS: &str = include_str!(
        "../../../packages/web/src/platform/web/capabilityBindings.ts"
    );

    assert!(
        CAPABILITIES.contains("updater: false"),
        "the web table must not silently grant the desktop-only updater"
    );
    assert!(
        CAPABILITIES.contains("versionSource: 'build-constant'"),
        "the web table's version source is the build constant, not a host probe"
    );
    assert!(
        BINDINGS.contains("CapabilityUnsupported"),
        "disabled capabilities must bind an all-throw port, never a permissive empty answer"
    );
    assert!(
        BINDINGS.contains("selectByCapability"),
        "table rows drive assembly-time downgrades, with no form-factor probe"
    );
}
