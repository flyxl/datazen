//! Where the app decides its data directory lives.
//!
//! The bundle-identifier default has to agree with the path the logger resolves
//! independently, or settings land in one directory while logs land in another.

use super::super::*;
use super::fixtures::*;

#[test]
fn default_app_data_dir_uses_bundle_identifier() {
    let _g = env_lock();
    std::env::remove_var("DATAZEN_DATA_DIR");
    let dir = DesktopRepository::default_app_data_dir().unwrap();
    assert!(
        dir.ends_with(APP_IDENTIFIER),
        "expected path ending with {APP_IDENTIFIER}, got {}",
        dir.display()
    );
}

#[test]
fn default_app_data_dir_matches_resolve_log_settings_path() {
    let _g = env_lock();
    std::env::remove_var("DATAZEN_DATA_DIR");
    let store_dir = DesktopRepository::default_app_data_dir().unwrap();
    let log_dir = dirs::data_dir()
        .map(|d| d.join(APP_IDENTIFIER))
        .expect("data dir");
    assert_eq!(store_dir, log_dir);
}

#[test]
fn data_dir_env_override_wins() {
    let _g = env_lock();
    let override_dir = std::env::temp_dir().join("datazen-e2e-override-test");
    std::env::set_var("DATAZEN_DATA_DIR", &override_dir);
    let dir = DesktopRepository::default_app_data_dir().unwrap();
    std::env::remove_var("DATAZEN_DATA_DIR");
    assert_eq!(dir, override_dir, "DATAZEN_DATA_DIR must win over default");
}
