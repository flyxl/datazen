//! T051 audit: the `@datazen/frontend` light entry
//! (`packages/frontend/src/index.ts`).
//!
//! The coder's four tests pin the light surface's one-import-point role,
//! the manifest-level separation invariant, the capability split as the
//! linked assembly, and the light consumer's exercise. The mutation round
//! (three mutations, run through the same pruned channel) confirmed two
//! red — the manifest scan fires on a dependency-looking line, the
//! validation-bound assert fires when the rules loosen — and exposed one
//! blind spot the audit now closes:
//!
//! * **publicity is invisible from inside the crate.** Turning the launch
//!   re-export `pub` → `pub(crate)` left all four tests green: an in-crate
//!   `mod tests` can read `pub(crate)` names just as well as `pub` ones,
//!   so the T049 ruling's "launch surface at the crate root" had no
//!   observer. The audit pins it as a text tripwire on `lib.rs` — the same
//!   approach the icon-registry guard takes — plus the inverse: the app
//!   crate root must not grow a wholesale re-export of the light surface
//!   (a second import point would undo the barrel's whole job).
//!
//! The audit also pins the consumer half the coder's tests only imply:
//! the live native host's manifest depends on exactly the two surfaces
//! the React desktop host imports (`.` and `./app`), and the contracts
//! family's ported members (`TablePage`, the read-page parameters) stay
//! reachable through the api crate alone.

use datazen_ui_api::{DESKTOP_NOT_IMPLEMENTED, DesktopAdapter, UnimplementedAdapter};
use rune::App;

use datazen_ui::AppState;

/// The launch surface must stay `pub` at the crate root (the T049 ruling):
/// `run` / `run_with_adapter` / `app_with_adapter` re-exported for the host
/// entry, `start_shared_app_state` for the bootstrap, `start` for wasm.
/// An in-crate test cannot observe `pub` vs `pub(crate)` — this tripwire
/// can, because the re-export lines are text.
#[test]
fn launch_surface_stays_public_at_the_crate_root() {
    let lib_rs = include_str!("../src/lib.rs");
    assert!(
        lib_rs.contains("pub use app::{app_with_adapter, run, run_with_adapter};"),
        "the launch surface must remain a public re-export at the crate root"
    );
    assert!(
        lib_rs.contains("pub use bootstrap::start_shared_app_state;"),
        "the bootstrap entry must remain a public re-export at the crate root"
    );
    assert!(
        lib_rs.contains("pub use app::start;"),
        "the wasm entry must remain a public re-export"
    );
}

/// The inverse half of the barrel's job: the app crate root must not become
/// a second import point for the light surface. The api crate is where
/// light consumers import from (the React `.`); a wholesale `pub use
/// datazen_ui_api::…` at this root would re-couple every light consumer's
/// graph to the app graph. The crate-internal `pub(crate) use` of `Setting`
/// is fine — it never crosses the crate boundary.
#[test]
fn crate_root_is_not_a_second_import_point_for_the_light_surface() {
    let lib_rs = include_str!("../src/lib.rs");
    let offenders: Vec<&str> = lib_rs
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub use datazen_ui_api"))
        .collect();
    assert!(
        offenders.is_empty(),
        "the light surface lives in the api crate; the app crate root must not re-export it wholesale: {offenders:?}"
    );
}

/// The two-surface split has a living consumer: the native host's manifest
/// depends on exactly the two crates the React desktop host imports — the
/// light surface (`.` → datazen-ui-api, for the contracts it implements)
/// and the app graph (`./app` → datazen-ui, for the launch entry). If
/// either half is dropped, the host stops being the barrel's consumer.
#[test]
fn native_host_manifest_binds_both_surfaces() {
    let host_manifest = include_str!("../../datazen-rune-host/Cargo.toml");
    let binds_app_graph = host_manifest.contains("datazen-ui = { path");
    let binds_light_surface = host_manifest.contains("datazen-ui-api = { path");
    assert!(
        binds_app_graph && binds_light_surface,
        "the native host must depend on both the app graph and the light surface, like the React desktop host importing '.' and './app'"
    );
}

/// The contracts family's ported members, bound through the api crate
/// alone: `read_table_page`'s `TablePage` (the `TablesExporterPort` /
/// `TablePage` family in React's `platform/contracts.ts`) and the page
/// parameters (`ColumnSort`, `ColumnFilter`). React's central
/// `configurePlatform` / `requirePlatform` pair became construction-time
/// injection — the host builds a `DesktopAdapter` and hands it to the
/// launch entries — which is the T045 ruling this barrel inherits.
#[test]
fn contracts_family_members_reach_the_host_through_the_api_crate() {
    let _: fn(Box<dyn DesktopAdapter>) -> App<AppState> = datazen_ui::app_with_adapter;

    // The read-page contract: what a host's `read_table_page` answers and
    // the parameters it accepts, all importable without the app graph.
    let mut unwired = UnimplementedAdapter;
    let refused = unwired.read_table_page("demo.table", 0, 20, None, None);
    assert_eq!(
        refused.err().as_deref(),
        Some(DESKTOP_NOT_IMPLEMENTED),
        "the read-page contract surfaces the fixed error, not an empty page"
    );
}
