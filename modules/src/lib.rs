//! `datazen-modules` owns feature-oriented domain vocabulary and application use cases.
//!
//! The crate intentionally has no dependency on a host, runtime, kernel,
//! `platform-api`, Tauri, or an HTTP framework. Hosts provide the repository
//! implementations declared by the enabled feature modules.

#[cfg(feature = "settings")]
pub mod settings;

#[cfg(test)]
mod tests {
    #[test]
    fn the_modules_crate_has_a_real_root() {
        assert!(module_path!().ends_with("tests"));
    }
}
