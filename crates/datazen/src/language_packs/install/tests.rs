//! `install.rs` 的测试：合法包安装全链、校验矩阵逐条钉、原子性与零残留。

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::{
    check_compression_ratio, inspect_language_pack_file, install_from_zip, ratio_exceeds_limit,
};
use crate::language_packs::manifest::{
    validate_locale_code, validate_message_key, LanguagePackManifest,
};

const DEMO_MANIFEST: &str = r#"{
  "packVersion": 1,
  "code": "ja",
  "label": "日本語",
  "version": "1.0.0",
  "author": "DataZen",
  "description": "Japanese UI translation"
}"#;

fn add_file(zip: &mut ZipWriter<File>, name: &str, content: &str, options: SimpleFileOptions) {
    zip.start_file(name, options).unwrap();
    zip.write_all(content.as_bytes()).unwrap();
}

fn manifest_json(code: &str, label: &str) -> String {
    format!(r#"{{"packVersion":1,"code":"{code}","label":"{label}","version":"1.0.0"}}"#)
}

fn domain_json(entries: &[(&str, &str)]) -> String {
    let map: BTreeMap<&str, &str> = entries.iter().copied().collect();
    serde_json::to_string_pretty(&map).unwrap()
}

/// 写一个合法演示包（manifest + settings 域两条 + core 域一条）。
fn write_demo_zip(zip_path: &Path) {
    let file = File::create(zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_file(&mut zip, "manifest.json", DEMO_MANIFEST, options);
    add_file(
        &mut zip,
        "settings.json",
        &domain_json(&[("settings.language", "言語"), ("settings.general", "全般")]),
        options,
    );
    add_file(
        &mut zip,
        "core.json",
        &domain_json(&[("common.ok", "了解")]),
        options,
    );
    zip.finish().unwrap();
}

/// 写一个自定义包：给定 manifest 文本与 (文件名, 内容) 列表。
fn write_zip(zip_path: &Path, manifest: &str, files: &[(&str, String)]) {
    let file = File::create(zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_file(&mut zip, "manifest.json", manifest, options);
    for (name, content) in files {
        add_file(&mut zip, name, content, options);
    }
    zip.finish().unwrap();
}

fn staging_dirs(packs_dir: &Path) -> Vec<String> {
    fs::read_dir(packs_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(".staging-"))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn install_valid_pack_writes_domain_files_and_manifest() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_demo_zip(&zip_path);

    let manifest = install_from_zip(&zip_path, &packs_dir).unwrap();
    assert_eq!(manifest.manifest.code, "ja");
    assert_eq!(manifest.manifest.label, "日本語");
    assert_eq!(
        manifest
            .domains
            .iter()
            .map(|d| d.message_count)
            .sum::<usize>(),
        3
    );

    let target = packs_dir.join("ja");
    assert!(target.join("manifest.json").is_file());
    assert!(target.join("settings.json").is_file());
    assert!(target.join("core.json").is_file());
    assert!(
        staging_dirs(&packs_dir).is_empty(),
        "staging must be cleaned"
    );
}

#[test]
fn reinstall_same_code_replaces_files_and_leaves_no_backup() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_demo_zip(&zip_path);
    install_from_zip(&zip_path, &packs_dir).unwrap();

    // 新版本包：settings.json 换了内容，core.json 消失（合法：域可缺席）。
    let v2_manifest = manifest_json("ja", "日本語 v2");
    let zip_v2 = tmp.path().join("pack-v2.zip");
    write_zip(
        &zip_v2,
        &v2_manifest,
        &[(
            "settings.json",
            domain_json(&[("settings.language", "言語（新版）")]),
        )],
    );

    let manifest = install_from_zip(&zip_v2, &packs_dir).unwrap();
    assert_eq!(manifest.manifest.label, "日本語 v2");

    let target = packs_dir.join("ja");
    let settings = fs::read_to_string(target.join("settings.json")).unwrap();
    assert!(settings.contains("言語（新版）"));
    assert!(
        !target.join("core.json").exists(),
        "old domain file must be gone"
    );
    assert!(
        !packs_dir.join("ja.old.bak").exists(),
        "backup must be removed after success"
    );
    assert!(staging_dirs(&packs_dir).is_empty());
}

#[test]
fn inspect_does_not_touch_packs_dir() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_demo_zip(&zip_path);

    let inspection = inspect_language_pack_file(&zip_path).unwrap();
    assert_eq!(inspection.manifest.code, "ja");
    assert_eq!(inspection.domains.len(), 2);
    let settings = inspection
        .domains
        .iter()
        .find(|d| d.domain == "settings")
        .unwrap();
    assert_eq!(settings.message_count, 2);

    assert!(!packs_dir.exists(), "inspect must not create packs dir");
}

#[test]
fn missing_zip_file_is_a_clear_error() {
    let tmp = TempDir::new().unwrap();
    let err =
        install_from_zip(&tmp.path().join("nope.zip"), &tmp.path().join("packs")).unwrap_err();
    assert!(err.contains("language pack file not found"), "{err}");
    let err = inspect_language_pack_file(&tmp.path().join("nope.zip")).unwrap_err();
    assert!(err.contains("language pack file not found"), "{err}");
}

// ── 校验矩阵：每个拒绝规则一颗钉 ─────────────────────────────────────

fn expect_install_error(zip_path: &Path, packs_dir: &Path, needle: &str) {
    let err = install_from_zip(zip_path, packs_dir)
        .expect_err("pack must be rejected")
        .to_string();
    assert!(err.contains(needle), "expected `{needle}` in: {err}");
    assert!(
        staging_dirs(packs_dir).is_empty(),
        "failed install must leave no staging residue"
    );
}

#[test]
fn unknown_manifest_field_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_zip(
        &zip_path,
        r#"{"packVersion":1,"code":"ja","label":"x","version":"1.0.0","typo":1}"#,
        &[],
    );
    expect_install_error(&zip_path, &packs_dir, "unknown field");
}

#[test]
fn wrong_pack_version_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_zip(
        &zip_path,
        r#"{"packVersion":2,"code":"ja","label":"x","version":"1.0.0"}"#,
        &[],
    );
    expect_install_error(&zip_path, &packs_dir, "unsupported packVersion");
}

#[test]
fn builtin_locale_code_is_rejected() {
    for code in ["en", "zh-CN"] {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("pack.zip");
        let packs_dir = tmp.path().join("language-packs");
        write_zip(&zip_path, &manifest_json(code, "X"), &[]);
        expect_install_error(&zip_path, &packs_dir, "conflicts with built-in");
    }
}

#[test]
fn invalid_locale_code_is_rejected() {
    for code in ["EN", "toolongcode", "j", "ja-"] {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("pack.zip");
        let packs_dir = tmp.path().join("language-packs");
        write_zip(&zip_path, &manifest_json(code, "X"), &[]);
        expect_install_error(&zip_path, &packs_dir, "invalid locale code");
    }
}

#[test]
fn missing_manifest_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    // 只有域文件、没有 manifest 的包。
    let file = File::create(&zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_file(
        &mut zip,
        "core.json",
        &domain_json(&[("common.ok", "OK")]),
        options,
    );
    zip.finish().unwrap();

    expect_install_error(&zip_path, &packs_dir, "missing manifest.json");
}

#[test]
fn unexpected_file_name_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let packs_dir = tmp.path().join("language-packs");

    let mut cases: Vec<(String, String)> = vec![
        ("typos.json".into(), domain_json(&[("common.ok", "OK")])),
        ("core.txt".into(), domain_json(&[("common.ok", "OK")])),
        ("entry.js".into(), "console.log(1)".into()),
    ];
    // 子目录嵌套。
    cases.push((
        "nested/core.json".into(),
        domain_json(&[("common.ok", "OK")]),
    ));

    for (name, content) in cases {
        let zip_path = tmp
            .path()
            .join(format!("pack-{}.zip", name.replace('/', "_")));
        write_zip(
            &zip_path,
            &manifest_json("ja", "X"),
            &[(name.as_str(), content)],
        );
        let err = install_from_zip(&zip_path, &packs_dir).unwrap_err();
        assert!(
            err.contains("unexpected file") || err.contains("nested paths"),
            "case `{name}`: {err}"
        );
    }
}

// 注：重复条目用例无法用 ZipWriter 构造（zip crate 在写入时就拒绝重复
// 文件名，`InvalidArchive("Duplicate filename")`）；
// `extract_language_pack_zip` 的重复检测是针对手工构造档案的运行时防线，
// 保留 fail-closed。

#[test]
fn directory_entry_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    let file = File::create(&zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.add_directory("ja/", options).unwrap();
    add_file(
        &mut zip,
        "manifest.json",
        &manifest_json("ja", "X"),
        options,
    );
    zip.finish().unwrap();

    expect_install_error(&zip_path, &packs_dir, "directory entries are not allowed");
}

#[test]
fn hidden_file_entry_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_zip(
        &zip_path,
        &manifest_json("ja", "X"),
        &[(".enabled", String::from("true"))],
    );
    expect_install_error(&zip_path, &packs_dir, "hidden file not allowed");
}

#[test]
fn path_traversal_entry_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let packs_dir = tmp.path().join("language-packs");
    for name in ["../evil.json", "/abs.json", "a/../../evil.json"] {
        let zip_path = tmp
            .path()
            .join(format!("pack-{}.zip", name.replace('/', "_")));
        write_zip(
            &zip_path,
            &manifest_json("ja", "X"),
            &[(name, domain_json(&[("common.ok", "OK")]))],
        );
        let err = install_from_zip(&zip_path, &packs_dir).unwrap_err();
        assert!(
            err.contains("unexpected file")
                || err.contains("nested paths")
                || err.contains("absolute path")
                || err.contains("path traversal"),
            "traversal case `{name}`: {err}"
        );
    }
    assert!(staging_dirs(&packs_dir).is_empty());
}

#[test]
fn prototype_pollution_keys_are_rejected() {
    let tmp = TempDir::new().unwrap();
    let packs_dir = tmp.path().join("language-packs");
    for key in ["__proto__", "constructor", "prototype", "__defineSetter__"] {
        let zip_path = tmp
            .path()
            .join(format!("pack-{}.zip", key.replace('_', "-")));
        write_zip(
            &zip_path,
            &manifest_json("ja", "X"),
            &[("core.json", domain_json(&[(key, "polluted")]))],
        );
        expect_install_error(&zip_path, &packs_dir, "forbidden message key");
    }
}

#[test]
fn invalid_message_key_is_rejected_but_empty_value_is_valid() {
    let tmp = TempDir::new().unwrap();
    let packs_dir = tmp.path().join("language-packs");

    let zip_path = tmp.path().join("pack-key.zip");
    write_zip(
        &zip_path,
        &manifest_json("ja", "X"),
        &[("core.json", domain_json(&[("has space", "v")]))],
    );
    expect_install_error(&zip_path, &packs_dir, "invalid message key");

    // 空串译文合法（en 源字典自带空值键，如 `pagination.pageOf`）。
    let zip_path = tmp.path().join("pack-empty-value.zip");
    write_zip(
        &zip_path,
        &manifest_json("ja", "X"),
        &[("core.json", domain_json(&[("pagination.pageOf", "")]))],
    );
    install_from_zip(&zip_path, &packs_dir).unwrap();
}

#[test]
fn control_characters_in_value_are_rejected() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_zip(
        &zip_path,
        &manifest_json("ja", "X"),
        &[("core.json", "{\"common.ok\": \"a\\u0007b\"}".to_string())],
    );
    expect_install_error(&zip_path, &packs_dir, "control characters");
}

#[test]
fn cross_domain_duplicate_key_installs_last_alphabetical_domain_wins() {
    // 与内置字典同款的跨域重复键（en 的 common.aiNotConfigured 在 ai 与
    // core 两域）：合法，读取时字母序靠后的域（core）胜。
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_zip(
        &zip_path,
        &manifest_json("ja", "X"),
        &[
            (
                "ai.json",
                domain_json(&[("common.aiNotConfigured", "from-ai")]),
            ),
            (
                "core.json",
                domain_json(&[("common.aiNotConfigured", "from-core")]),
            ),
        ],
    );
    install_from_zip(&zip_path, &packs_dir).unwrap();
    let manager = crate::language_packs::LanguagePackManager::new(packs_dir);
    assert_eq!(manager.load_from_disk(), 1);
    let pack = manager.read_messages("ja").unwrap();
    assert_eq!(
        pack.messages.get("common.aiNotConfigured"),
        Some(&"from-core".to_string())
    );
}

#[test]
fn non_string_message_value_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_zip(
        &zip_path,
        &manifest_json("ja", "X"),
        &[("core.json", "{\"common.ok\": 42}".to_string())],
    );
    expect_install_error(&zip_path, &packs_dir, "invalid domain file");
}

#[test]
fn partial_pack_is_valid() {
    // 域文件可以缺席（部分翻译走 en 兜底是产品语义）。
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("pack.zip");
    let packs_dir = tmp.path().join("language-packs");
    write_zip(
        &zip_path,
        &manifest_json("ja", "X"),
        &[(
            "settings.json",
            domain_json(&[("settings.language", "言語")]),
        )],
    );
    let manifest = install_from_zip(&zip_path, &packs_dir).unwrap();
    assert_eq!(manifest.manifest.code, "ja");
    assert!(packs_dir.join("ja").join("settings.json").is_file());
}

// ── 纯函数：压缩比防线 ─────────────────────────────────────────────

#[test]
fn compression_ratio_math() {
    assert!(ratio_exceeds_limit(1_000_000, 100, 100));
    assert!(!ratio_exceeds_limit(10_000, 100, 100));
    // compressed == 0 且 uncompressed > 0 视为炸弹（无法自证清白）。
    assert!(ratio_exceeds_limit(1, 0, 100));
    assert!(!ratio_exceeds_limit(0, 0, 100));

    assert!(check_compression_ratio(101, 1, 100, "t").is_err());
    assert!(check_compression_ratio(100, 1, 100, "t").is_ok());
}

// ── manifest/校验直查（与 install 全链互补） ─────────────────────────

#[test]
fn manifest_roundtrip_through_domain_dir() {
    let tmp = TempDir::new().unwrap();
    let dir: PathBuf = tmp.path().join("ja");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("manifest.json"), DEMO_MANIFEST).unwrap();
    fs::write(dir.join("core.json"), domain_json(&[("common.ok", "了解")])).unwrap();

    let manifest: LanguagePackManifest = serde_json::from_str(DEMO_MANIFEST).unwrap();
    manifest.validate().unwrap();
    validate_locale_code("ja").unwrap();
    validate_message_key("common.ok").unwrap();
}
