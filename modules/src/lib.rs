//! `datazen-modules` owns feature-oriented domain vocabulary and application use cases.
//!
//! The crate intentionally has no dependency on a host, runtime, kernel,
//! `platform-api`, or Tauri. Hosts provide the repository implementations
//! declared by the enabled feature modules. The `ai` feature additionally
//! carries the AI provider adapters (merged 2026-10-26, D1.5): their outbound
//! HTTP client (`reqwest`) and the bottom-layer `datazen-infra` utilities are
//! deliberate, user-decided exceptions recorded in the refactor plan.

#[cfg(feature = "ai")]
pub mod ai;

#[cfg(feature = "connection")]
pub mod connection;

#[cfg(feature = "named_views")]
pub mod named_views;

#[cfg(feature = "settings")]
pub mod settings;

#[cfg(test)]
mod tests {
    #[test]
    fn the_modules_crate_has_a_real_root() {
        assert!(module_path!().ends_with("tests"));
    }
}
