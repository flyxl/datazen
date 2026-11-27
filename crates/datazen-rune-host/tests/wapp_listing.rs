//! Installed registry integration through the stock native adapter and real
//! Rune workspace rail. Fixtures are local tempdirs with the original disk
//! format, never a connected user's profile or an injected UI list.
#![cfg(unix)]

use datazen::workspace_apps::{manager::WappManager, summary::WappSummary as OriginalSummary};
use datazen_rune_host::{FileConnectionRepository, LiveAdapter};
use datazen_ui::{app_poll, i18n::t, start_shared_app_state, view, AppState, Section};
use datazen_ui_api::{types::wapp::WappSummary, DesktopAdapter, WappListError};
use rune::{Env, Runtime};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

fn host(data: &Path) -> LiveAdapter {
    let repo = FileConnectionRepository::open(&data.join("connections.json")).unwrap();
    LiveAdapter::with_data_dir(Arc::new(repo), data)
}

fn fixture(data: &Path, id: &str, name: &str, page: bool, theme: bool, enabled: bool) -> PathBuf {
    let dir = data.join("wapps").join(id);
    fs::create_dir_all(&dir).unwrap();
    let mut manifest = json!({
        "id": id, "name": name, "author": "Local Fixture", "description": "Read-only fixture",
        "version": "1.2.0", "apiVersion": 2, "permissions": ["storage:local"],
        "contributes": { "pages": [], "themes": [] }
    });
    if page {
        manifest["entry"] = json!("index.html");
        manifest["contributes"]["pages"] = json!([{ "id": "main", "title": "Main" }]);
        fs::write(dir.join("index.html"), "<script>throw Error('never evaluate');</script>").unwrap();
    }
    if theme {
        manifest["contributes"]["themes"] = json!([
            { "id": "demo-dark", "name": "Demo Dark", "tokensCss": "tokens.css", "modes": ["dark"] }
        ]);
        fs::write(dir.join("tokens.css"), ":root { --accent: #f0a; }").unwrap();
    }
    if enabled {
        fs::write(dir.join(".enabled"), "").unwrap();
    }
    fs::write(dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
    dir
}

fn runtime(data: &Path) -> Runtime<AppState> {
    let state = start_shared_app_state(Some(Box::new(host(data))));
    let mut rt = Runtime::new(state, view, (1440.0, 900.0), Env { system_dark: true, scale: 1.0 });
    rt.poll = Some(app_poll);
    rt.frame(0.0);
    rt
}

fn press(rt: &mut Runtime<AppState>, key: &str) {
    rt.settle_all();
    rt.frame(1.0 / 60.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn visible(rt: &Runtime<AppState>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements().into_iter().find(|node| !node.exiting && node.key.as_deref() == Some(key))
}

fn until(rt: &mut Runtime<AppState>, ready_key: &str) {
    let until = Instant::now() + Duration::from_secs(5);
    while visible(rt, ready_key).is_none() {
        assert!(Instant::now() < until, "timed out waiting for {ready_key}");
        std::thread::sleep(Duration::from_millis(5));
        rt.frame(1.0 / 60.0);
    }
}

fn assert_unreadable(adapter: &mut LiveAdapter) {
    assert!(adapter.begin_wapp_list(4));
    assert!(!adapter.begin_wapp_list(5), "one in-flight list per adapter");
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(answer) = adapter.poll_wapp_list() {
            assert_eq!(answer.generation, 4);
            assert_eq!(answer.result, Err(WappListError::Unreadable));
            return;
        }
        assert!(Instant::now() < until, "worker did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn stock_empty_registry_is_genuine_and_does_not_consult_the_tauri_profile() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let native = root.join("datazen-native");
    fs::create_dir_all(&native).unwrap();
    fixture(&root.join("com.tbeasy.datazen"), "test.foreign", "Hidden Foreign App", true, false, true);
    let mut rt = runtime(&native);
    press(&mut rt, "rail-wapps");
    until(&mut rt, "extension-page-empty");
    assert_eq!(rt.state.active, Section::Wapps);
    assert!(rt.state.wapps.is_empty());
    assert!(!native.join("wapps").exists(), "a missing registry stays missing");
    assert!(visible(&rt, "extension-page-error").is_none());
    assert!(visible(&rt, "extension-card-test.foreign").is_none());
    assert_eq!(
        visible(&rt, "extension-page-count").unwrap().text.as_deref(),
        Some(t("extensions.page.count").replace("{count}", "0").as_str())
    );
}

#[test]
fn stock_registry_uses_original_validated_rows_and_retains_last_good_on_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let enabled = fixture(&root, "test.workspace", "Alpha", true, false, true);
    let disabled = fixture(&root, "test.theme", "Beta", false, true, false);
    fixture(&root, "test.dual", "Beta", true, true, true);
    fs::create_dir_all(root.join("wapps/.staging")).unwrap();
    fs::create_dir_all(root.join("wapps/foreign")).unwrap();
    fs::write(root.join("wapps/README"), b"not a package").unwrap();
    let manifest = fs::read(enabled.join("manifest.json")).unwrap();
    let original = WappManager::new(root.join("wapps"));
    assert_eq!(original.load_from_disk(), 3);
    let projections: Vec<WappSummary> = original.list().iter().map(|wapp| {
        serde_json::from_value(serde_json::to_value(OriginalSummary::from(wapp)).unwrap()).unwrap()
    }).collect();
    let mut adapter = host(&root);
    assert!(adapter.begin_wapp_list(37));
    let deadline = Instant::now() + Duration::from_secs(5);
    let actual = loop {
        if let Some(answer) = adapter.poll_wapp_list() {
            assert_eq!(answer.generation, 37);
            break answer.result.unwrap();
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(actual, projections, "same sorted manifest projection as Tauri");

    let mut rt = runtime(&root);
    press(&mut rt, "rail-wapps");
    until(&mut rt, "extension-card-test.workspace");
    assert_eq!(rt.state.wapps.iter().map(|wapp| wapp.id.as_str()).collect::<Vec<_>>(),
        ["test.workspace", "test.dual", "test.theme"]);
    assert!(visible(&rt, "extension-page-error").is_none());
    assert!(visible(&rt, "extension-page-read-only").is_some());
    assert!(visible(&rt, "extension-card-test.theme").is_none(), "default Workspace chip");
    for control in ["toggle", "open", "uninstall"] {
        let key = format!("extension-card-test.workspace-{control}");
        assert!(!visible(&rt, &key).unwrap().focusable);
        press(&mut rt, &key);
    }
    assert_eq!(rt.state.active, Section::Wapps);
    assert!(rt.state.wapps[0].enabled);
    assert!(rt.state.wapp_remove_request.is_none());
    assert!(rt.state.set_wapp_enabled("test.workspace", false).is_err());
    assert!(!rt.state.open_wapp_page("test.workspace"));
    assert!(rt.state.remove_wapp("test.workspace").is_err());

    press(&mut rt, "extension-filter-theme");
    until(&mut rt, "extension-card-test.theme");
    assert!(!rt.state.wapps[2].enabled);
    assert!(visible(&rt, "extension-card-test.theme-theme-hint").is_none(),
        "read-only list does not direct the user to activate it in Settings");
    press(&mut rt, "extension-filter-all");
    until(&mut rt, "extension-card-test.workspace");
    assert_eq!(rt.elements().iter().filter(|node| !node.exiting
        && node.key.as_deref() == Some("extension-card-test.dual")).count(), 1);

    press(&mut rt, "extension-install-button");
    until(&mut rt, "install-wapp-dialog");
    press(&mut rt, "extension-install-cancel");
    assert!(!rt.state.install_wapp_open, "existing local dialog can still be cancelled");

    let corrupt = fixture(&root, "test.corrupt", "Unknown Version", true, false, false);
    let mut payload: serde_json::Value =
        serde_json::from_slice(&fs::read(corrupt.join("manifest.json")).unwrap()).unwrap();
    payload["apiVersion"] = json!(99);
    fs::write(corrupt.join("manifest.json"), serde_json::to_vec(&payload).unwrap()).unwrap();
    press(&mut rt, "rail-connections");
    press(&mut rt, "rail-wapps");
    until(&mut rt, "extension-page-error");
    assert_eq!(
        visible(&rt, "extension-page-error").unwrap().text.as_deref(),
        Some(t("extensions.page.nativeListUnreadable"))
    );
    assert!(visible(&rt, "extension-card-test.workspace").is_some());
    assert!(visible(&rt, "extension-card-test.corrupt").is_none());
    assert_eq!(rt.state.wapps.len(), 3, "failed refresh did not replace good rows");
    assert_eq!(fs::read(enabled.join("manifest.json")).unwrap(), manifest);
    assert!(enabled.join(".enabled").is_file());
    assert!(!disabled.join(".enabled").exists());
    assert_eq!(fs::read_to_string(disabled.join("tokens.css")).unwrap(), ":root { --accent: #f0a; }");

    // The same bad profile on a fresh mount has no verified count/list.
    let mut fresh = runtime(&root);
    press(&mut fresh, "rail-wapps");
    until(&mut fresh, "extension-page-error");
    assert!(visible(&fresh, "extension-page-empty").is_none());
    assert!(visible(&fresh, "extension-page-count").is_none());
}

#[test]
fn linked_invalid_or_unbounded_registry_entries_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let data = root.join("profile");
    fs::create_dir(&data).unwrap();
    let foreign = fixture(&root.join("foreign"), "test.outside", "Outside", true, false, true);
    let mut adapter = host(&data);
    let wapps = data.join("wapps");

    fs::write(&wapps, b"not a directory").unwrap();
    assert_unreadable(&mut adapter);
    fs::remove_file(&wapps).unwrap();
    symlink(root.join("foreign/wapps"), &wapps).unwrap();
    assert_unreadable(&mut adapter);
    fs::remove_file(&wapps).unwrap();
    fs::create_dir(&wapps).unwrap();
    let package = wapps.join("test.package");
    symlink(&foreign, &package).unwrap();
    assert_unreadable(&mut adapter);
    fs::remove_file(&package).unwrap();

    let package = fixture(&data, "test.package", "Bounded", true, false, false);
    let marker = package.join(".enabled");
    symlink(foreign.join(".enabled"), &marker).unwrap();
    assert_unreadable(&mut adapter);
    fs::remove_file(&marker).unwrap();
    let manifest = package.join("manifest.json");
    fs::remove_file(&manifest).unwrap();
    symlink(foreign.join("manifest.json"), &manifest).unwrap();
    assert_unreadable(&mut adapter);
    fs::remove_file(&manifest).unwrap();
    fs::hard_link(foreign.join("manifest.json"), &manifest).unwrap();
    assert_unreadable(&mut adapter);
    fs::remove_file(&manifest).unwrap();
    fs::write(&manifest, vec![b' '; 256 * 1024 + 1]).unwrap();
    assert_unreadable(&mut adapter);
    fs::remove_dir_all(&package).unwrap();
    let malformed = wapps.join("invalid-package-id");
    fs::create_dir(&malformed).unwrap();
    fs::write(malformed.join("manifest.json"), b"{}").unwrap();
    assert_unreadable(&mut adapter);
}
