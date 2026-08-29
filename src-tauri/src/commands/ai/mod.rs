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
pub(crate) use util::*;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod ipc_contract_guards;

#[cfg(test)]
mod mock_provider_tests;

#[cfg(test)]
mod tests;
