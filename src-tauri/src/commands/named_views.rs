//! The three named-view commands (08 §3.8).
//!
//! The shell does three things and nothing else: it hands the shared layer a
//! store and an id minter, it maps the shared error onto [`CommandError`], and
//! it hands the answer back. Every rule — the name uniqueness inside a scope,
//! the one-default-per-scope rule, the ordering, the idempotent delete — lives
//! in `datazen_feature_core::services::named_views`.
//!
//! The commands are **desktop-only for now**: the storage is the desktop
//! store's `{appData}/named_views.json`, and a browser build has no such
//! place. Its transport declares the gap as a capability gap, so the caller
//! gets an explicit "not supported" instead of an empty list that reads like
//! "you have no saved views".

use datazen_feature_core::services::named_views::{
    delete_named_view as delete_view, list_named_views as list_views, save_named_view as save_view,
    NamedView, NamedViewScope,
};
use tauri::State;

use super::error::CommandError;
use super::AppState;
use crate::store::HostViewIdMinter;

/// The views of one scope, or of every scope when `scope` is omitted.
///
/// The order is the one the menu shows: the default view first, then the rest
/// by name.
pub(crate) async fn list_named_views_impl(
    state: &AppState,
    scope: Option<NamedViewScope>,
) -> Result<Vec<NamedView>, CommandError> {
    let views = list_views(state.store.as_ref(), scope.as_ref()).await?;
    Ok(views)
}

/// Save a view and return the record as it was stored.
///
/// The answer carries the minted id and the timestamps, so the frontend can
/// replace its local copy instead of guessing them.
pub(crate) async fn save_named_view_impl(
    state: &AppState,
    view: NamedView,
) -> Result<NamedView, CommandError> {
    let saved = save_view(state.store.as_ref(), &HostViewIdMinter, view).await?;
    Ok(saved)
}

/// Delete a view and return the remaining ones.
///
/// Deleting an id nobody carries is not an error.
pub(crate) async fn delete_named_view_impl(
    state: &AppState,
    id: &str,
) -> Result<Vec<NamedView>, CommandError> {
    let remaining = delete_view(state.store.as_ref(), id).await?;
    Ok(remaining)
}

/// The saved views of one scope (or of every scope when `scope` is omitted).
#[tauri::command]
pub async fn list_named_views(
    state: State<'_, AppState>,
    scope: Option<NamedViewScope>,
) -> Result<Vec<NamedView>, CommandError> {
    list_named_views_impl(&state, scope).await
}

/// Save a named view; the backend mints the id and stamps the timestamps.
#[tauri::command]
pub async fn save_named_view(
    state: State<'_, AppState>,
    view: NamedView,
) -> Result<NamedView, CommandError> {
    save_named_view_impl(&state, view).await
}

/// Delete a named view; the remaining views come back with the receipt.
#[tauri::command]
pub async fn delete_named_view(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<NamedView>, CommandError> {
    delete_named_view_impl(&state, &id).await
}

#[cfg(test)]
mod tests {
    /// The host's handler list.
    ///
    /// A Tauri command that is not registered is never called and produces no
    /// compile error, so the registration has to be pinned by a test — the
    /// same discipline `ipc_surface_tests.rs` applies to the merged IPC.
    const BOOTSTRAP_RS: &str = include_str!("../bootstrap/run.rs");

    #[test]
    fn the_three_commands_are_registered() {
        assert!(
            BOOTSTRAP_RS.contains("commands::list_named_views,"),
            "list_named_views must be registered"
        );
        assert!(
            BOOTSTRAP_RS.contains("commands::save_named_view,"),
            "save_named_view must be registered"
        );
        assert!(
            BOOTSTRAP_RS.contains("commands::delete_named_view,"),
            "delete_named_view must be registered"
        );
    }

    #[test]
    fn the_command_shell_holds_no_rule() {
        // The rules live in the shared layer. A rule that crept into the
        // command shell would exist twice — once for the desktop and once
        // for whatever form factor comes next — and the two would drift.
        //
        // The needles are assembled at run time so this test's own source
        // never contains a contract code (the front-end registration guard
        // scans the Rust sources for them).
        let grid = "grid";
        let view = "view";
        let name_exists = "nameExists";
        let invalid = "invalid";
        let source = include_str!("named_views.rs");
        for forbidden in [
            format!("{grid}.{view}.{name_exists}"),
            format!("{grid}.{view}.{invalid}"),
        ] {
            assert!(
                !source.contains(&forbidden),
                "the command shell must not raise {forbidden} itself"
            );
        }
    }
}
