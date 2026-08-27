//! F1「Rust 插件基座」集成测试（测试 agent 补充）。
//!
//! 运行：`cargo test -p datazen --lib extensions::integration_tests`
//! 说明：lib.rs 中 `mod extensions` 为 crate 私有，外部 tests/ 目标无法访问，
//! 故本文件经 `#[cfg(test)] mod integration_tests;` 以 lib 单测目标编译
//! （接线见 plugins/mod.rs，零发布代码影响）。

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::storage::MAX_STORAGE_BYTES;
use super::{
    is_valid_extension_id, parse_manifest, storage_get, storage_remove, validate_manifest,
    validate_extension_dir, ExtensionManager,
};
use crate::commands::{
    get_extension_manifest_impl, install_extension_from_path_impl, list_extensions_impl,
    extension_storage_get_impl, extension_storage_remove_impl, extension_storage_set_impl,
    read_extension_file_impl, remove_extension_impl, set_extension_enabled_impl,
};
use crate::testing::app_state::TestAppState;

const PAGE_MANIFEST_ID: &str = "acme.bill-audit";

const PAGE_MANIFEST: &str = r#"{
  "id": "acme.bill-audit",
  "name": "Bill Audit",
  "version": "1.0.0",
  "apiVersion": 2,
  "entry": "index.html",
  "contributes": {
    "pages": [{ "id": "quota-check", "title": "Quota Check", "icon": "assets/icon.svg" }]
  },
  "permissions": ["context:connections", "storage:local"]
}"#;

const THEME_MANIFEST: &str = r#"{
  "id": "acme.one",
  "name": "Theme One",
  "version": "1.2.0",
  "apiVersion": 2,
  "contributes": {
    "themes": [{ "id": "one-dark", "name": "One Dark", "tokensCss": "tokens.css", "modes": ["dark"] }]
  }
}"#;

fn page_plugin_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("manifest.json", PAGE_MANIFEST),
        ("index.html", "<html>bill-audit</html>"),
        (
            "assets/icon.svg",
            "<svg xmlns='http://www.w3.org/2000/svg'/>",
        ),
    ]
}

fn theme_plugin_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("manifest.json", THEME_MANIFEST),
        ("tokens.css", ":root { --c-accent: red; }"),
    ]
}

fn write_dir(dir: &Path, entries: &[(&str, &str)]) {
    fs::create_dir_all(dir).unwrap();
    for (name, content) in entries {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

fn zip_from_entries(path: &Path, entries: &[(&str, &str)]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

/// ZIP whose single entry claims ~51 MiB uncompressed (over the 50 MB quota).
fn oversize_declared_zip(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("blob.html", options).unwrap();
    let chunk = vec![b'0'; 1024 * 1024];
    for _ in 0..51 {
        zip.write_all(&chunk).unwrap();
    }
    zip.finish().unwrap();
}

fn plugins_root(test: &TestAppState) -> PathBuf {
    test.state.extensions.extensions_dir().to_path_buf()
}

fn staging_dirs(plugins_dir: &Path) -> Vec<String> {
    fs::read_dir(plugins_dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.starts_with(".staging-") || n.ends_with(".old.bak"))
        .collect()
}

async fn install_error(test: &TestAppState, path: &Path) -> String {
    install_extension_from_path_impl(&test.state, path.to_string_lossy().to_string())
        .await
        .expect_err("install must fail")
        .to_string()
}

fn contains_any(haystack: &str, needles: &str) -> bool {
    needles.split('|').any(|n| haystack.contains(n))
}

/// Assert a patched manifest fails at parse or validate level with `expect`,
/// and also fails end-to-end through the install path leaving no residue.
async fn assert_variant_rejected(
    test: &TestAppState,
    tmp: &TempDir,
    idx: usize,
    label: &str,
    patched: &str,
    expect: &str,
) {
    match parse_manifest(patched) {
        Err(err) => assert!(err.contains(expect), "[{label}] got: {err}"),
        Ok(manifest) => {
            let dir = tmp.path().join(format!("check-{idx}"));
            write_dir(&dir, &page_plugin_entries());
            let err = validate_manifest(&manifest, &dir)
                .expect_err(&format!("[{label}] must fail validation"));
            assert!(err.contains(expect), "[{label}] got: {err}");
            assert!(validate_extension_dir(&dir.join("..").join("nonexistent")).is_err());
        }
    }

    let mut entries = page_plugin_entries();
    entries[0] = ("manifest.json", patched);
    let src = tmp.path().join(format!("pkg-{idx}"));
    write_dir(&src, &entries);
    let err = install_error(test, &src).await;
    assert!(
        contains_any(&err.to_lowercase(), &expect.to_lowercase()),
        "[{label}] expected `{expect}` in install error: {err}"
    );
    assert!(list_extensions_impl(&test.state).is_empty(), "[{label}]");
    assert!(staging_dirs(&plugins_root(test)).is_empty(), "[{label}]");
}

// ---------------------------------------------------------------------------
// F1-I01 全流程：安装 → list → 停用 → list → 启用 → 移除（含持久化与幂等）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_lifecycle_install_list_toggle_remove() {
    let test = TestAppState::new().await;
    assert!(list_extensions_impl(&test.state).is_empty());

    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("bill-audit.zip");
    zip_from_entries(&zip_path, &page_plugin_entries());

    // -- 安装：返回摘要且 enabled=true
    let summary =
        install_extension_from_path_impl(&test.state, zip_path.to_string_lossy().to_string())
            .await
            .unwrap();
    assert_eq!(summary.id, PAGE_MANIFEST_ID);
    assert!(summary.enabled);
    assert_eq!(
        summary.permissions,
        vec![
            "context:connections".to_string(),
            "storage:local".to_string()
        ]
    );
    assert_eq!(summary.pages.len(), 1);
    assert_eq!(summary.pages[0].id, "quota-check");

    // -- list：1 条、enabled=true、`.enabled` 落盘且目录可复验
    let plugins = list_extensions_impl(&test.state);
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].id, PAGE_MANIFEST_ID);
    assert!(plugins[0].enabled);
    let dir = plugins_root(&test).join(PAGE_MANIFEST_ID);
    assert!(dir.join(".enabled").is_file());
    assert!(
        validate_extension_dir(&dir).is_ok(),
        "installed dir revalidates"
    );

    // -- manifest 查询
    let manifest = get_extension_manifest_impl(&test.state, PAGE_MANIFEST_ID).unwrap();
    assert_eq!(manifest.version, "1.0.0");

    // -- storage 写入（供启停切换后验证保留）
    extension_storage_set_impl(
        &test.state,
        PAGE_MANIFEST_ID.into(),
        "lastUid".into(),
        json!(42),
    )
    .await
    .unwrap();

    // -- set_enabled(false)：仍列出但 disabled；marker 消失；读取被拒
    set_extension_enabled_impl(&test.state, PAGE_MANIFEST_ID.into(), false)
        .await
        .unwrap();
    let plugins = list_extensions_impl(&test.state);
    assert_eq!(plugins.len(), 1, "disabled plugin stays listed");
    assert!(!plugins[0].enabled);
    assert!(!dir.join(".enabled").exists());
    let err = read_extension_file_impl(&test.state, PAGE_MANIFEST_ID.into(), "index.html".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("disabled"), "{err}");

    // -- 重启模拟：新 manager 从磁盘恢复 disabled 状态
    let reloaded = ExtensionManager::new(plugins_root(&test));
    assert_eq!(reloaded.load_from_disk(), 1);
    assert!(!reloaded.get(PAGE_MANIFEST_ID).unwrap().enabled);

    // -- set_enabled(true)：恢复启用；storage 数据不受启停影响
    set_extension_enabled_impl(&test.state, PAGE_MANIFEST_ID.into(), true)
        .await
        .unwrap();
    assert!(list_extensions_impl(&test.state)[0].enabled);
    assert_eq!(
        extension_storage_get_impl(&test.state, PAGE_MANIFEST_ID.into(), "lastUid".into())
            .await
            .unwrap(),
        Some(json!(42))
    );

    // -- remove：目录（含 `.storage.json`）删除并注销；再次 remove 报 NotFound（幂等语义清晰）
    remove_extension_impl(&test.state, PAGE_MANIFEST_ID.into())
        .await
        .unwrap();
    assert!(list_extensions_impl(&test.state).is_empty());
    assert!(!plugins_root(&test).join(PAGE_MANIFEST_ID).exists());

    let err = remove_extension_impl(&test.state, PAGE_MANIFEST_ID.into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
    assert!(staging_dirs(&plugins_root(&test)).is_empty());
}

// ---------------------------------------------------------------------------
// F1-I02 恶意 zip：穿越 / 绝对路径 / 超 50MB 声明 / 非法扩展名 —— 全部拒绝无残留
// ---------------------------------------------------------------------------

#[tokio::test]
async fn malicious_zips_rejected_without_side_effects() {
    let test = TestAppState::new().await;
    let tmp = TempDir::new().unwrap();

    let cases: Vec<(&str, Vec<(&str, &str)>, &str)> = vec![
        (
            "traversal-dotdot",
            vec![("../outside.html", "<html>evil</html>")],
            "traversal|invalid zip entry|unsafe",
        ),
        (
            "absolute-unix-path",
            vec![("/etc/datazen-evil.html", "<html>evil</html>")],
            "invalid zip entry|unsafe|absolute|traversal",
        ),
        (
            "windows-drive-path",
            vec![("C:/Windows/datazen-evil.html", "<html>evil</html>")],
            "invalid zip entry|unsafe|absolute|traversal|windows",
        ),
        (
            "nested-escape",
            vec![("assets/../../outside.html", "<html>evil</html>")],
            "traversal|invalid zip entry|unsafe",
        ),
        (
            "forbidden-extension-sh",
            vec![
                ("manifest.json", PAGE_MANIFEST),
                ("index.html", "<html></html>"),
                ("run.sh", "#!/bin/sh"),
            ],
            "forbidden extension .sh",
        ),
    ];

    for (name, entries, expect) in &cases {
        let zip_path = tmp.path().join(format!("{name}.zip"));
        zip_from_entries(&zip_path, entries);

        let err = install_error(&test, &zip_path).await;
        assert!(
            contains_any(&err, expect),
            "[{name}] expected `{expect}`, got: {err}"
        );
        assert!(
            list_extensions_impl(&test.state).is_empty(),
            "[{name}] registry stays empty"
        );
        assert!(
            staging_dirs(&plugins_root(&test)).is_empty(),
            "[{name}] no staging/backup leftovers"
        );
    }

    // 超 50MB 声明大小（首遍基于 central directory 尺寸即拒，不解压）。
    let big = tmp.path().join("oversize.zip");
    oversize_declared_zip(&big);
    let err = install_error(&test, &big).await;
    assert!(
        contains_any(&err, "uncompressed size limit|size limit|zip bomb"),
        "oversize declared package: {err}"
    );
    assert!(list_extensions_impl(&test.state).is_empty());
    assert!(staging_dirs(&plugins_root(&test)).is_empty());
}

// ---------------------------------------------------------------------------
// F1-I03 manifest 边界：id 点号 / api_version 1·3 / backend={} / 未知权限 /
// deny_unknown_fields 多余字段 / 图标缺失
// ---------------------------------------------------------------------------

#[tokio::test]
async fn manifest_boundary_rules_enforced_on_install() {
    let test = TestAppState::new().await;
    let tmp = TempDir::new().unwrap();

    let variants: Vec<(&str, String, &str)> = vec![
        (
            "missing publisher dot",
            PAGE_MANIFEST.replace("\"id\": \"acme.bill-audit\"", "\"id\": \"bill-audit\""),
            "invalid extension id",
        ),
        (
            "extra dot in id",
            PAGE_MANIFEST.replace("\"id\": \"acme.bill-audit\"", "\"id\": \"ac.me.bill\""),
            "invalid extension id",
        ),
        (
            "api_version=1 too old",
            PAGE_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 1"),
            "apiVersion",
        ),
        (
            "api_version=3 too new",
            PAGE_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 3"),
            "apiVersion",
        ),
        (
            "backend empty object",
            PAGE_MANIFEST.replace(
                "\"entry\": \"index.html\",",
                "\"entry\": \"index.html\", \"backend\": {},",
            ),
            "backend",
        ),
        (
            "unknown permission string",
            PAGE_MANIFEST.replace(
                "\"permissions\": [\"context:connections\", \"storage:local\"]",
                "\"permissions\": [\"context:connections\", \"fs:write-all\"]",
            ),
            "unknown variant",
        ),
        (
            "unknown top-level field",
            PAGE_MANIFEST.replace(
                "\"entry\": \"index.html\",",
                "\"entry\": \"index.html\", \"autoEnable\": true,",
            ),
            "autoEnable",
        ),
        (
            "unknown field inside page object",
            PAGE_MANIFEST.replace(
                "\"icon\": \"assets/icon.svg\" }",
                "\"icon\": \"assets/icon.svg\", \"shell\": true }",
            ),
            "unknown field",
        ),
    ];

    for (idx, (label, patched, expect)) in variants.into_iter().enumerate() {
        assert_variant_rejected(&test, &tmp, idx, label, &patched, expect).await;
    }

    // 规则 2 文案：必须含 DataZen 版本指引（「需要更新版本的 DataZen >= x.y.z」）。
    let old =
        parse_manifest(&PAGE_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 1")).unwrap();
    let dir = tmp.path().join(PAGE_MANIFEST_ID);
    write_dir(&dir, &page_plugin_entries());
    let err = validate_manifest(&old, &dir).unwrap_err();
    assert!(
        err.contains("DataZen") && err.contains("apiVersion"),
        "{err}"
    );

    // 页面图标声明了但文件缺失 → 明确报错。
    let mut m = parse_manifest(PAGE_MANIFEST).unwrap();
    m.contributes.pages[0].icon = Some("assets/missing.svg".into());
    let err = validate_manifest(&m, &dir).unwrap_err();
    assert!(err.contains("page icon not found"), "{err}");

    // id 字符集边界补充（纯函数级）。
    assert!(!is_valid_extension_id("-lead.bill"));
    assert!(!is_valid_extension_id("acme."));
    assert!(!is_valid_extension_id(".bill"));
    assert!(is_valid_extension_id("a0.b-c"));
}

// ---------------------------------------------------------------------------
// F1-I04 storage：跨插件隔离 + 1MB 限额（impl 层，双真实插件）
// ---------------------------------------------------------------------------

#[tokio::test]
async fn storage_isolated_across_plugins_and_capped_at_1mb() {
    let test = TestAppState::new().await;
    let tmp = TempDir::new().unwrap();

    for (file, entries) in [
        ("one.zip", theme_plugin_entries()),
        ("two.zip", page_plugin_entries()),
    ] {
        let zip_path = tmp.path().join(file);
        zip_from_entries(&zip_path, &entries);
        install_extension_from_path_impl(&test.state, zip_path.to_string_lossy().to_string())
            .await
            .unwrap();
    }
    assert_eq!(list_extensions_impl(&test.state).len(), 2);

    // 同 key 双插件互不干扰。
    extension_storage_set_impl(
        &test.state,
        "acme.one".into(),
        "shared-key".into(),
        json!("from-one"),
    )
    .await
    .unwrap();
    extension_storage_set_impl(
        &test.state,
        PAGE_MANIFEST_ID.into(),
        "shared-key".into(),
        json!(7),
    )
    .await
    .unwrap();
    assert_eq!(
        extension_storage_get_impl(&test.state, "acme.one".into(), "shared-key".into())
            .await
            .unwrap(),
        Some(json!("from-one"))
    );
    assert_eq!(
        extension_storage_get_impl(&test.state, PAGE_MANIFEST_ID.into(), "shared-key".into())
            .await
            .unwrap(),
        Some(json!(7))
    );

    // 未注册插件在 IPC 层被拒（先于磁盘访问）。
    let err = extension_storage_set_impl(&test.state, "acme.ghost".into(), "k".into(), json!(1))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");

    // 1MB 限额：略小于上限可写入；追加后超限拒绝且原数据不变。
    let big_value = json!("x".repeat(MAX_STORAGE_BYTES - 512));
    extension_storage_set_impl(
        &test.state,
        PAGE_MANIFEST_ID.into(),
        "blob".into(),
        big_value.clone(),
    )
    .await
    .unwrap();
    assert_eq!(
        extension_storage_get_impl(&test.state, PAGE_MANIFEST_ID.into(), "blob".into())
            .await
            .unwrap(),
        Some(big_value)
    );

    let err = extension_storage_set_impl(
        &test.state,
        PAGE_MANIFEST_ID.into(),
        "overflow".into(),
        json!("y".repeat(2048)),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("exceeds limit"), "{err}");
    assert_eq!(
        extension_storage_get_impl(&test.state, PAGE_MANIFEST_ID.into(), "shared-key".into())
            .await
            .unwrap(),
        Some(json!(7)),
        "failed oversized write must not clobber existing store"
    );
}

// ---------------------------------------------------------------------------
// F1-I05 storage remove 幂等：重复删除返回 Ok，键存在性由底层布尔值区分
// ---------------------------------------------------------------------------

#[tokio::test]
async fn storage_remove_is_idempotent() {
    let test = TestAppState::new().await;
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("bill-audit.zip");
    zip_from_entries(&zip_path, &page_plugin_entries());
    install_extension_from_path_impl(&test.state, zip_path.to_string_lossy().to_string())
        .await
        .unwrap();

    extension_storage_set_impl(&test.state, PAGE_MANIFEST_ID.into(), "k".into(), json!("v"))
        .await
        .unwrap();

    // 底层：第一次删除返回 true（存在），第二次返回 false（幂等不报错）。
    assert!(storage_remove(&plugins_root(&test), PAGE_MANIFEST_ID, "k").unwrap());
    assert!(!storage_remove(&plugins_root(&test), PAGE_MANIFEST_ID, "k").unwrap());
    assert_eq!(
        storage_get(&plugins_root(&test), PAGE_MANIFEST_ID, "k").unwrap(),
        None
    );

    // IPC 层：两次 remove 均 Ok(()).
    extension_storage_remove_impl(&test.state, PAGE_MANIFEST_ID.into(), "k".into())
        .await
        .unwrap();
    extension_storage_remove_impl(&test.state, PAGE_MANIFEST_ID.into(), "k".into())
        .await
        .unwrap();
    assert!(!plugins_root(&test)
        .join(PAGE_MANIFEST_ID)
        .join(".storage.json")
        .exists());
}

// ---------------------------------------------------------------------------
// F1-I06 read_plugin_file：正常 / 隐藏文件拒绝 / 穿越 / 绝对路径 / 未启用拒绝
// ---------------------------------------------------------------------------

#[tokio::test]
async fn read_plugin_file_enforces_sandbox_rules() {
    let test = TestAppState::new().await;
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("bill-audit.zip");
    zip_from_entries(&zip_path, &page_plugin_entries());
    install_extension_from_path_impl(&test.state, zip_path.to_string_lossy().to_string())
        .await
        .unwrap();

    // 正常读取：根文件与嵌套资产。
    assert_eq!(
        read_extension_file_impl(&test.state, PAGE_MANIFEST_ID.into(), "index.html".into())
            .await
            .unwrap(),
        b"<html>bill-audit</html>".to_vec()
    );
    read_extension_file_impl(
        &test.state,
        PAGE_MANIFEST_ID.into(),
        "assets/icon.svg".into(),
    )
    .await
    .unwrap();

    // 宿主托管隐藏文件一律拒绝。
    for hidden in [".storage.json", ".enabled", "assets/.secret.css"] {
        let err = read_extension_file_impl(&test.state, PAGE_MANIFEST_ID.into(), hidden.into())
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("hidden") || err.to_string().contains("unsafe"),
            "`{hidden}`: {err}"
        );
    }

    // 穿越 / 绝对路径 / 反斜杠穿越。
    for evil in [
        "../settings.json",
        "assets/../../x.html",
        "/etc/passwd",
        "..\\evil.html",
    ] {
        let err = read_extension_file_impl(&test.state, PAGE_MANIFEST_ID.into(), evil.into())
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("unsafe"),
            "`{evil}` must be rejected: {err}"
        );
    }

    // 缺失文件 → NotFound；未知插件 → NotFound。
    let err = read_extension_file_impl(&test.state, PAGE_MANIFEST_ID.into(), "nope.html".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
    let err = read_extension_file_impl(&test.state, "acme.ghost".into(), "index.html".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");

    // 未启用插件读取被拒。
    set_extension_enabled_impl(&test.state, PAGE_MANIFEST_ID.into(), false)
        .await
        .unwrap();
    let err = read_extension_file_impl(&test.state, PAGE_MANIFEST_ID.into(), "index.html".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("disabled"), "{err}");
}
