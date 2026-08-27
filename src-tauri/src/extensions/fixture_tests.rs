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
use super::manifest::{parse_manifest, validate_extension_dir, validate_manifest};
use super::EXTENSION_API_VERSION;

fn sample_plugin_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../e2e/fixtures/sample-plugin")
}

fn load_fixture_manifest() -> (PathBuf, super::ExtensionManifest) {
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
    assert_eq!(manifest.api_version, EXTENSION_API_VERSION);
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
    validate_extension_dir(&installed)
        .unwrap_or_else(|e| panic!("installed fixture must revalidate: {e}"));
    assert_eq!(manifest.id, "datazen.sample");
    assert!(installed.join("index.html").is_file());
}

/// Source tree for repo-bundled extension packages (`packages/extensions/`).
fn extensions_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../packages/extensions")
}

/// Guards every shipped extension package against rot: each directory under
/// `packages/extensions/` must keep passing the full manifest rule set
/// (§2.2 rules 1–7, including folder name == manifest.id) so the samples stay
/// installable from the management page without any preprocessing.
#[test]
fn repo_extension_packages_pass_manifest_validation() {
    let root = extensions_root();
    let mut seen = std::collections::BTreeSet::new();
    for entry in fs::read_dir(&root).expect("read packages/extensions") {
        let path = entry.expect("dir entry").path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if !name.contains('.') {
            continue; // skip non-package helper directories (e.g. dist/)
        }
        let manifest = validate_extension_dir(&path)
            .unwrap_or_else(|e| panic!("extension package `{name}` failed validation: {e}"));
        assert_eq!(
            manifest.api_version, EXTENSION_API_VERSION,
            "extension `{name}` apiVersion drifted"
        );
        seen.insert(manifest.id);
    }

    assert!(
        seen.contains("community.slate-blue"),
        "converted theme extension missing, saw {seen:?}"
    );
    assert!(
        seen.contains("datazen.playground"),
        "sample extension missing, saw {seen:?}"
    );
}

/// The converted community theme extension must declare its theme
/// contribution with a tokens.css that exists on disk (rule 5) — this is what
/// Settings → Appearance lists once installed.
#[test]
fn community_slate_blue_extension_declares_theme_contribution() {
    let dir = extensions_root().join("community.slate-blue");
    let manifest = validate_extension_dir(&dir)
        .unwrap_or_else(|e| panic!("community.slate-blue extension invalid: {e}"));
    assert!(
        manifest.entry.is_none(),
        "pure-theme extension needs no entry"
    );
    assert!(
        manifest.permissions.is_empty(),
        "pure-theme extension needs no permissions"
    );

    let themes: Vec<_> = manifest.contributes.themes.iter().collect();
    assert_eq!(themes.len(), 1);
    assert_eq!(themes[0].id, "slate-blue");
    assert_eq!(
        themes[0].modes,
        vec!["light".to_string(), "dark".to_string()]
    );
    assert!(dir.join(&themes[0].tokens_css).is_file());
    // Legacy ThemePack capabilities must survive the conversion: chart
    // palette + semantic icon overrides ride along with the theme.
    let charts = themes[0]
        .charts_json
        .as_deref()
        .unwrap_or_else(|| panic!("slate-blue must declare chartsJson"));
    let icons = themes[0]
        .icons_dir
        .as_deref()
        .unwrap_or_else(|| panic!("slate-blue must declare iconsDir"));
    assert!(dir.join(charts).is_file(), "charts.json missing: {charts}");
    assert!(dir.join(icons).is_dir(), "icons dir missing: {icons}");
    assert!(
        dir.join(icons).join("nav.settings.svg").is_file(),
        "icons dir must contain semantic overrides"
    );
}
