use super::*;
use std::io::Write;
use std::sync::{Mutex, MutexGuard};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const DEMO_MANIFEST: &str = r#"{
      "id": "acme.demo",
      "name": "Demo",
      "version": "1.0.0",
      "apiVersion": 2,
      "entry": "index.html",
      "contributes": { "pages": [{ "id": "main", "title": "Main" }] },
      "permissions": ["storage:local"]
    }"#;

fn add_file(zip: &mut ZipWriter<File>, name: &str, content: &str, options: SimpleFileOptions) {
    zip.start_file(name, options).unwrap();
    zip.write_all(content.as_bytes()).unwrap();
}

fn write_demo_zip(zip_path: &Path, top_folder: Option<&str>) {
    let file = fs::File::create(zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let prefix = top_folder.map(|f| format!("{f}/")).unwrap_or_default();
    add_file(
        &mut zip,
        &format!("{prefix}manifest.json"),
        DEMO_MANIFEST,
        options,
    );
    add_file(
        &mut zip,
        &format!("{prefix}index.html"),
        "<html>v1</html>",
        options,
    );
    zip.finish().unwrap();
}

#[test]
fn install_from_zip_installs_valid_package() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("demo.zip");
    write_demo_zip(&zip_path, None);

    let wapps_root = TempDir::new().unwrap();
    let manifest = install_from_zip(&zip_path, wapps_root.path()).unwrap();
    assert_eq!(manifest.id, "acme.demo");

    let installed = wapps_root.path().join("acme.demo");
    assert!(installed.join("manifest.json").is_file());
    assert!(installed.join("index.html").is_file());
    assert_eq!(
        fs::read_to_string(installed.join("index.html")).unwrap(),
        "<html>v1</html>"
    );
    validate_wapp_dir(&installed).unwrap();
}

#[test]
fn install_from_zip_accepts_single_top_level_folder() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("demo.zip");
    write_demo_zip(&zip_path, Some("acme.demo"));

    let wapps_root = TempDir::new().unwrap();
    let manifest = install_from_zip(&zip_path, wapps_root.path()).unwrap();
    assert_eq!(manifest.id, "acme.demo");
    assert!(wapps_root.path().join("acme.demo/index.html").is_file());
}

#[test]
fn install_from_zip_rejects_traversal_entries() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("evil.zip");
    {
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        // 两级 `..`：staging 在 `{wapps_root}/.staging-{uuid}`，位于 wapps root 下一层，
        // 所以两级才真的越出被测的 wapps root，落点就是我们下面给的 `sandbox/outside.html`。
        add_file(&mut zip, "../../outside.html", "<html>evil</html>", options);
        zip.finish().unwrap();
    }

    // 本测试自建的沙箱顶层：wapps root 是它的子目录，越界目标就摆在这一层。
    // 断言因此只盯着本测试自己的目录树，与系统共享临时目录无关。
    // （原先盯的是 TempDir 路径的上一级目录，也就是**全局共享的 $TMPDIR**：
    //   别的进程或测试往里丢一个 `outside.html` 就能把这里搞红，
    //   而它并不能证明「条目没有逃出被安装的 wapps root」。）
    let sandbox = TempDir::new().unwrap();
    let wapps_root = sandbox.path().join("root");
    let escape_target = sandbox.path().join("outside.html");
    fs::create_dir_all(&wapps_root).unwrap();
    fs::write(wapps_root.join("keep.txt"), "unchanged").unwrap();

    let err = install_from_zip(&zip_path, &wapps_root).unwrap_err();
    assert!(
        err.contains("traversal") || err.contains("invalid zip entry"),
        "unexpected: {err}"
    );
    // Nothing escaped the wapps root and no staging dir remains.
    assert_eq!(
        fs::read_to_string(wapps_root.join("keep.txt")).unwrap(),
        "unchanged"
    );
    assert!(
        !escape_target.exists(),
        "越界条目不得落在沙箱顶层：{}",
        escape_target.display()
    );
    assert!(read_staging_dirs(&wapps_root).is_empty());
}

fn read_staging_dirs(root: &Path) -> Vec<String> {
    fs::read_dir(root)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| n.starts_with(".staging-"))
        .collect()
}

#[test]
fn install_from_zip_rejects_hidden_and_forbidden_entries() {
    for entry_name in [".storage.json", ".enabled", "assets/evil.sh", "run.exe"] {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("bad.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            add_file(&mut zip, entry_name, "payload", options);
            zip.finish().unwrap();
        }
        let wapps_root = TempDir::new().unwrap();
        let err = install_from_zip(&zip_path, wapps_root.path()).unwrap_err();
        assert!(
            err.contains("hidden file")
                || err.contains("forbidden extension")
                || err.contains("without extension"),
            "`{entry_name}`: unexpected error: {err}"
        );
    }
}

#[test]
fn extract_enforces_configurable_size_limit() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("big.zip");
    {
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        add_file(&mut zip, "blob.html", &"a".repeat(64 * 1024), options);
        zip.finish().unwrap();
    }

    let dest = tmp.path().join("out");
    let limits = WappZipLimits {
        max_uncompressed_bytes: 32 * 1024,
        ..WappZipLimits::default()
    };
    let err = extract_wapp_zip_with_limits(&zip_path, &dest, limits).unwrap_err();
    assert!(err.contains("uncompressed size limit"), "unexpected: {err}");
}

#[test]
fn install_from_zip_rejects_ratio_bomb_with_default_limits() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("bomb.zip");
    {
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        // 512 KiB of zeros compress far beyond MAX_COMPRESSION_RATIO.
        add_file(&mut zip, "bomb.html", &"0".repeat(512 * 1024), options);
        zip.finish().unwrap();
    }

    let wapps_root = TempDir::new().unwrap();
    let err = install_from_zip(&zip_path, wapps_root.path()).unwrap_err();
    assert!(err.contains("ratio"), "unexpected: {err}");
    assert!(read_staging_dirs(wapps_root.path()).is_empty());
}

#[test]
fn reinstall_backs_up_existing_and_cleans_backup() {
    let tmp = TempDir::new().unwrap();
    let wapps_root = TempDir::new().unwrap();

    let v1 = tmp.path().join("v1.zip");
    write_demo_zip(&v1, None);
    install_from_zip(&v1, wapps_root.path()).unwrap();

    let v2 = tmp.path().join("v2.zip");
    {
        let file = fs::File::create(&v2).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = DEMO_MANIFEST.replace("\"version\": \"1.0.0\"", "\"version\": \"2.0.0\"");
        add_file(&mut zip, "manifest.json", &manifest, options);
        add_file(&mut zip, "index.html", "<html>v2</html>", options);
        add_file(&mut zip, "assets/icon.svg", "<svg/>", options);
        zip.finish().unwrap();
    }

    let manifest = install_from_zip(&v2, wapps_root.path()).unwrap();
    assert_eq!(manifest.version, "2.0.0");

    let installed = wapps_root.path().join("acme.demo");
    assert_eq!(
        fs::read_to_string(installed.join("index.html")).unwrap(),
        "<html>v2</html>"
    );
    assert!(installed.join("assets/icon.svg").is_file());
    // Backup removed after success; no staging leftovers.
    assert!(!wapps_root.path().join("acme.demo.old.bak").exists());
    assert!(read_staging_dirs(wapps_root.path()).is_empty());
}

#[test]
fn failed_install_keeps_previous_package_and_cleans_staging() {
    let tmp = TempDir::new().unwrap();
    let wapps_root = TempDir::new().unwrap();

    let good = tmp.path().join("good.zip");
    write_demo_zip(&good, None);
    install_from_zip(&good, wapps_root.path()).unwrap();

    // Broken package: pages declared but entry file missing.
    let bad = tmp.path().join("bad.zip");
    {
        let file = fs::File::create(&bad).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = DEMO_MANIFEST.replace("\"version\": \"1.0.0\"", "\"version\": \"3.0.0\"");
        add_file(&mut zip, "manifest.json", &manifest, options);
        zip.finish().unwrap();
    }

    let err = install_from_zip(&bad, wapps_root.path()).unwrap_err();
    assert!(err.contains("entry"), "unexpected: {err}");

    let installed = wapps_root.path().join("acme.demo");
    assert_eq!(
        fs::read_to_string(installed.join("index.html")).unwrap(),
        "<html>v1</html>",
        "previous package must survive a failed upgrade"
    );
    assert!(read_staging_dirs(wapps_root.path()).is_empty());
}

#[test]
fn install_from_dir_installs_package() {
    let src = TempDir::new().unwrap();
    fs::write(src.path().join("manifest.json"), DEMO_MANIFEST).unwrap();
    fs::write(src.path().join("index.html"), "<html>dir-src</html>").unwrap();
    fs::create_dir_all(src.path().join("assets")).unwrap();
    fs::write(src.path().join("assets/icon.svg"), "<svg/>").unwrap();

    let wapps_root = TempDir::new().unwrap();
    let manifest = install_from_dir(src.path(), wapps_root.path()).unwrap();
    assert_eq!(manifest.id, "acme.demo");

    let installed = wapps_root.path().join("acme.demo");
    assert!(installed.join("assets/icon.svg").is_file());

    // Reinstall from an updated directory replaces the previous package.
    fs::write(src.path().join("index.html"), "<html>dir-v2</html>").unwrap();
    install_from_dir(src.path(), wapps_root.path()).unwrap();
    assert_eq!(
        fs::read_to_string(installed.join("index.html")).unwrap(),
        "<html>dir-v2</html>"
    );
    assert!(!wapps_root.path().join("acme.demo.old.bak").exists());
}

#[test]
fn install_from_dir_skips_hidden_files() {
    let src = TempDir::new().unwrap();
    fs::write(src.path().join("manifest.json"), DEMO_MANIFEST).unwrap();
    fs::write(src.path().join("index.html"), "<html></html>").unwrap();
    fs::write(src.path().join(".DS_Store"), "junk").unwrap();
    fs::create_dir_all(src.path().join(".git")).unwrap();
    fs::write(src.path().join(".git/config"), "[core]").unwrap();

    let wapps_root = TempDir::new().unwrap();
    install_from_dir(src.path(), wapps_root.path()).unwrap();

    let installed = wapps_root.path().join("acme.demo");
    assert!(!installed.join(".DS_Store").exists());
    assert!(!installed.join(".git").exists());
    validate_wapp_dir(&installed).unwrap();
}

#[test]
fn install_from_dir_rejects_symlinks_and_bad_extensions() {
    let src = TempDir::new().unwrap();
    fs::write(src.path().join("manifest.json"), DEMO_MANIFEST).unwrap();
    fs::write(src.path().join("index.html"), "<html></html>").unwrap();
    let target = src.path().join("index.html");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, src.path().join("link.html")).unwrap();
        let wapps_root = TempDir::new().unwrap();
        let err = install_from_dir(src.path(), wapps_root.path()).unwrap_err();
        assert!(err.contains("symlink not allowed"), "unexpected: {err}");
        let _ = fs::remove_file(src.path().join("link.html"));
    }

    fs::write(src.path().join("script.py"), "print('hi')").unwrap();
    let wapps_root = TempDir::new().unwrap();
    let err = install_from_dir(src.path(), wapps_root.path()).unwrap_err();
    assert!(err.contains("forbidden extension .py"), "unexpected: {err}");
}

#[test]
fn install_from_dir_missing_source_fails() {
    let wapps_root = TempDir::new().unwrap();
    let err = install_from_dir(Path::new("/nonexistent/src"), wapps_root.path()).unwrap_err();
    assert!(err.contains("not found"), "unexpected: {err}");
}

// Inspect staging dirs are created in the global temp dir by production
// code, so parallel tests would observe each other's transient
// `.datazen-inspect-*` entries and race on count_inspect_dirs(). Every
// test below that calls inspect_wapp_package holds this lock for its
// whole body; no other test module creates that prefix.
static INSPECT_TMP_LOCK: Mutex<()> = Mutex::new(());

fn inspect_tmp_lock() -> MutexGuard<'static, ()> {
    INSPECT_TMP_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn count_inspect_dirs() -> usize {
    fs::read_dir(std::env::temp_dir())
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(INSPECT_PREFIX))
        })
        .count()
}

#[test]
fn inspect_wapp_package_returns_manifest_without_installing() {
    let _inspect_guard = inspect_tmp_lock();
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("demo.zip");
    write_demo_zip(&zip_path, None);

    let before = count_inspect_dirs();
    let manifest = inspect_wapp_package(&zip_path).unwrap();
    assert_eq!(manifest.id, "acme.demo");
    assert_eq!(manifest.name, "Demo");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(
        manifest
            .permissions
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["storage:local"]
    );

    // The throwaway staging dir is cleaned up.
    assert_eq!(count_inspect_dirs(), before);
}

#[test]
fn inspect_wapp_package_accepts_top_level_folder_and_plain_dirs() {
    let _inspect_guard = inspect_tmp_lock();
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("demo.zip");
    write_demo_zip(&zip_path, Some("acme.demo"));
    let manifest = inspect_wapp_package(&zip_path).unwrap();
    assert_eq!(manifest.id, "acme.demo");

    // Directory sources keep install semantics: the folder name does not
    // have to match the manifest id (staging gets renamed on real install).
    let src = TempDir::new().unwrap();
    fs::write(src.path().join("manifest.json"), DEMO_MANIFEST).unwrap();
    fs::write(src.path().join("index.html"), "<html></html>").unwrap();
    let manifest = inspect_wapp_package(src.path()).unwrap();
    assert_eq!(manifest.id, "acme.demo");
}

#[test]
fn inspect_wapp_package_rejects_invalid_packages() {
    let _inspect_guard = inspect_tmp_lock();
    // 前后增量，而不是绝对 0：inspect 的 staging 根由生产代码写死为
    // `std::env::temp_dir()`，是**全局共享**目录，别处（src-tauri 的 wapps 测试、
    // GUI 的 registry）都可能留下同前缀的常驻目录。INSPECT_TMP_LOCK 只串行化本
    // 进程内的测试，对跨进程的常驻目录零保护 —— 绝对 0 会把别人留下的目录算到
    // 自己头上（实测：$TMPDIR 里放一个 .datazen-inspect-* 即 EXIT=101，left:1 right:0）。
    // 该前后增量形态对「常驻目录」免疫，对**瞬时并发**目录仍不免疫；彻底解法是给
    // inspect 的 staging 根一个可注入参数、测试指向自建 TempDir，属独立变更。
    let before = count_inspect_dirs();
    // Missing path.
    let err = inspect_wapp_package(Path::new("/nonexistent/datazen-inspect.zip")).unwrap_err();
    assert!(err.contains("not found"), "unexpected: {err}");

    // Manifest failing validation (apiVersion mismatch).
    let tmp = TempDir::new().unwrap();
    let bad = tmp.path().join("bad.zip");
    {
        let file = fs::File::create(&bad).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let manifest = DEMO_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 99");
        add_file(&mut zip, "manifest.json", &manifest, options);
        add_file(&mut zip, "index.html", "<html></html>", options);
        zip.finish().unwrap();
    }
    let err = inspect_wapp_package(&bad).unwrap_err();
    assert!(err.contains("apiVersion"), "unexpected: {err}");

    // Malicious traversal entry is rejected by the shared extraction rules.
    let evil = tmp.path().join("evil.zip");
    {
        let file = fs::File::create(&evil).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        add_file(&mut zip, "../outside.html", "<html>evil</html>", options);
        zip.finish().unwrap();
    }
    let err = inspect_wapp_package(&evil).unwrap_err();
    assert!(
        err.contains("traversal") || err.contains("invalid zip entry"),
        "unexpected: {err}"
    );
    assert_eq!(count_inspect_dirs(), before);
}
