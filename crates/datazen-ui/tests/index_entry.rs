//! T051 — `@datazen/frontend` light entry (`packages/frontend/src/index.ts`).
//!
//! React's `index.ts` is the barrel that keeps the package's surface narrow:
//! `.` re-exports the platform ports and capability bits plus the
//! host-support helpers the desktop adapters consume, and keeps the app
//! graph behind `./app` so a light consumer's test graph never has to know
//! it exists. In Rune the barrel dissolves into the crate split — the light
//! surface is the `datazen-ui-api` crate, the launch surface is this
//! crate's root (the T049 ruling) — so there is no `index.rs` to write;
//! these tests pin the barrel contract from the consumer's side, headless
//! (no window is ever opened):
//!
//! 1. one import point: every contracts-family head the React barrel
//!    re-exported from `.` is reachable through a single `use` of the api
//!    crate — the barrel's role became the crate boundary;
//! 2. the separation invariant: the light crate does not depend on the app
//!    crate (the direction that keeps `.` light is a cargo fact, pinned as
//!    a manifest scan);
//! 3. capability bits are the linked assembly, not a runtime table — the
//!    desktop form binds the native launch surface, the web form the wasm
//!    entry (`IS_DESKTOP_BUILD` folded at compile time both sides);
//! 4. a light consumer's exercise: the host-support half (a typed settings
//!    entry's validation, the fixed not-implemented error) works with no
//!    `AppState`, no launch entry and no window in scope.

use datazen_ui_api::{
    ConnectionProfile, DemoAdapter, DesktopAdapter, McpHttpResponse, Setting, UnimplementedAdapter,
    DESKTOP_NOT_IMPLEMENTED,
};
use rune::App;

use datazen_ui::AppState;

/// The barrel's single-import-point role: the contracts family the React
/// barrel re-exported from `.` (`export * from './platform/contracts'` plus
/// the `AppSettings`/`McpHttpStatus` types) is the api crate's surface.
/// These bindings fail the moment a head silently changes shape or moves.
#[test]
fn light_surface_is_one_import_point() {
    // contracts: the two adapters a host assembles around (unwired + host)
    let unwired: Box<dyn DesktopAdapter> = Box::new(UnimplementedAdapter);
    let host: Box<dyn DesktopAdapter> =
        Box::new(DemoAdapter::with_connections(vec![ConnectionProfile {
            id: "light-one".into(),
            name: "Light One".into(),
            engine: "sqlite".into(),
            host: None,
        }]));
    let _ = (unwired, host);

    // `McpHttpStatus` (types/index): the status channel rides on the
    // mcpClient port's response, not a standalone type.
    let response = McpHttpResponse {
        ok: true,
        status: 200,
        status_text: "OK".into(),
        headers: Vec::new(),
        body: String::new(),
    };
    let status: u16 = response.status;
    assert_eq!(status, 200);

    // `AppSettings` (types/index): today's typed settings entry is the
    // `Setting` enum, and it carries the product's validation rules.
    let setting = Setting::Layout {
        key: "navigator-width".into(),
        value: 260,
    };
    assert_eq!(setting.validate(), Ok(()));
}

/// What keeps `.` light in Rust: the light crate must not depend on the app
/// crate. Cargo would reject the cycle, so the direction
/// datazen-ui -> datazen-ui-api is the invariant; this scan pins it where
/// it is written down, so a future dependency line fails here instead of
/// in a host that quietly stopped being light.
#[test]
fn light_crate_does_not_depend_on_the_app_crate() {
    let manifest = include_str!("../../datazen-ui-api/Cargo.toml");
    let offenders: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("datazen-ui ")
                || line.starts_with("datazen-ui=")
                || line.starts_with("datazen-ui.")
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "the light surface crate must stay independent of the app graph: {offenders:?}"
    );
}

/// Capability bits are a compile-time property of the form factor (React:
/// `DESKTOP_CAPABILITIES` / `WEB_CAPABILITIES` / `IS_DESKTOP_BUILD`, "a
/// plain module constant … no runtime environment sniffing anywhere"). The
/// Rune form factor is the linked assembly: the desktop form's table is the
/// native launch surface with host injection, the web form's is the wasm
/// entry. No runtime table exists to re-export and none is invented.
#[test]
fn capability_bits_are_the_linked_assembly() {
    // `IS_DESKTOP_BUILD`: this build links the desktop assembly — a target
    // fact, folded at compile time exactly like the React constant.
    let is_desktop_build = cfg!(not(target_arch = "wasm32"));

    // The desktop form's table: the launch surface, host-injectable.
    let run: fn() = datazen_ui::run;
    let app_with_adapter: fn(Box<dyn DesktopAdapter>) -> App<AppState> =
        datazen_ui::app_with_adapter;
    let _ = (run, app_with_adapter);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let run_with_adapter: fn(Box<dyn DesktopAdapter>) = datazen_ui::run_with_adapter;
        let _ = run_with_adapter;
    }
    // The web form's table: the wasm entry (bound only where it exists —
    // the same cfg the crate root gates the re-export with).
    #[cfg(target_arch = "wasm32")]
    {
        let start: fn() = datazen_ui::start;
        let _ = start;
    }
    let _ = is_desktop_build;
}

/// The light consumer's exercise: everything below uses only the light
/// surface — no `AppState`, no launch entry, no window — which is the whole
/// point of keeping `./app` out of `.`. The unwired adapter answers the
/// fixed not-implemented sentence (never a silent empty answer), and a
/// settings entry rejects the same out-of-range value both ports reject.
#[test]
fn light_consumer_needs_no_app_graph() {
    let unwired = UnimplementedAdapter;
    assert_eq!(
        unwired.read_ui_settings().err().as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "an unwired host must surface the fixed error, not an empty answer"
    );

    let oversized = Setting::TablePageSize { rows: 501 };
    assert_eq!(
        oversized.validate(),
        Err("table page size must be 1..=500, got 501".to_string()),
        "the product's validation rules live on the light surface"
    );
}
