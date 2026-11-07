use super::command_test_host::host_command_app;
use super::*;

const BOOTSTRAP_RS: &str = include_str!("../../bootstrap/run.rs");

#[test]
fn merged_wapp_commands_gate_override_path_in_production() {
    for gone in ["commands::list_extensions,"] {
        assert!(
            !BOOTSTRAP_RS.contains(gone),
            "`{gone}` must no longer be registered"
        );
    }
    assert!(BOOTSTRAP_RS.contains("commands::list_wapps,"));
    assert!(BOOTSTRAP_RS.contains("commands::install_wapp,"));
    assert!(BOOTSTRAP_RS.contains("commands::remove_wapp,"));
}

use crate::testing::app_state_test_support::TestAppState;

use serde_json::json;

use std::fs;

use std::io::Write as _;

use std::path::Path;

use tempfile::TempDir;

use zip::write::SimpleFileOptions;

use zip::{CompressionMethod, ZipWriter};

const DEMO_MANIFEST: &str = r#"{
      "id": "acme.demo",
      "name": "Demo Wapp",
      "version": "1.0.0",
      "apiVersion": 2,
      "author": "Acme",
      "entry": "index.html",
      "contributes": {
        "pages": [{ "id": "main", "title": "Main", "icon": "assets/icon.svg" }],
        "themes": [{
          "id": "demo-dark",
          "name": "Demo Dark",
          "tokensCss": "themes/demo-dark/tokens.css",
          "modes": ["dark"]
        }]
      },
      "permissions": ["storage:local", "command:invoke"]
    }"#;

fn write_demo_zip(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in [
        ("manifest.json", DEMO_MANIFEST),
        ("index.html", "<html>demo</html>"),
        (
            "assets/icon.svg",
            "<svg xmlns='http://www.w3.org/2000/svg'/>",
        ),
        ("themes/demo-dark/tokens.css", ":root { --c-accent: red; }"),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn write_demo_dir(dir: &Path) {
    for (name, content) in [
        ("manifest.json", DEMO_MANIFEST),
        ("index.html", "<html>demo</html>"),
        (
            "assets/icon.svg",
            "<svg xmlns='http://www.w3.org/2000/svg'/>",
        ),
        ("themes/demo-dark/tokens.css", ":root { --c-accent: red; }"),
    ] {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

async fn install_zip(test: &TestAppState, path: &Path) -> WappSummary {
    install_wapp_from_path_impl(&test.state, path.to_string_lossy().to_string())
        .await
        .unwrap()
}

#[tokio::test]
async fn wapp_driver_command_uses_an_independent_runtime_target() {
    let test = TestAppState::with_tables_and_resource_provider().await;
    let zip_path = test._temp.path().join("wapp-runtime.zip");
    write_demo_zip(&zip_path);
    install_zip(&test, &zip_path).await;
    test.save_connection("wapp-runtime").await;

    // B2 域刀：`execute_wapp_driver_command_impl` 转发层已删，本测试改走
    // 真实命令入口（mock_app 托管 AppState）。准入规则（权限/黑名单/args
    // 形状）随接线住在命令处理函数里；读取用例本体在
    // `datazen::workflow::runtime_reads`。
    let app = host_command_app(test);
    let result = execute_wapp_driver_command(
        app.state(),
        WappDriverCommandRequest {
            wapp_id: "acme.demo".into(),
            connection_id: "wapp-runtime".into(),
            command: "query".into(),
            args: json!({"sql": "SELECT * FROM users", "database": "app"}),
        },
    )
    .await
    .expect("a permitted read command must execute through Runtime");

    assert!(result.is_object());
    let state = app.state();
    let adapter = state.platform_require().expect("platform is ready");
    let call = adapter.begin_call();
    assert!(
        adapter
            .runtime()
            .expect("runtime is ready")
            .get_session_by_id(call.context(), "wapp-runtime")
            .await
            .is_err(),
        "Wapp invocation must not create a persistent editor session"
    );
}

#[tokio::test]
async fn disabled_wapp_cannot_invoke_driver_commands() {
    let test = TestAppState::new().await;
    let zip_path = test._temp.path().join("wapp-disabled.zip");
    write_demo_zip(&zip_path);
    install_zip(&test, &zip_path).await;
    set_wapp_enabled_impl(&test.state, "acme.demo".into(), false)
        .await
        .unwrap();

    // B2 域刀：同上，改走真实命令入口；拒绝文案是 IPC 契约。
    let app = host_command_app(test);
    let error = execute_wapp_driver_command(
        app.state(),
        WappDriverCommandRequest {
            wapp_id: "acme.demo".into(),
            connection_id: "not-used".into(),
            command: "query".into(),
            args: json!({"sql": "SELECT 1"}),
        },
    )
    .await
    .expect_err("disabled Wapp must be denied before runtime access");
    assert!(error.to_string().contains("disabled"));
}

#[tokio::test]
async fn install_list_enable_disable_remove_flow() {
    let test = TestAppState::new().await;
    assert!(list_wapps_impl(&test.state).is_empty());

    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("demo.zip");
    write_demo_zip(&zip_path);

    // -- install (zip)
    let summary = install_zip(&test, &zip_path).await;
    assert_eq!(summary.id, "acme.demo");
    assert!(summary.enabled);
    assert_eq!(
        summary.permissions,
        vec!["storage:local".to_string(), "command:invoke".to_string()]
    );
    assert_eq!(summary.pages.len(), 1);
    assert_eq!(summary.pages[0].id, "main");
    assert_eq!(summary.themes.len(), 1);
    assert_eq!(summary.themes[0].id, "demo-dark");

    // -- listed with enabled=true
    let wapps = list_wapps_impl(&test.state);
    assert_eq!(wapps.len(), 1);
    assert_eq!(wapps[0].id, "acme.demo");
    assert!(wapps[0].enabled);

    // -- marker file written
    assert!(test
        .state
        .store
        .data_dir()
        .join("wapps/acme.demo/.enabled")
        .is_file());

    // -- manifest lookup
    let manifest = get_wapp_manifest_impl(&test.state, "acme.demo").unwrap();
    assert_eq!(manifest.version, "1.0.0");

    // -- disable: still listed, enabled=false, marker removed
    set_wapp_enabled_impl(&test.state, "acme.demo".into(), false)
        .await
        .unwrap();
    let wapps = list_wapps_impl(&test.state);
    assert_eq!(wapps.len(), 1);
    assert!(!wapps[0].enabled);
    assert!(!test
        .state
        .store
        .data_dir()
        .join("wapps/acme.demo/.enabled")
        .exists());

    // -- reads are refused while disabled
    let err = read_wapp_file_impl(&test.state, "acme.demo".into(), "index.html".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("disabled"));

    // -- enable again
    set_wapp_enabled_impl(&test.state, "acme.demo".into(), true)
        .await
        .unwrap();
    assert!(list_wapps_impl(&test.state)[0].enabled);

    // -- remove deletes the directory and unregisters
    remove_wapp_impl(&test.state, "acme.demo".into())
        .await
        .unwrap();
    assert!(list_wapps_impl(&test.state).is_empty());
    assert!(!test.state.store.data_dir().join("wapps/acme.demo").exists());

    // -- unknown ids error cleanly
    assert!(remove_wapp_impl(&test.state, "acme.demo".into())
        .await
        .is_err());
    assert!(get_wapp_manifest_impl(&test.state, "acme.demo").is_err());
}

#[tokio::test]
async fn install_from_directory_and_reinstall() {
    let test = TestAppState::new().await;
    let src = TempDir::new().unwrap();
    write_demo_dir(src.path());

    let summary =
        install_wapp_from_path_impl(&test.state, src.path().to_string_lossy().to_string())
            .await
            .unwrap();
    assert_eq!(summary.id, "acme.demo");

    // Reinstall over the same id keeps exactly one entry.
    install_wapp_from_path_impl(&test.state, src.path().to_string_lossy().to_string())
        .await
        .unwrap();
    assert_eq!(list_wapps_impl(&test.state).len(), 1);
}

#[tokio::test]
async fn install_rejects_invalid_packages() {
    let test = TestAppState::new().await;

    // Missing path.
    let err = install_wapp_from_path_impl(&test.state, "/nonexistent/pkg.zip".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"));

    // Zip whose manifest fails validation (apiVersion mismatch).
    let tmp = TempDir::new().unwrap();
    let bad = tmp.path().join("bad.zip");
    {
        let file = fs::File::create(&bad).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = DEMO_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 99");
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(manifest.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    let err = install_wapp_from_path_impl(&test.state, bad.to_string_lossy().to_string())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("apiVersion"), "{err}");
    assert!(list_wapps_impl(&test.state).is_empty());
}

// B2 域刀：`inspect_wapp_package_previews_manifest_without_writing` 已删除——
// 包校验本体（含「不落盘安装」与 apiVersion 拒绝）在
// `crates/datazen/src/wapps/install/tests.rs` 的 inspect_* 三测有逐面覆盖；宿主侧
// 剩下的 NotFound 预检留在 `inspect_wapp_package_with_dialog` 命令里（该
// 命令需要 Wry 对话框句柄 + webdriver 路径门，不进单测）。

#[tokio::test]
async fn storage_requires_existing_wapp_and_namespaces_by_id() {
    let test = TestAppState::new().await;

    // Unknown wapp id is rejected before touching the disk.
    assert!(
        wapp_storage_get_impl(&test.state, "acme.ghost".into(), "k".into())
            .await
            .is_err()
    );

    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("demo.zip");
    write_demo_zip(&zip_path);
    install_zip(&test, &zip_path).await;

    wapp_storage_set_impl(
        &test.state,
        "acme.demo".into(),
        "lastUid".into(),
        json!(58043285),
    )
    .await
    .unwrap();

    let value = wapp_storage_get_impl(&test.state, "acme.demo".into(), "lastUid".into())
        .await
        .unwrap();
    assert_eq!(value, Some(json!(58043285)));

    wapp_storage_remove_impl(&test.state, "acme.demo".into(), "lastUid".into())
        .await
        .unwrap();
    let value = wapp_storage_get_impl(&test.state, "acme.demo".into(), "lastUid".into())
        .await
        .unwrap();
    assert_eq!(value, None);
}

#[tokio::test]
async fn read_wapp_file_enforces_sandbox_rules() {
    let test = TestAppState::new().await;

    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("demo.zip");
    write_demo_zip(&zip_path);
    install_zip(&test, &zip_path).await;

    // Normal read.
    let html = read_wapp_file_impl(&test.state, "acme.demo".into(), "index.html".into())
        .await
        .unwrap();
    assert_eq!(html, b"<html>demo</html>");

    // Nested read inside assets/.
    read_wapp_file_impl(&test.state, "acme.demo".into(), "assets/icon.svg".into())
        .await
        .unwrap();

    // Host-managed hidden files are refused.
    for hidden in [".storage.json", ".enabled"] {
        let err = read_wapp_file_impl(&test.state, "acme.demo".into(), hidden.into())
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("hidden") || err.to_string().contains("unsafe"),
            "`{hidden}`: {err}"
        );
    }

    // Traversal is refused.
    let err = read_wapp_file_impl(&test.state, "acme.demo".into(), "../settings.json".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unsafe"), "{err}");

    // Missing files are NotFound.
    let err = read_wapp_file_impl(&test.state, "acme.demo".into(), "nope.html".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");

    // Unknown wapps are NotFound before any path handling.
    let err = read_wapp_file_impl(&test.state, "acme.ghost".into(), "index.html".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
}

#[tokio::test]
async fn load_from_disk_restores_registry_with_persisted_enabled_state() {
    let test = TestAppState::new().await;
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("demo.zip");
    write_demo_zip(&zip_path);
    install_zip(&test, &zip_path).await;
    set_wapp_enabled_impl(&test.state, "acme.demo".into(), false)
        .await
        .unwrap();

    // Simulate a restart: fresh manager over the same app-data dir.
    let reloaded = datazen::workspace_apps::manager::WappManager::new(
        test.state.store.data_dir().join("wapps"),
    );
    reloaded.load_from_disk();
    let restored = reloaded.get("acme.demo").expect("wapp restored");
    assert!(!restored.enabled);
    assert_eq!(restored.manifest.name, "Demo Wapp");
}

// B2 域刀：`summary_serializes_camel_case_payload` 与
// `wapp_package_kind_parses_zip_and_folder_aliases` 两个纯逻辑测试随投影/
// 词汇解析本体迁 `crates/datazen/src/wapps/{summary,package_kind}.rs`（内联）。

#[tokio::test]
async fn pick_session_is_consumed_once_on_install_path_resolution() {
    let tmp = TempDir::new().unwrap();
    let token = insert_pick_session(tmp.path().to_path_buf()).await.unwrap();
    let resolved = take_pick_session(&token).await.unwrap();
    assert_eq!(resolved, tmp.path());
    assert!(take_pick_session(&token).await.is_err());
}
