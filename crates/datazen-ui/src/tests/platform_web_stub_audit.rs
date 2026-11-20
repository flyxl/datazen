//! Independent T094 audit of the `platform/web/stub.ts` parity ruling recorded
//! in `platform_web_stub.rs`. The ruling is X-class (no Rune product file), so
//! the audit measures the two source files the ruling leans on directly, not
//! the coder's own prose:
//!
//!   1. The React stub really declares all 12 `createWebPlatformServices`
//!      methods, all 5 exported factories and the key browser behaviors — the
//!      "complete mapping" claim is checked against the real source.
//!   2. Rune's shared API really exposes no browser-platform ports (clipboard /
//!      fullscreen / notification / window.open) — the "documented absence"
//!      claim is honest only if the API truly has no such seam.
//!   3. The coder's mapping table still lists every stub method, so dropping a
//!      table row (a listed mutation target) turns a test red.
//!
//! No geometry/a11y observation applies: this ruling adds no Rune component,
//! only a parity test module.

/// The React source the ruling maps, embedded verbatim.
const STUB_TS: &str = include_str!("../../../../packages/frontend/src/platform/web/stub.ts");

/// The Rune shared API where `DesktopAdapter` and its fail-closed defaults live.
const UI_API_RS: &str = include_str!("../../../../crates/datazen-ui-api/src/lib.rs");

/// The coder's ruling-test source (for the mapping-table completeness guard).
const RULING_RS: &str = include_str!("platform_web_stub.rs");

const SERVICE_METHOD_DECLS: &[&str] = &[
    "async saveTextWithDialog(",
    "async saveBinaryWithDialog(",
    "async openTextWithDialog(",
    "async openDirectoryWithDialog(",
    "async writeClipboard(",
    "async writeClipboardHtml(",
    "async readClipboard(",
    "async openExternal(",
    "async toggleFullscreen(",
    "async showNotification(",
    "async pickPath(",
    "async getOsPlatform(",
];

const EXPORTED_FACTORIES: &[&str] = &[
    "export function createWebBackendTransportStub(",
    "export function createWebPlatformServices(",
    "export function createWebWindowRouter(",
    "export function createWebEventChannel(",
    "export function assembleWebPlatformStub(",
];

/// Browser-only identifiers that must not appear in the Rune shared API. The
/// ruling maps them to "no equivalent / web shell owns it"; if one showed up
/// in `DesktopAdapter`, that documented absence would be a lie.
const BROWSER_ONLY_API_IDENTIFIERS: &[&str] = &[
    "clipboard",
    "ClipboardItem",
    "fullscreen",
    "Notification",
    "window.open",
    "WEB_CAPABILITIES",
    "PlatformCapabilities",
];

#[test]
fn react_stub_defines_every_mapped_service_method() {
    for decl in SERVICE_METHOD_DECLS {
        assert!(
            STUB_TS.contains(decl),
            "the ruling maps `{}` but the React source no longer declares it",
            decl
        );
    }
    assert_eq!(
        SERVICE_METHOD_DECLS.len(),
        12,
        "the source has 12 createWebPlatformServices methods, not the brief's 11"
    );
}

#[test]
fn react_stub_defines_every_exported_factory_and_behavior() {
    for factory in EXPORTED_FACTORIES {
        assert!(STUB_TS.contains(factory), "missing exported factory `{}`", factory);
    }
    for behavior in [
        "CapabilityUnsupported",
        "navigator.clipboard.writeText",
        "navigator.clipboard.readText",
        "window.open",
        "requestFullscreen",
        "exitFullscreen",
        "new Notification(",
        "createBrowserEventChannel",
        "createBlobTablesExporter",
        "WEB_CAPABILITIES",
    ] {
        assert!(STUB_TS.contains(behavior), "missing web behavior `{}`", behavior);
    }
}

#[test]
fn rune_shared_api_has_no_browser_platform_ports() {
    for ident in BROWSER_ONLY_API_IDENTIFIERS {
        assert!(
            !UI_API_RS.contains(ident),
            "DesktopAdapter API unexpectedly exposes browser-only `{}`; the ruling's documented absence would be false",
            ident
        );
    }
}

/// The coder's mapping table must still list every stub method and factory.
/// We look only inside the table section (before the "Note:" paragraph that
/// separately re-lists the names), so dropping a table row is actually caught
/// rather than masked by the prose elsewhere in the file.
#[test]
fn ruling_mapping_table_lists_every_stub_method() {
    const HEADER: &str = "| React factory / method | Rune counterpart, or honest absence |";
    let start = RULING_RS
        .find(HEADER)
        .expect("ruling must have a mapping table header");
    let after = &RULING_RS[start..];
    let end = after.find("//! Note:").unwrap_or(after.len());
    let table = &after[..end];

    const ROW_MARKERS: &[&str] = &[
        "`createWebPlatformServices.saveTextWithDialog`",
        "`saveBinaryWithDialog` →",
        "`openTextWithDialog` →",
        "`openDirectoryWithDialog` →",
        "`writeClipboard(text)` →",
        "`writeClipboardHtml({html, altText})` →",
        "`readClipboard()` →",
        "`openExternal({url, path})` →",
        "`toggleFullscreen()` →",
        "`showNotification({title, body})` →",
        "`pickPath()` →",
        "`getOsPlatform()` →",
        "`createWebBackendTransportStub`",
        "`createWebWindowRouter`",
        "`createWebEventChannel`",
        "`assembleWebPlatformStub`",
        "`WEB_CAPABILITIES`",
    ];
    for marker in ROW_MARKERS {
        assert!(
            table.contains(marker),
            "mapping table dropped a row for `{}`",
            marker
        );
    }
}
