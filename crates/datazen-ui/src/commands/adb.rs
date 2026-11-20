//! Port of `packages/frontend/src/commands/adb.ts` (T047).
//!
//! IPC refactor decision 2: the ADB helpers are SQLite-driver commands
//! (`requires_connection = false`) executed through the unified
//! `execute_driver_command(driverType: "sqlite", …)` entry instead of
//! dedicated host IPCs. The JSON shapes match the former host commands so
//! component types stay unchanged — the driver side
//! (`crates/drivers/sqlite/src/adb.rs`) declares the three commands and
//! their metadata.
//!
//! # What maps where
//!
//! - `driverCommands.execute(...)` → [`DesktopAdapter::execute_driver_command`];
//!   the `result.data as T` cast becomes the typed serde deserialization
//!   (a shape mismatch surfaces as an error instead of a lying cast).
//! - `adbPullDatabaseWithDialog`'s `savedPath ?? null` collapse: the host
//!   thin shell has already replaced the saveDialog payload with
//!   `{ savedPath }` (null when cancelled) by the time the adapter answers
//!   here — the port keeps the `?? null` normalization for a missing key
//!   verbatim.
//! - The `async` wrappers become synchronous adapter calls (the UI's frame
//!   loop never blocks: long work goes through `begin_job`, which these
//!   quick device listings do not use — same as the React fire-and-await
//!   helpers, minus the JS event loop).

use serde::Deserialize;

use crate::state_def::AppState;

/// The ADB commands' driver (declared by `crates/drivers/sqlite`).
pub(crate) const ADB_DRIVER_TYPE: &str = "sqlite";

/// An installed device package (`adb_list_packages` rows).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct AdbPackage {
    pub(crate) package_name: String,
}

/// A device database file (`adb_list_databases` rows).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(crate) struct AdbDatabaseFile {
    pub(crate) path: String,
    pub(crate) name: String,
}

/// The unified entry every helper below routes through (React's
/// `executeAdbCommand`): unbound driver command, no connection session.
fn execute_adb_command(
    state: &mut AppState,
    command: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    state.desktop.execute_driver_command(ADB_DRIVER_TYPE, command, input)
}

/// List the device's installed packages.
pub(crate) fn adb_list_packages(state: &mut AppState) -> Result<Vec<AdbPackage>, String> {
    let data = execute_adb_command(state, "adb_list_packages", &serde_json::json!({}))?;
    serde_json::from_value(data).map_err(|error| error.to_string())
}

/// List one package's database files.
pub(crate) fn adb_list_databases(
    state: &mut AppState,
    package_name: &str,
) -> Result<Vec<AdbDatabaseFile>, String> {
    let data = execute_adb_command(
        state,
        "adb_list_databases",
        &serde_json::json!({ "package": package_name }),
    )?;
    serde_json::from_value(data).map_err(|error| error.to_string())
}

/// Pull a device database and save it through the host's native save
/// dialog.
///
/// The sqlite driver command declares a saveDialog in its metadata: it
/// returns `{ fileName, dataBase64 }`, the host thin shell pops the
/// native dialog, writes the bytes and replaces the payload with
/// `{ savedPath }` (null when cancelled). No path ever travels from the
/// UI to the filesystem.
pub(crate) fn adb_pull_database_with_dialog(
    state: &mut AppState,
    package_name: &str,
    db_path: &str,
) -> Result<Option<String>, String> {
    #[derive(Deserialize)]
    struct PullResult {
        #[serde(rename = "savedPath")]
        saved_path: Option<String>,
    }
    let data = execute_adb_command(
        state,
        "adb_pull_database",
        &serde_json::json!({ "package": package_name, "dbPath": db_path }),
    )?;
    let result: PullResult = serde_json::from_value(data).map_err(|error| error.to_string())?;
    Ok(result.saved_path)
}
