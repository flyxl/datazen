//! 已装语言包注册表：列表、读译文、卸载、启动扫描计数。
//!
//! **无状态设计**（`app_state_shape_tests` 的 AppState 字段上限守卫裁决）：
//! 磁盘目录 `{packs_dir}/{code}/` 是唯一事实源，每个方法现读现扫——
//! 没有 `AppState` 字段、没有进程级缓存，也就没有跨窗口状态漂移。宿主命令
//! 每次从 `state.store.data_dir()` 现构实例（构造只是持一个路径，零成本）。
//!
//! 代价是 `list()` 每次全量解析（每包约 1MB JSON、毫秒级，设置页打开频率
//! 完全可承受）；换来的是 `register`/`unregister` 类缓存维护的整类缺失。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::install::validate_pack_dir;
use super::manifest::{validate_locale_code, LanguagePackManifest, LANGUAGE_PACK_DOMAINS};
use super::summary::{LanguagePackMessages, LanguagePackSummary};

/// 语言包注册表（目录由宿主注入；桌面：`{appData}/language-packs`）。
///
/// 实例可以按命令调用现构现弃（无内部可变状态）。
#[derive(Debug)]
pub struct LanguagePackManager {
    packs_dir: PathBuf,
}

impl LanguagePackManager {
    pub fn new(packs_dir: PathBuf) -> Self {
        Self { packs_dir }
    }

    /// 存储根目录（一个已装包一个子目录）。
    pub fn packs_dir(&self) -> &std::path::Path {
        &self.packs_dir
    }

    /// 扫描 `{packs_dir}`，返回合法包数（供启动日志）。坏包跳过并 warn
    /// （一个坏包不能炸 boot，与 wapp 扫描同语义）；staging/备份（点前缀）
    /// 忽略。同 `list()` 的校验规则。
    pub fn load_from_disk(&self) -> usize {
        self.list().len()
    }

    /// 已装包列表：扫描目录并全量校验，坏包跳过 + warn；按 code 排序的
    /// 稳定投影。
    pub fn list(&self) -> Vec<LanguagePackSummary> {
        let Ok(entries) = fs::read_dir(&self.packs_dir) else {
            return Vec::new();
        };
        let mut summaries: Vec<LanguagePackSummary> = Vec::new();
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
                    let message_count: usize =
                        inspection.domains.iter().map(|d| d.message_count).sum();
                    summaries.push(LanguagePackSummary {
                        code: inspection.manifest.code,
                        label: inspection.manifest.label,
                        version: inspection.manifest.version,
                        author: inspection.manifest.author,
                        description: inspection.manifest.description,
                        message_count,
                        file_name: name,
                    });
                }
                Err(error) => {
                    tracing::warn!(dir = %name, error = %error, "invalid language pack; skipping");
                }
            }
        }
        summaries.sort_by(|a, b| a.code.cmp(&b.code));
        summaries
    }

    /// 读取一个已装包的全部译文（manifest 现读取 label；域文件按字母序
    /// 合并、后者胜——与内置全量合并同语义，跨域重复键合法）。
    ///
    /// 应答带 `label`：前端注册进共享翻译引擎需要「code + label + messages」
    /// 三件，一次 IPC 拿全。未知/格式非法的 code 返回 NotFound 文案。
    pub fn read_messages(&self, code: &str) -> Result<LanguagePackMessages, String> {
        validate_locale_code(code).map_err(|_| format!("language pack not found: {code}"))?;
        let dir = self.packs_dir.join(code);
        let manifest_path = dir.join(super::manifest::MANIFEST_FILE);
        if !dir.is_dir() || !manifest_path.is_file() {
            return Err(format!("language pack not found: {code}"));
        }
        let manifest: LanguagePackManifest = serde_json::from_str(
            &fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest.json: {e}"))?,
        )
        .map_err(|e| format!("invalid language pack manifest: {e}"))?;

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
            label: manifest.label,
            messages,
        })
    }

    /// 卸载：删除 `{code}/` 目录（连同遗留备份）。未知 code 返回 NotFound
    /// 文案；前端负责先切换活动语言（回退 en）。
    pub fn remove(&self, code: &str) -> Result<(), String> {
        validate_locale_code(code).map_err(|_| format!("language pack not found: {code}"))?;
        let dir = self.packs_dir.join(code);
        let backup = self.packs_dir.join(format!("{code}.old.bak"));
        if !dir.exists() {
            return Err(format!("language pack not found: {code}"));
        }
        fs::remove_dir_all(&dir).map_err(|e| format!("remove language pack: {e}"))?;
        if backup.exists() {
            let _ = fs::remove_dir_all(&backup);
        }
        Ok(())
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
        install_from_zip(&zip_path, &packs_dir).unwrap();
        LanguagePackManager::new(packs_dir)
    }

    #[test]
    fn list_scans_disk_sorted_by_code() {
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
        assert!(json.get("pack_version").is_none());
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
    fn remove_deletes_directory() {
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
    fn list_skips_bad_packs_and_mismatched_dir_names() {
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

        let manager = LanguagePackManager::new(packs_dir);
        assert_eq!(manager.load_from_disk(), 1);
        let list = manager.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code, "ja");
        assert!(manager.read_messages("ja").is_ok());
    }

    #[test]
    fn list_with_missing_dir_is_empty() {
        let manager = LanguagePackManager::new(PathBuf::from("/nonexistent/language-packs"));
        assert_eq!(manager.load_from_disk(), 0);
        assert!(manager.list().is_empty());
    }

    #[test]
    fn summary_shape_is_camel_case_wire() {
        let summary = LanguagePackSummary {
            code: "ja".into(),
            label: "日本語".into(),
            version: "1.2.3".into(),
            author: Some("DataZen".into()),
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

    #[test]
    fn cross_domain_duplicate_key_last_alphabetical_domain_wins() {
        // manager 读取侧的同款裁决（install 侧另有安装用例）：ai 与 core
        // 两域同键，字母序靠后的 core 胜。
        let tmp = TempDir::new().unwrap();
        let packs_dir = tmp.path().join("language-packs");
        let dir = packs_dir.join("ja");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(MANIFEST_FILE),
            r#"{"packVersion":1,"code":"ja","label":"X","version":"1.0.0"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("ai.json"),
            r#"{"common.aiNotConfigured": "from-ai"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("core.json"),
            r#"{"common.aiNotConfigured": "from-core"}"#,
        )
        .unwrap();
        let manager = LanguagePackManager::new(packs_dir);
        let pack = manager.read_messages("ja").unwrap();
        assert_eq!(
            pack.messages.get("common.aiNotConfigured"),
            Some(&"from-core".to_string())
        );
    }
}
