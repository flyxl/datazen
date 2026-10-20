//! AI-related Tauri IPC commands.

mod chat;
mod config;
mod generate;
mod grant;
mod prompts;
mod util;

pub use chat::*;
pub use config::*;
pub use generate::*;
pub use grant::*;
pub use prompts::*;
pub(crate) use util::language_hint;

use crate::commands::error::CommandError;
use crate::commands::AppState;
use datazen_feature_core::ai::cancel::CancelReceipt;
use tauri::State;

/// Stable owner fingerprint for this process. AI grants and stream cancellation
/// are scoped to the same identity that admits owner intents, so a forged
/// request id cannot cancel another client's stream (CM-53).
pub(crate) fn ai_owner_fingerprint(state: &AppState) -> String {
    match state.platform.require() {
        Ok(adapter) => format!("ai:{}", adapter.identity().client_instance_id()),
        // Before identity assembly no AI stream can be registered either; the
        // constant keeps cancel/register symmetric instead of inventing an owner.
        Err(_) => "ai:unassembled-client".to_string(),
    }
}

/// Cancel an in-flight AI streaming request by its `request_id`.
///
/// Returns a receipt: marking only succeeds for the owning client, and an
/// unknown id reports that the request already finished.
#[tauri::command]
pub async fn ai_cancel(
    request_id: String,
    state: State<'_, AppState>,
) -> Result<CancelReceipt, CommandError> {
    let owner = ai_owner_fingerprint(&state);
    Ok(state.cancel_registry.cancel(&request_id, &owner).await)
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
