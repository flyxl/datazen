//! zh-CN strings, generated from the React locale packs at build time.
//!
//! `packages/frontend/src/locales/zh-CN/*.ts` stays the single source of truth
//! (see `build.rs`), so a translated string never exists twice. Callers ask for
//! a key and get the Chinese text; an unknown key falls back to the key itself
//! so a missing entry shows up in the UI instead of panicking or rendering
//! nothing.
//!
//! Placeholders such as `{count}` stay in the returned text: the caller formats
//! them, exactly as the React `t('…', { count })` call does.

include!(concat!(env!("OUT_DIR"), "/i18n_zh.rs"));

/// The Chinese text behind `key`, or `key` itself when the table lacks it.
///
/// A few React packs intentionally map a key to the empty string (a status line
/// with nothing to say yet); those come back empty, matching `t()` there.
pub(crate) fn t(key: &str) -> &str {
    match lookup(key) {
        Some(text) => text,
        None => key,
    }
}

/// Whether `key` has a translation; useful for a parity test over used keys.
pub(crate) fn has(key: &str) -> bool {
    lookup(key).is_some()
}

/// How many entries the generated table holds.
pub(crate) fn len() -> usize {
    ZH_CN.len()
}

fn lookup(key: &str) -> Option<&'static str> {
    match ZH_CN.binary_search_by(|(candidate, _)| (*candidate).cmp(key)) {
        Ok(index) => Some(ZH_CN[index].1),
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

    /// The keys in every `t("…")` call in `source`.
    fn translation_keys(source: &str) -> Vec<&str> {
        let bytes = source.as_bytes();
        let mut keys = Vec::new();
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
            if let Some(key) = source.get(start..end) {
                keys.push(key);
            }
            i = end + 1;
        }
        keys
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
