//! Map OS locale strings to supported UI language codes.

const BUILTIN: &[&str] = &["en", "zh-CN"];

/// Returns the best built-in UI language for a system locale tag.
/// Extension locale packs are handled on the frontend side.
pub fn resolve_ui_language(system_locale: &str) -> &'static str {
    let normalized = system_locale.trim().replace('_', "-");
    if normalized.is_empty() {
        return "en";
    }

    for builtin in BUILTIN {
        if normalized.eq_ignore_ascii_case(builtin) {
            return builtin;
        }
    }

    let lower = normalized.to_lowercase();

    if lower.starts_with("zh-hans") || lower.starts_with("zh-cn") || lower.starts_with("zh") {
        return "zh-CN";
    }

    "en"
}

/// Reads the OS locale and maps it to a built-in UI language.
pub fn default_ui_language() -> String {
    resolve_ui_language(&system_locale()).to_string()
}

fn system_locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en".to_string())
}

#[cfg(test)]
mod tests {
    use super::resolve_ui_language;

    #[test]
    fn empty_or_unknown_falls_back_to_en() {
        assert_eq!(resolve_ui_language(""), "en");
        assert_eq!(resolve_ui_language("   "), "en");
        assert_eq!(resolve_ui_language("xx"), "en");
        assert_eq!(resolve_ui_language("it-IT"), "en");
        assert_eq!(resolve_ui_language("nl-NL"), "en");
    }

    #[test]
    fn english_variants() {
        assert_eq!(resolve_ui_language("en"), "en");
        assert_eq!(resolve_ui_language("en-US"), "en");
        assert_eq!(resolve_ui_language("en_GB"), "en");
        assert_eq!(resolve_ui_language("EN-au"), "en");
    }

    #[test]
    fn chinese_simplified() {
        assert_eq!(resolve_ui_language("zh-CN"), "zh-CN");
        assert_eq!(resolve_ui_language("zh_CN"), "zh-CN");
        assert_eq!(resolve_ui_language("zh-Hans"), "zh-CN");
        assert_eq!(resolve_ui_language("zh-Hans-CN"), "zh-CN");
        assert_eq!(resolve_ui_language("zh"), "zh-CN");
    }

    #[test]
    fn non_builtin_languages_fall_back_to_en() {
        assert_eq!(resolve_ui_language("es"), "en");
        assert_eq!(resolve_ui_language("fr"), "en");
        assert_eq!(resolve_ui_language("de"), "en");
        assert_eq!(resolve_ui_language("ja"), "en");
        assert_eq!(resolve_ui_language("ko"), "en");
        assert_eq!(resolve_ui_language("ru"), "en");
        assert_eq!(resolve_ui_language("pt-BR"), "en");
    }

    #[test]
    fn chinese_variants_map_to_zh_cn() {
        assert_eq!(resolve_ui_language("zh-TW"), "zh-CN");
        assert_eq!(resolve_ui_language("zh-Hant"), "zh-CN");
        assert_eq!(resolve_ui_language("zh-HK"), "zh-CN");
    }

    #[test]
    fn exact_builtin_codes_are_preserved() {
        for code in super::BUILTIN {
            assert_eq!(resolve_ui_language(code), *code);
        }
    }

    #[test]
    fn trims_whitespace_around_locale() {
        assert_eq!(resolve_ui_language("  en-US  "), "en");
        assert_eq!(resolve_ui_language("\tzh-CN\n"), "zh-CN");
    }

    #[test]
    fn case_insensitive_exact_match() {
        assert_eq!(resolve_ui_language("EN"), "en");
        assert_eq!(resolve_ui_language("ZH-CN"), "zh-CN");
    }

    #[test]
    fn default_ui_language_is_builtin() {
        let lang = super::default_ui_language();
        assert!(
            super::BUILTIN.contains(&lang.as_str()),
            "unexpected default language: {lang}"
        );
    }
}
