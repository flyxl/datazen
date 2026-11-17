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

    #[test]
    fn placeholders_survive_translation() {
        let Some((_, text)) = ZH_CN.iter().find(|(_, text)| text.contains("{count}")) else {
            panic!("no zh-CN entry carries a {{count}} placeholder");
        };
        assert!(text.contains("{count}"));
    }
}
