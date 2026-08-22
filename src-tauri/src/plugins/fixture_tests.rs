//! Guards the F9 E2E fixture (`e2e/fixtures/sample-plugin/`) against rot.
//!
//! The sample plugin package must keep passing the full manifest rule set
//! (§2.2 rules 1–7) and the real directory-install path, otherwise every
//! journey in `e2e/specs/plugins.spec.ts` fails for fixture reasons instead of
//! product reasons. Reads the fixture relative to `CARGO_MANIFEST_DIR` so the
//! check runs both locally and in CI without any setup.

use std::fs;
use std::path::PathBuf;

use super::install::install_from_dir;
use super::manifest::{parse_manifest, validate_manifest, validate_plugin_dir};
use super::PLUGIN_API_VERSION;

fn sample_plugin_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../e2e/fixtures/sample-plugin")
}

fn load_fixture_manifest() -> (PathBuf, super::PluginManifest) {
    let dir = sample_plugin_fixture();
    let content = fs::read_to_string(dir.join("manifest.json"))
        .unwrap_or_else(|e| panic!("read fixture manifest.json: {e}"));
    let manifest = parse_manifest(&content)
        .unwrap_or_else(|e| panic!("fixture manifest.json must parse: {e}"));
    (dir, manifest)
}

#[test]
fn e2e_sample_plugin_fixture_passes_manifest_validation() {
    let (dir, manifest) = load_fixture_manifest();

    // Full rule set 1–7 against the on-disk package (entry/icon/tokens paths,
    // extension whitelist, svg scan, quotas). The source folder name is a repo
    // path and need not equal the id — real installs rename staging to `{id}`.
    validate_manifest(&manifest, &dir)
        .unwrap_or_else(|e| panic!("sample-plugin fixture drifted out of spec: {e}"));

    assert_eq!(manifest.id, "datazen.sample");
    assert_eq!(manifest.api_version, PLUGIN_API_VERSION);
    assert_eq!(manifest.name, "Sample Hello");
    assert_eq!(manifest.entry.as_deref(), Some("index.html"));
    assert_eq!(manifest.contributes.pages.len(), 1);
    assert_eq!(manifest.contributes.pages[0].id, "hello");
    assert_eq!(manifest.contributes.pages[0].show_in, "workspace");
    assert_eq!(manifest.contributes.themes.len(), 1);
    assert_eq!(manifest.contributes.themes[0].id, "sample-light");
    assert_eq!(manifest.contributes.themes[0].modes, vec!["light"]);
    assert_eq!(
        manifest
            .permissions
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["context:connections", "command:invoke", "storage:local"]
    );
}

#[test]
fn e2e_sample_plugin_fixture_declares_all_required_files() {
    let dir = sample_plugin_fixture();
    for rel in [
        "index.html",
        "assets/app.js",
        "assets/icon.svg",
        "themes/sample/tokens.css",
    ] {
        assert!(
            dir.join(rel).is_file(),
            "fixture file missing: {rel} (the E2E journeys and bridge page depend on it)"
        );
    }
}

#[test]
fn e2e_sample_plugin_fixture_installs_through_the_real_path() {
    let (dir, _) = load_fixture_manifest();

    let plugins_root = tempfile::TempDir::new().unwrap();
    let manifest = install_from_dir(&dir, plugins_root.path())
        .unwrap_or_else(|e| panic!("directory install of the fixture must succeed: {e}"));

    // The staged copy was renamed to `{plugins_dir}/{id}` and revalidates
    // cleanly there (this also enforces folder name == manifest.id).
    let installed = plugins_root.path().join("datazen.sample");
    validate_plugin_dir(&installed)
        .unwrap_or_else(|e| panic!("installed fixture must revalidate: {e}"));
    assert_eq!(manifest.id, "datazen.sample");
    assert!(installed.join("index.html").is_file());
}
