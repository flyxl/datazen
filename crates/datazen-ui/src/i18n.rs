//! Built-in strings, generated from the React locale packs at build time.
//!
//! The packs stay the source of truth (see `build.rs`), with a few native-only
//! status strings that the React UI does not use. Native Rune initially
//! uses Simplified Chinese, matching prior releases; an unknown key falls back
//! to the key itself rather than rendering nothing.
//!
//! Placeholders such as `{count}` stay in the returned text: the caller formats
//! them, exactly as the React `t('…', { count })` call does.

use std::cell::Cell;

use datazen_ui_api::BuiltinLanguage;

include!(concat!(env!("OUT_DIR"), "/i18n_builtin.rs"));

thread_local! {
    static RENDER_LANGUAGE: Cell<BuiltinLanguage> =
        const { Cell::new(BuiltinLanguage::SimplifiedChinese) };
}

/// Bind translations to one app's view build rather than mutating global
/// state that would survive into another window, app instance, or test.
pub(crate) fn with_language<R>(locale: BuiltinLanguage, render: impl FnOnce() -> R) -> R {
    struct Restore(BuiltinLanguage);
    impl Drop for Restore {
        fn drop(&mut self) {
            RENDER_LANGUAGE.set(self.0);
        }
    }

    let _restore = Restore(RENDER_LANGUAGE.replace(locale));
    render()
}

/// The rendered locale's text, the original Chinese text if missing in
/// English, or the key when neither table contains it.
///
/// A few React packs intentionally map a key to the empty string (a status line
/// with nothing to say yet); those come back empty, matching `t()` there.
pub fn t(key: &str) -> &str {
    let locale = RENDER_LANGUAGE.get();
    lookup(locale, key)
        .or_else(|| lookup(BuiltinLanguage::SimplifiedChinese, key))
        .unwrap_or(key)
}

/// Whether `key` has a translation; useful for a parity test over used keys.
pub fn has(key: &str) -> bool {
    lookup(BuiltinLanguage::SimplifiedChinese, key).is_some()
}

/// How many entries the generated table holds.
pub fn len() -> usize {
    ZH_CN.len()
}

fn lookup(locale: BuiltinLanguage, key: &str) -> Option<&'static str> {
    let native = match key {
        "settings.nativeEditorFont.bundled" => Some(("内置等宽字体（IBM Plex Mono）", "Bundled monospace (IBM Plex Mono)")),
        "settings.nativeEditorFont.system" => Some(("系统等宽字体（不可用时使用内置字体）", "System monospace (bundled fallback)")),
        "extensions.page.nativeListUnavailable" => Some((
            "当前平台无法读取本地扩展列表。",
            "Installed extensions are unavailable on this platform.",
        )),
        "extensions.page.nativeListUnreadable" => Some((
            "无法验证本地扩展列表。",
            "Could not verify the installed extensions.",
        )),
        "extensions.page.nativeReadOnly" => Some((
            "仅查看已安装的扩展。无法打开、启用、禁用、卸载或应用扩展主题。",
            "Installed extensions are read-only. Opening, enabling, disabling, uninstalling, and applying themes are unavailable.",
        )),
        _ => None,
    };
    if let Some((zh, en)) = native {
        return Some(match locale {
            BuiltinLanguage::SimplifiedChinese => zh,
            BuiltinLanguage::English => en,
        });
    }
    let table: &[(&str, &str)] = match locale {
        BuiltinLanguage::SimplifiedChinese => &ZH_CN,
        BuiltinLanguage::English => &EN,
    };
    match table.binary_search_by(|(candidate, _)| (*candidate).cmp(key)) {
        Ok(index) => Some(table[index].1),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_the_react_packs() {
        assert!(len() > 3_000, "only {} zh-CN entries", len());
        let mut previous = "";
        for (key, _) in ZH_CN.iter() {
            assert!(*key > previous, "{key} is out of order");
            previous = key;
        }
    }

    #[test]
    fn a_known_key_returns_its_chinese_text() {
        assert_eq!(t("common.save"), "保存");
        assert_eq!(t("common.cancel"), "取消");
        assert!(has("common.save"));
    }

    #[test]
    fn english_table_is_used_only_in_its_render_scope() {
        assert_eq!(
            EN.iter().map(|(key, _)| key).collect::<Vec<_>>(),
            ZH_CN.iter().map(|(key, _)| key).collect::<Vec<_>>()
        );
        with_language(BuiltinLanguage::English, || {
            assert_eq!(t("settings.appearance"), "Appearance");
            with_language(BuiltinLanguage::SimplifiedChinese, || {
                assert_eq!(t("settings.appearance"), "外观");
            });
            assert_eq!(t("settings.appearance"), "Appearance");
        });
        assert_eq!(t("settings.appearance"), "外观");
    }

    #[test]
    fn an_unknown_key_falls_back_to_the_key() {
        assert_eq!(t("datazen.no.such.key"), "datazen.no.such.key");
        assert!(!has("datazen.no.such.key"));
    }

    /// Every `t("key")` written in the crate must resolve to a pack entry, so a
    /// typo shows up in the test run instead of in the UI as a raw key.
    #[test]
    fn every_key_the_ui_asks_for_exists() {
        let mut checked = 0;
        for (path, source) in rust_sources() {
            if path.file_name().is_some_and(|name| name == "i18n.rs") {
                continue;
            }
            for key in translation_keys(&source) {
                assert!(
                    has(key),
                    "{} asks for {key}, which the zh-CN packs do not define",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no t(\"…\") call found in the crate");
    }

    /// Every `.rs` file under `src/`, with its text.
    fn rust_sources() -> Vec<(std::path::PathBuf, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut sources = Vec::new();
        collect(&root, &mut sources);
        sources
    }

    fn collect(dir: &std::path::Path, into: &mut Vec<(std::path::PathBuf, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, into);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    into.push((path, text));
                }
            }
        }
    }

    /// The keys in every `t("…")` call in `source`'s code lines. A `//` line —
    /// `//!` and `///` doc comments included — is prose: a quoted example like
    /// `t("...")` in a doc comment describes the call sites the guard checks,
    /// and is not a key the packs must define.
    fn translation_keys(source: &str) -> Vec<&str> {
        let mut keys = Vec::new();
        for line in source.lines() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            translation_keys_in_line(line, &mut keys);
        }
        keys
    }

    /// The `t("…")` keys in one code line.
    fn translation_keys_in_line<'a>(line: &'a str, keys: &mut Vec<&'a str>) {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i + 2 < bytes.len() {
            let call = bytes[i] == b't'
                && bytes[i + 1] == b'('
                && bytes[i + 2] == b'"'
                && (i == 0 || !is_ident(bytes[i - 1]));
            if !call {
                i += 1;
                continue;
            }
            let start = i + 3;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'"' {
                end += 1;
            }
            if end >= bytes.len() {
                break;
            }
            if let Some(key) = line.get(start..end) {
                keys.push(key);
            }
            i = end + 1;
        }
    }

    fn is_ident(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    #[test]
    fn placeholders_survive_translation() {
        let Some((_, text)) = ZH_CN.iter().find(|(_, text)| text.contains("{count}")) else {
            panic!("no zh-CN entry carries a {{count}} placeholder");
        };
        assert!(text.contains("{count}"));
    }
}
