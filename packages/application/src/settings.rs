//! Compatibility re-export for the D1 settings use case.
//!
//! The service moved to `datazen_modules::settings` in D1. Existing imports may
//! use this module until the remaining `ApplicationServices` compatibility
//! surface is removed in the D1 cleanup phase; there is no second implementation.

pub use datazen_modules::settings::{SettingsError, SettingsService};
