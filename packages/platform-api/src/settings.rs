//! Compatibility exports for the settings vocabulary now owned by
//! `datazen-modules::settings`.
//!
//! D1 keeps this import path for existing feature consumers. The definitions,
//! normalization, and validation live in exactly one crate: `datazen-modules`.

pub use datazen_modules::settings::{
    ExecutionSettings, PortableThemePreference, SharedSettings, SqlFormatOptions, SqlSnippet,
    UserPreferences,
};
