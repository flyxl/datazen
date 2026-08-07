//! Map OS locale strings to supported UI language codes.

const SUPPORTED: &[&str] = &[
    "en", "zh-CN", "zh-TW", "es", "fr", "de", "ja", "pt-BR", "ru", "ko",
];

/// Returns the best supported UI language for a system locale tag.
pub fn resolve_ui_language(system_locale: &str) -> &'static str {
    let normalized = system_locale.trim().replace('_', "-");
    if normalized.is_empty() {
        return "en";
    }

    for supported in SUPPORTED {
        if normalized.eq_ignore_ascii_case(supported) {
            return supported;
        }
    }

    let lower = normalized.to_lowercase();

    if lower.starts_with("zh-hans") || lower.starts_with("zh-cn") {
        return "zh-CN";
    }
    if lower.starts_with("zh-hant")
        || lower.starts_with("zh-tw")
        || lower.starts_with("zh-hk")
        || lower.starts_with("zh-mo")
    {
        return "zh-TW";
    }
    if lower.starts_with("zh") {
        return "zh-CN";
    }

    if lower == "pt" || lower.starts_with("pt-") {
        return "pt-BR";
    }

    if lower.starts_with("en") {
        return "en";
    }
    if lower.starts_with("es") {
        return "es";
    }
    if lower.starts_with("fr") {
        return "fr";
    }
    if lower.starts_with("de") {
        return "de";
    }
    if lower.starts_with("ja") {
        return "ja";
    }
    if lower.starts_with("ko") {
        return "ko";
    }
    if lower.starts_with("ru") {
        return "ru";
    }

    "en"
}

/// Reads the OS locale and maps it to a supported UI language.
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
    fn chinese_traditional() {
        assert_eq!(resolve_ui_language("zh-TW"), "zh-TW");
        assert_eq!(resolve_ui_language("zh-Hant"), "zh-TW");
        assert_eq!(resolve_ui_language("zh-Hant-TW"), "zh-TW");
        assert_eq!(resolve_ui_language("zh-HK"), "zh-TW");
        assert_eq!(resolve_ui_language("zh-MO"), "zh-TW");
    }

    #[test]
    fn european_languages() {
        assert_eq!(resolve_ui_language("es"), "es");
        assert_eq!(resolve_ui_language("es-ES"), "es");
        assert_eq!(resolve_ui_language("es-MX"), "es");
        assert_eq!(resolve_ui_language("fr"), "fr");
        assert_eq!(resolve_ui_language("fr-FR"), "fr");
        assert_eq!(resolve_ui_language("fr-CA"), "fr");
        assert_eq!(resolve_ui_language("de"), "de");
        assert_eq!(resolve_ui_language("de-DE"), "de");
        assert_eq!(resolve_ui_language("de-AT"), "de");
    }

    #[test]
    fn asian_languages() {
        assert_eq!(resolve_ui_language("ja"), "ja");
        assert_eq!(resolve_ui_language("ja-JP"), "ja");
        assert_eq!(resolve_ui_language("ko"), "ko");
        assert_eq!(resolve_ui_language("ko-KR"), "ko");
        assert_eq!(resolve_ui_language("ru"), "ru");
        assert_eq!(resolve_ui_language("ru-RU"), "ru");
    }

    #[test]
    fn portuguese_maps_to_brazilian() {
        assert_eq!(resolve_ui_language("pt"), "pt-BR");
        assert_eq!(resolve_ui_language("pt-BR"), "pt-BR");
        assert_eq!(resolve_ui_language("pt_BR"), "pt-BR");
        assert_eq!(resolve_ui_language("pt-PT"), "pt-BR");
    }

    #[test]
    fn exact_supported_codes_are_preserved() {
        for code in super::SUPPORTED {
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
        assert_eq!(resolve_ui_language("Pt-Br"), "pt-BR");
        assert_eq!(resolve_ui_language("KO"), "ko");
    }

    #[test]
    fn more_region_variants() {
        assert_eq!(resolve_ui_language("es-AR"), "es");
        assert_eq!(resolve_ui_language("fr_BE"), "fr");
        assert_eq!(resolve_ui_language("de_CH"), "de");
        assert_eq!(resolve_ui_language("en-IN"), "en");
        assert_eq!(resolve_ui_language("zh-SG"), "zh-CN");
    }

    #[test]
    fn default_ui_language_is_supported() {
        let lang = super::default_ui_language();
        assert!(
            super::SUPPORTED.contains(&lang.as_str()),
            "unexpected default language: {lang}"
        );
    }
}
