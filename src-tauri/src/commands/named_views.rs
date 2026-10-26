//! The three named-view commands (08 §3.8).
//!
//! The shell does three things and nothing else: it takes the aggregate's
//! named-view port (P3: the host store no longer appears in this file), it
//! hands the shared layer that port and an id minter, it maps the shared error
//! onto [`CommandError`], and it hands the answer back. Every rule — the name uniqueness inside a scope,
//! the one-default-per-scope rule, the ordering, the idempotent delete — lives
//! in `datazen_modules::named_views`.
//!
//! The commands are **desktop-only for now**: the storage is the desktop
//! store's `{appData}/named_views.json`, and a browser build has no such
//! place. Its transport declares the gap as a capability gap, so the caller
//! gets an explicit "not supported" instead of an empty list that reads like
//! "you have no saved views".

use datazen_modules::named_views::{NamedView, NamedViewScope};
use tauri::State;

use super::error::CommandError;
use super::AppState;
use crate::platform::ipc::require_named_views_service;

/// The views of one scope, or of every scope when `scope` is omitted.
///
/// The order is the one the menu shows: the default view first, then the rest
/// by name.
pub(crate) async fn list_named_views_impl(
    state: &AppState,
    scope: Option<NamedViewScope>,
) -> Result<Vec<NamedView>, CommandError> {
    let service = require_named_views_service(state)?;
    let views = service.list(scope.as_ref()).await?;
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
    let service = require_named_views_service(state)?;
    let saved = service.save(view).await?;
    Ok(saved)
}

/// Delete a view and return the remaining ones.
///
/// Deleting an id nobody carries is not an error.
pub(crate) async fn delete_named_view_impl(
    state: &AppState,
    id: &str,
) -> Result<Vec<NamedView>, CommandError> {
    let service = require_named_views_service(state)?;
    let remaining = service.delete(id).await?;
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

    /// 生产半边不再出现宿主 store（P3 第一条命令族；D6 item 1 后命令改经
    /// `require_named_views_service` 取模块服务，守卫随迁）。
    ///
    /// 计数而不是「有没有」：端口宿主 store 的直读曾经是三条命令各一处，
    /// 只要有人把 `.as_ref()` 换回 `state.store.as_ref()`，这里会先红。
    #[test]
    fn the_command_shell_reads_the_aggregate_not_the_host_store() {
        let source = include_str!("named_views.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert_eq!(
            production.matches("state.store").count(),
            0,
            "named views 命令必须走模块服务，不得直读宿主 store"
        );
        assert_eq!(
            production
                .matches("require_named_views_service(state)")
                .count(),
            3,
            "三条命令各取一次模块服务"
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
