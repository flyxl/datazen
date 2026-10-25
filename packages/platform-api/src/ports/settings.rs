//! Compatibility export for the settings repository now owned by the settings
//! module. Host adapters implement the module-owned trait directly.

pub use datazen_modules::settings::{SettingsRepository, SettingsRepositoryError};
