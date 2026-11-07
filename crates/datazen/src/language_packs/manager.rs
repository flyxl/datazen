//! 已装语言包注册表：启动扫描、列表、读译文、卸载。
//!
//! 与 `wapps::manager` 同构的共享注册表：磁盘目录 `{packs_dir}/{code}/`
//! 是事实源，内存表是投影（启动扫描 + 安装后 `register` 维护）。所有方法
//! 线程安全；读译文每次都从磁盘现读（活动包注册以磁盘为准），列表走
//! 内存投影（设置页打开/跨窗口事件刷新用，避免每次全量解析）。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use super::install::validate_pack_dir;
use super::manifest::{validate_locale_code, LanguagePackManifest, LANGUAGE_PACK_DOMAINS};
use super::summary::{LanguagePackMessages, LanguagePackSummary};

/// 单个已装包的内存投影。
#[derive(Debug, Clone)]
struct LoadedPack {
    manifest: LanguagePackManifest,
    message_count: usize,
}

/// 共享语言包注册表。目录由宿主注入（桌面：`{appData}/language-packs`）。
#[derive(Debug)]
pub struct LanguagePackManager {
    packs_dir: PathBuf,
    packs: RwLock<BTreeMap<String, LoadedPack>>,
}

impl LanguagePackManager {
    pub fn new(packs_dir: PathBuf) -> Self {
        Self {
            packs_dir,
            packs: RwLock::new(BTreeMap::new()),
        }
    }

    /// 存储根目录（一个已装包一个子目录）。
    pub fn packs_dir(&self) -> &std::path::Path {
        &self.packs_dir
    }

    /// 安装成功后由宿主调用：登记（或覆盖）一个包的内存投影。
    pub fn register(&self, manifest: LanguagePackManifest, message_count: usize) {
        let code = manifest.code.clone();
        let mut packs = self.write_lock();
        packs.insert(
            code,
            LoadedPack {
                manifest,
                message_count,
            },
        );
    }

    /// 扫描 `{packs_dir}` 并登记每个合法包；坏包跳过并 warn（一个坏包
    /// 不能炸 boot，与 wapp 扫描同语义）。staging/备份（点前缀）忽略。
    /// 返回登记数（供启动日志）。
    pub fn load_from_disk(&self) -> usize {
        let mut packs = self.write_lock();
        packs.clear();

        let Ok(entries) = fs::read_dir(&self.packs_dir) else {
            return 0;
        };
        let mut loaded = 0usize;
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            match validate_pack_dir(&path) {
                Ok(inspection) => {
                    let message_count: usize =
                        inspection.domains.iter().map(|d| d.message_count).sum();
                    // 目录名必须与 manifest.code 一致（安装时的不变式；
                    // 手工搬动目录属完整性破坏，跳过并告警）。
                    if inspection.manifest.code != name {
                        tracing::warn!(
                            dir = %name,
                            code = %inspection.manifest.code,
                            "language pack dir name does not match manifest code; skipping"
                        );
                        continue;
                    }
                    packs.insert(
                        inspection.manifest.code.clone(),
                        LoadedPack {
                            manifest: inspection.manifest,
                            message_count,
                        },
                    );
                    loaded += 1;
                }
                Err(error) => {
                    tracing::warn!(dir = %name, error = %error, "invalid language pack; skipping");
                }
            }
        }
        loaded
    }

    /// 已装包列表（按 code 排序的稳定投影）。
    pub fn list(&self) -> Vec<LanguagePackSummary> {
        let packs = self.read_lock();
        packs
            .values()
            .map(|loaded| LanguagePackSummary {
                code: loaded.manifest.code.clone(),
                label: loaded.manifest.label.clone(),
                version: loaded.manifest.version.clone(),
                author: loaded.manifest.author.clone(),
                description: loaded.manifest.description.clone(),
                message_count: loaded.message_count,
                file_name: loaded.manifest.code.clone(),
            })
            .collect()
    }

    /// 读取一个已装包的全部译文（磁盘现读，合并全部域文件）。
    ///
    /// 应答带 `label`：前端注册进共享翻译引擎需要「code + label + messages」
    /// 三件，一次 IPC 拿全。未知/未登记的 code 返回 NotFound 文案。
    pub fn read_messages(&self, code: &str) -> Result<LanguagePackMessages, String> {
        validate_locale_code(code).map_err(|_| format!("language pack not found: {code}"))?;
        let (label, dir) = {
            let packs = self.read_lock();
            let loaded = packs
                .get(code)
                .ok_or_else(|| format!("language pack not found: {code}"))?;
            (loaded.manifest.label.clone(), self.packs_dir.join(code))
        };

        // 域文件按字母序合并、后者胜——与内置全量合并（locales/<code>/
        // index.ts 的字母序 import 展开）对同一份源数据产出同一个生效值；
        // 跨域重复键因此合法（如 en 的 `common.aiNotConfigured`）。
        let mut domains: Vec<&str> = LANGUAGE_PACK_DOMAINS.to_vec();
        domains.sort_unstable();
        let mut messages: BTreeMap<String, String> = BTreeMap::new();
        for domain in domains {
            let path = dir.join(format!("{domain}.json"));
            if !path.is_file() {
                continue;
            }
            let text =
                fs::read_to_string(&path).map_err(|e| format!("read `{domain}.json`: {e}"))?;
            let map: BTreeMap<String, String> = serde_json::from_str(&text)
                .map_err(|e| format!("invalid domain file `{domain}.json`: {e}"))?;
            for (key, value) in map {
                // 跨域重复键：字母序靠后的域胜（确定性、与内置同语义）。
                messages.insert(key, value);
            }
        }
        Ok(LanguagePackMessages {
            code: code.to_string(),
            label,
            messages,
        })
    }

    /// 卸载：删除 `{code}/` 目录（连同遗留备份），并从内存表移除。
    /// 未知 code 返回 NotFound 文案；前端负责先切换活动语言（回退 en）。
    pub fn remove(&self, code: &str) -> Result<(), String> {
        validate_locale_code(code).map_err(|_| format!("language pack not found: {code}"))?;
        {
            let packs = self.read_lock();
            if !packs.contains_key(code) {
                return Err(format!("language pack not found: {code}"));
            }
        }
        let dir = self.packs_dir.join(code);
        let backup = self.packs_dir.join(format!("{code}.old.bak"));
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("remove language pack: {e}"))?;
        }
        if backup.exists() {
            let _ = fs::remove_dir_all(&backup);
        }
        self.write_lock().remove(code);
        Ok(())
    }

    fn read_lock(&self) -> std::sync::RwLockReadGuard<'_, BTreeMap<String, LoadedPack>> {
        self.packs.read().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "language pack registry read lock poisoned; recovering");
            e.into_inner()
        })
    }

    fn write_lock(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<String, LoadedPack>> {
        self.packs.write().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "language pack registry write lock poisoned; recovering");
            e.into_inner()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_packs::install::install_from_zip;
    use crate::language_packs::manifest::{MANIFEST_FILE, PACK_VERSION};
    use std::collections::BTreeMap;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn add_file(zip: &mut ZipWriter<File>, name: &str, content: &str) {
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file(name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }

    fn write_pack_zip(zip_path: &Path, code: &str, label: &str) {
        let file = File::create(zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        add_file(
            &mut zip,
            MANIFEST_FILE,
            &format!(
                r#"{{"packVersion":{PACK_VERSION},"code":"{code}","label":"{label}","version":"1.0.0"}}"#
            ),
        );
        add_file(
            &mut zip,
            "settings.json",
            "{\"settings.language\": \"言語\", \"settings.general\": \"全般\"}",
        );
        add_file(&mut zip, "core.json", "{\"common.ok\": \"了解\"}");
        zip.finish().unwrap();
    }

    fn installed_manager(tmp: &TempDir, code: &str, label: &str) -> LanguagePackManager {
        let zip_path = tmp.path().join("pack.zip");
        let packs_dir = tmp.path().join("language-packs");
        write_pack_zip(&zip_path, code, label);
        let inspection = install_from_zip(&zip_path, &packs_dir).unwrap();
        let manager = LanguagePackManager::new(packs_dir);
        let count: usize = inspection.domains.iter().map(|d| d.message_count).sum();
        manager.register(inspection.manifest, count);
        manager
    }

    #[test]
    fn list_reflects_registered_pack_sorted_by_code() {
        let tmp = TempDir::new().unwrap();
        let manager = installed_manager(&tmp, "ja", "日本語");
        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code, "ja");
        assert_eq!(list[0].label, "日本語");
        assert_eq!(list[0].message_count, 3);
        assert_eq!(list[0].file_name, "ja");
        // 序列化形状钉子：camelCase 线契约。
        let json = serde_json::to_value(&list[0]).unwrap();
        assert!(json.get("messageCount").is_some());
        assert!(json.get("packVersion").is_none());
    }

    #[test]
    fn read_messages_merges_domains_with_label() {
        let tmp = TempDir::new().unwrap();
        let manager = installed_manager(&tmp, "ja", "日本語");
        let pack = manager.read_messages("ja").unwrap();
        assert_eq!(pack.code, "ja");
        assert_eq!(pack.label, "日本語");
        assert_eq!(pack.messages.len(), 3);
        assert_eq!(
            pack.messages.get("settings.language"),
            Some(&"言語".to_string())
        );
        assert_eq!(pack.messages.get("common.ok"), Some(&"了解".to_string()));

        // 序列化形状钉子。
        let json = serde_json::to_value(&pack).unwrap();
        assert!(json.get("messages").is_some());
        assert_eq!(json["label"], "日本語");
    }

    #[test]
    fn read_unknown_or_builtin_code_is_not_found() {
        let tmp = TempDir::new().unwrap();
        let manager = installed_manager(&tmp, "ja", "日本語");
        for code in ["de", "en", "zh-CN", "", "no/such", ".."] {
            let err = manager.read_messages(code).unwrap_err();
            assert!(err.contains("language pack not found"), "{code}: {err}");
        }
    }

    #[test]
    fn remove_deletes_directory_and_registry_entry() {
        let tmp = TempDir::new().unwrap();
        let manager = installed_manager(&tmp, "ja", "日本語");
        let dir = manager.packs_dir().join("ja");
        assert!(dir.is_dir());

        manager.remove("ja").unwrap();
        assert!(!dir.exists());
        assert!(manager.list().is_empty());
        assert!(manager.read_messages("ja").is_err());

        let err = manager.remove("ja").unwrap_err();
        assert!(err.contains("language pack not found"));
    }

    #[test]
    fn load_from_disk_scans_valid_packs_and_skips_bad_ones() {
        let tmp = TempDir::new().unwrap();
        let packs_dir = tmp.path().join("language-packs");

        // 合法包 ja。
        let zip_path = tmp.path().join("pack.zip");
        write_pack_zip(&zip_path, "ja", "日本語");
        install_from_zip(&zip_path, &packs_dir).unwrap();

        // 坏包 ko：manifest 非法（code 冲突内置）。
        let bad_dir = packs_dir.join("ko");
        fs::create_dir_all(&bad_dir).unwrap();
        fs::write(
            bad_dir.join(MANIFEST_FILE),
            r#"{"packVersion":1,"code":"en","label":"X","version":"1.0.0"}"#,
        )
        .unwrap();

        // 目录名与 manifest.code 不一致的包。
        let mismatch_dir = packs_dir.join("zz");
        fs::create_dir_all(&mismatch_dir).unwrap();
        fs::write(
            mismatch_dir.join(MANIFEST_FILE),
            r#"{"packVersion":1,"code":"ja","label":"Y","version":"2.0.0"}"#,
        )
        .unwrap();

        // staging 残留（点前缀，忽略）。
        fs::create_dir_all(packs_dir.join(".staging-xyz")).unwrap();

        let manager = LanguagePackManager::new(packs_dir.clone());
        assert_eq!(manager.load_from_disk(), 1);
        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code, "ja");
        // 扫描后 read_messages 也可用（内存表已登记）。
        assert!(manager.read_messages("ja").is_ok());
    }

    #[test]
    fn load_from_disk_with_missing_dir_returns_zero() {
        let manager = LanguagePackManager::new(PathBuf::from("/nonexistent/language-packs"));
        assert_eq!(manager.load_from_disk(), 0);
        assert!(manager.list().is_empty());
    }

    #[test]
    fn register_overrides_existing_projection() {
        let tmp = TempDir::new().unwrap();
        let manager = installed_manager(&tmp, "ja", "日本語");

        let manifest = LanguagePackManifest {
            pack_version: PACK_VERSION,
            code: "ja".into(),
            label: "日本語 v2".into(),
            version: "2.0.0".into(),
            author: None,
            description: None,
        };
        manager.register(manifest, 42);

        let list = manager.list();
        assert_eq!(list[0].version, "2.0.0");
        assert_eq!(list[0].message_count, 42);
        // 磁盘是事实源：read_messages 仍读旧文件的译文。
        let pack = manager.read_messages("ja").unwrap();
        assert_eq!(pack.label, "日本語 v2");
        assert_eq!(pack.messages.len(), 3);
    }

    #[test]
    fn summary_shape_is_camel_case_wire() {
        let manifest = LanguagePackManifest {
            pack_version: PACK_VERSION,
            code: "ja".into(),
            label: "日本語".into(),
            version: "1.2.3".into(),
            author: Some("DataZen".into()),
            description: None,
        };
        let summary = LanguagePackSummary {
            code: manifest.code.clone(),
            label: manifest.label.clone(),
            version: manifest.version.clone(),
            author: manifest.author.clone(),
            description: None,
            message_count: 3,
            file_name: "ja".into(),
        };
        let json = serde_json::to_value(&summary).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        for expected in [
            "code",
            "label",
            "version",
            "author",
            "messageCount",
            "fileName",
        ] {
            assert!(keys.contains(&expected), "missing {expected} in {keys:?}");
        }
        assert!(!keys.contains(&"pack_version"));
    }

    #[test]
    fn messages_map_serializes_as_plain_object() {
        let mut messages = BTreeMap::new();
        messages.insert("common.ok".to_string(), "了解".to_string());
        let wire = LanguagePackMessages {
            code: "ja".into(),
            label: "日本語".into(),
            messages,
        };
        let json = serde_json::to_value(&wire).unwrap();
        assert_eq!(json["messages"]["common.ok"], "了解");
    }
}
