//! Settings vertical slice: domain vocabulary, repository boundary, and use cases.

pub mod application;
pub mod domain;

pub use application::{SettingsError, SettingsService};
pub use domain::{
    ExecutionSettings, MonitorEmailSettings, MonitorSettings, OnboardingState,
    PortableThemePreference, SettingsRepository, SettingsRepositoryError, SharedSettings,
    SqlFormatOptions, SqlSnippet, UserPreferences,
};
