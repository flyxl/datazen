//! AI-related Tauri IPC commands.

mod chat;
mod config;
mod generate;
mod prompts;
mod util;

pub use chat::*;
pub use config::*;
pub use generate::*;
pub use prompts::*;
pub(crate) use util::language_hint;

use crate::commands::error::CommandError;
use crate::commands::AppState;
use tauri::State;

/// Cancel an in-flight AI streaming request by its `request_id`.
///
/// Returns `true` if the request was found and cancelled, `false` otherwise.
#[tauri::command]
pub async fn ai_cancel(
    request_id: String,
    state: State<'_, AppState>,
) -> Result<bool, CommandError> {
    Ok(state.cancel_registry.cancel(&request_id).await)
}

#[cfg(test)]
pub(crate) use util::*;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod ipc_contract_guards;

#[cfg(test)]
mod mock_provider_tests;

#[cfg(test)]
mod tests;
