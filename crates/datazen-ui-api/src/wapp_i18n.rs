use std::collections::BTreeMap;

use serde_json::Value;

pub type WappDictionary = BTreeMap<String, String>;

pub struct WappI18nDeps {
    pub read_wapp_file: Box<dyn Fn(&str, &str) -> Result<Vec<u8>, String>>,
}

#[derive(Default)]
pub struct WappI18n {
    cache: BTreeMap<String, Option<WappDictionary>>,
}

pub const WAPP_I18N_GAP: [&str; 3] = [
    "wappCommands.readWappFile(wappId, `locales/${locale}.json`)",
    "new TextDecoder().decode(new Uint8Array(bytes))",
    "JSON.parse(text)",
];

pub const WAPP_I18N_SURFACE: [&str; 8] = [
    "resolveWappString",
    "readDictionary",
    "dictionaryCache",
    "KEY_RE",
    "LOCALE_RE",
    "WappI18nDeps",
    "WappI18n",
    "WAPP_I18N_GAP",
];

pub fn is_wapp_i18n_key_valid(key: &str) -> bool {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 {
        return false;
    }
    if !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'.' || *byte == b'_' || *byte == b'-')
}

pub fn is_wapp_i18n_locale_valid(locale: &str) -> bool {
    let (language, region) = match locale.split_once('-') {
        Some((language, region)) => (language, Some(region)),
        None => (locale, None),
    };
    if language.len() < 2
        || language.len() > 3
        || !language.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }
    match region {
        None => true,
        Some(region) => {
            !region.is_empty()
                && region.len() <= 8
                && region.bytes().all(|byte| byte.is_ascii_alphanumeric())
        }
    }
}

pub fn wapp_i18n_candidates(locale: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let base = locale.split('-').next().unwrap_or(locale);
    for candidate in [locale, base, "en"] {
        if !candidates.iter().any(|existing| existing == candidate) {
            candidates.push(candidate.to_owned());
        }
    }
    candidates
}

pub fn wapp_i18n_dictionary_path(locale: &str) -> String {
    format!("locales/{locale}.json")
}

fn parse_dictionary(bytes: &[u8]) -> Option<WappDictionary> {
    let text = String::from_utf8_lossy(bytes);
    let value: Value = serde_json::from_str(&text).ok()?;
    let object = value.as_object()?;
    let mut dictionary = WappDictionary::new();
    for (key, value) in object {
        if let Some(text) = value.as_str() {
            dictionary.insert(key.clone(), text.to_owned());
        }
    }
    Some(dictionary)
}

impl WappI18n {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.cache.clear();
    }

    pub fn is_cached(&self, wapp_id: &str, locale: &str) -> bool {
        self.cache.contains_key(&format!("{wapp_id}:{locale}"))
    }

    pub fn cached_locale_count(&self) -> usize {
        self.cache.len()
    }

    fn ensure_dictionary(&mut self, deps: &WappI18nDeps, wapp_id: &str, locale: &str) {
        let cache_key = format!("{wapp_id}:{locale}");
        if self.cache.contains_key(&cache_key) {
            return;
        }
        let path = wapp_i18n_dictionary_path(locale);
        let dictionary = match (deps.read_wapp_file)(wapp_id, &path) {
            Ok(bytes) => parse_dictionary(&bytes),
            Err(_) => None,
        };
        self.cache.insert(cache_key, dictionary);
    }

    pub fn resolve(
        &mut self,
        deps: &WappI18nDeps,
        wapp_id: &str,
        key: &str,
        locale: &str,
    ) -> Option<String> {
        if !is_wapp_i18n_key_valid(key) || !is_wapp_i18n_locale_valid(locale) {
            return None;
        }
        for candidate in wapp_i18n_candidates(locale) {
            self.ensure_dictionary(deps, wapp_id, &candidate);
            let cache_key = format!("{wapp_id}:{candidate}");
            let value = self
                .cache
                .get(&cache_key)
                .and_then(|entry| entry.as_ref())
                .and_then(|dictionary| dictionary.get(key))
                .cloned();
            if value.is_some() {
                return value;
            }
        }
        None
    }
}

pub fn resolve_wapp_string(
    runtime: &mut WappI18n,
    deps: &WappI18nDeps,
    wapp_id: &str,
    key: &str,
    locale: &str,
) -> Option<String> {
    runtime.resolve(deps, wapp_id, key, locale)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use serde_json::json;

    use super::*;

    type Calls = Rc<RefCell<Vec<(String, String)>>>;

    fn deps(files: Vec<(String, Vec<u8>)>, calls: &Calls) -> WappI18nDeps {
        let calls = Rc::clone(calls);
        WappI18nDeps {
            read_wapp_file: Box::new(move |wapp_id, path| {
                calls
                    .borrow_mut()
                    .push((wapp_id.to_owned(), path.to_owned()));
                match files.iter().find(|(candidate, _)| candidate == path) {
                    Some((_, bytes)) => Ok(bytes.clone()),
                    None => Err(format!("missing {path}")),
                }
            }),
        }
    }

    fn file(path: &str, value: serde_json::Value) -> (String, Vec<u8>) {
        (path.to_owned(), value.to_string().into_bytes())
    }

    fn text(value: Option<String>) -> String {
        value.unwrap_or_else(|| panic!("expected a resolved string"))
    }

    #[test]
    fn key_grammar_matches_the_react_regex() {
        let longest = "x".repeat(128);
        let too_long = "x".repeat(129);
        for key in ["g", "greet", "A.b_c-9", longest.as_str()] {
            assert!(is_wapp_i18n_key_valid(key), "{key} should be valid");
        }
        for key in ["", ".greet", "a b", "a/b", "a@b", "你好", too_long.as_str()] {
            assert!(!is_wapp_i18n_key_valid(key), "{key} should be invalid");
        }
    }

    #[test]
    fn locale_grammar_matches_the_react_regex() {
        for locale in ["en", "zh", "zh-CN", "de-DE", "en-1", "abc-12345678"] {
            assert!(
                is_wapp_i18n_locale_valid(locale),
                "{locale} should be valid"
            );
        }
        for locale in [
            "",
            "e",
            "abcd",
            "en-",
            "en-123456789",
            "en_US",
            "en-US-x",
            "中文",
            "EN-abc!",
        ] {
            assert!(
                !is_wapp_i18n_locale_valid(locale),
                "{locale} should be invalid"
            );
        }
    }

    #[test]
    fn candidates_are_deduplicated_in_exact_base_en_order() {
        assert_eq!(wapp_i18n_candidates("en"), ["en"]);
        assert_eq!(wapp_i18n_candidates("zh-CN"), ["zh-CN", "zh", "en"]);
        assert_eq!(wapp_i18n_candidates("de"), ["de", "en"]);
        assert_eq!(wapp_i18n_candidates("EN"), ["EN", "en"]);
    }

    #[test]
    fn dictionary_path_is_the_package_relative_json() {
        assert_eq!(wapp_i18n_dictionary_path("zh-CN"), "locales/zh-CN.json");
        assert_eq!(wapp_i18n_dictionary_path("en"), "locales/en.json");
    }

    #[test]
    fn resolve_prefers_exact_locale_then_base_then_en() {
        let calls = Calls::default();
        let deps = deps(
            vec![
                file(
                    "locales/zh-CN.json",
                    json!({"onlyExact": "精确", "shared": "精确共享"}),
                ),
                file(
                    "locales/zh.json",
                    json!({"onlyBase": "基础", "shared": "基础共享"}),
                ),
                file(
                    "locales/en.json",
                    json!({"onlyEn": "English", "shared": "EN"}),
                ),
            ],
            &calls,
        );
        let mut runtime = WappI18n::new();
        let mut resolve = |key: &str| runtime.resolve(&deps, "acme.app", key, "zh-CN");
        assert_eq!(text(resolve("onlyExact")), "精确");
        assert_eq!(text(resolve("onlyBase")), "基础");
        assert_eq!(text(resolve("onlyEn")), "English");
        assert_eq!(text(resolve("shared")), "精确共享");
        assert_eq!(resolve("absent"), None);
        assert_eq!(runtime.cached_locale_count(), 3);
        assert_eq!(calls.borrow().len(), 3);
    }

    #[test]
    fn an_empty_string_value_is_a_hit_and_stops_the_fallback() {
        let calls = Calls::default();
        let deps = deps(
            vec![
                file("locales/zh-CN.json", json!({"greet": ""})),
                file("locales/en.json", json!({"greet": "Hello"})),
            ],
            &calls,
        );
        let mut runtime = WappI18n::new();
        assert_eq!(
            text(runtime.resolve(&deps, "acme.app", "greet", "zh-CN")),
            ""
        );
        assert_eq!(calls.borrow().len(), 1);
    }

    #[test]
    fn invalid_key_or_locale_short_circuits_file_reads() {
        let calls = Calls::default();
        let deps = deps(
            vec![file("locales/en.json", json!({"greet": "Hello"}))],
            &calls,
        );
        let mut runtime = WappI18n::new();
        assert_eq!(runtime.resolve(&deps, "acme.app", ".greet", "en"), None);
        assert_eq!(runtime.resolve(&deps, "acme.app", "greet", "en_US"), None);
        assert!(calls.borrow().is_empty());
        assert_eq!(runtime.cached_locale_count(), 0);
    }

    #[test]
    fn missing_and_broken_dictionaries_are_soft_nulls() {
        let cases: Vec<(String, Vec<u8>)> = vec![
            ("locales/aa.json".to_owned(), b"{".to_vec()),
            ("locales/bb.json".to_owned(), b"[]".to_vec()),
            ("locales/cc.json".to_owned(), b"null".to_vec()),
            ("locales/dd.json".to_owned(), b"7".to_vec()),
        ];
        let calls = Calls::default();
        let deps = deps(cases, &calls);
        let mut runtime = WappI18n::new();
        for locale in ["aa", "bb", "cc", "dd"] {
            assert_eq!(runtime.resolve(&deps, "acme.app", "greet", locale), None);
            assert!(runtime.is_cached("acme.app", locale));
        }
        assert_eq!(calls.borrow().len(), 5);
    }

    #[test]
    fn a_failed_file_read_is_cached_as_a_null_dictionary() {
        let calls = Calls::default();
        let deps = deps(vec![], &calls);
        let mut runtime = WappI18n::new();
        assert_eq!(runtime.resolve(&deps, "acme.app", "greet", "de"), None);
        assert_eq!(runtime.resolve(&deps, "acme.app", "greet", "de"), None);
        assert!(runtime.is_cached("acme.app", "de"));
        assert!(runtime.is_cached("acme.app", "en"));
        assert_eq!(calls.borrow().len(), 2);
    }

    #[test]
    fn non_string_values_are_dropped_but_string_values_are_kept() {
        let calls = Calls::default();
        let deps = deps(
            vec![file(
                "locales/en.json",
                json!({"greet": "Hello", "count": 7, "ok": true, "nil": null}),
            )],
            &calls,
        );
        let mut runtime = WappI18n::new();
        assert_eq!(
            text(runtime.resolve(&deps, "acme.app", "greet", "en")),
            "Hello"
        );
        assert_eq!(runtime.resolve(&deps, "acme.app", "count", "en"), None);
        assert_eq!(runtime.resolve(&deps, "acme.app", "nil", "en"), None);
    }

    #[test]
    fn cache_is_per_wapp_and_can_be_reset() {
        let calls = Calls::default();
        let deps = deps(
            vec![(
                "locales/en.json".to_owned(),
                json!({"greet": "A"}).to_string().into_bytes(),
            )],
            &calls,
        );
        let mut runtime = WappI18n::new();
        assert_eq!(text(runtime.resolve(&deps, "a.one", "greet", "en")), "A");
        assert_eq!(text(runtime.resolve(&deps, "a.two", "greet", "en")), "A");
        assert_eq!(calls.borrow().len(), 2);
        assert!(runtime.is_cached("a.one", "en"));
        assert!(runtime.is_cached("a.two", "en"));
        runtime.reset();
        assert_eq!(runtime.cached_locale_count(), 0);
        assert_eq!(
            text(resolve_wapp_string(
                &mut runtime,
                &deps,
                "a.one",
                "greet",
                "en"
            )),
            "A"
        );
        assert_eq!(calls.borrow().len(), 3);
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(WAPP_I18N_GAP.len(), 3);
        assert!(WAPP_I18N_GAP[0].contains("readWappFile"));
        assert_eq!(WAPP_I18N_SURFACE.len(), 8);
        assert!(WAPP_I18N_SURFACE.contains(&"resolveWappString"));
        assert_eq!(crate::WAPP_I18N_GAP.len(), 3);
        assert_eq!(crate::WAPP_I18N_SURFACE.len(), 8);
        let _: Option<crate::WappI18n> = Some(WappI18n::new());
        let _: Option<crate::WappDictionary> = Some(WappDictionary::new());
    }
}
