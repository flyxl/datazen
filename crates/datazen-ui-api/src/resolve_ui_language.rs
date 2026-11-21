use crate::BUILTIN_LOCALES;

pub const SUPPORTED_UI_LANGUAGES: [&str; 2] = BUILTIN_LOCALES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupportedUiLanguage {
    En,
    ZhCn,
}

impl SupportedUiLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
        }
    }
}

fn normalize_locale_tag(locale: &str) -> String {
    locale.trim().replace('_', "-")
}

fn resolve_builtin_ui_language(normalized: &str) -> Option<SupportedUiLanguage> {
    if normalized.is_empty() {
        return None;
    }
    for supported in SUPPORTED_UI_LANGUAGES {
        if normalized.eq_ignore_ascii_case(supported) {
            return match supported {
                "en" => Some(SupportedUiLanguage::En),
                "zh-CN" => Some(SupportedUiLanguage::ZhCn),
                _ => None,
            };
        }
    }
    let lower = normalized.to_lowercase();
    if lower.starts_with("zh") {
        return Some(SupportedUiLanguage::ZhCn);
    }
    if lower.starts_with("en") {
        return Some(SupportedUiLanguage::En);
    }
    None
}

pub fn resolve_ui_language(system_locale: &str) -> SupportedUiLanguage {
    resolve_builtin_ui_language(&normalize_locale_tag(system_locale))
        .unwrap_or(SupportedUiLanguage::En)
}

pub fn normalize_settings_language(language: &str) -> String {
    let normalized = normalize_locale_tag(language);
    if normalized.is_empty() {
        return "en".to_owned();
    }
    resolve_builtin_ui_language(&normalized).map_or(normalized, |value| value.as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_languages_match_the_builtin_locale_table() {
        assert_eq!(SUPPORTED_UI_LANGUAGES, ["en", "zh-CN"]);
        assert_eq!(SupportedUiLanguage::En.as_str(), "en");
        assert_eq!(SupportedUiLanguage::ZhCn.as_str(), "zh-CN");
    }

    #[test]
    fn system_locale_maps_exact_prefix_and_fallback() {
        assert_eq!(resolve_ui_language(""), SupportedUiLanguage::En);
        assert_eq!(resolve_ui_language("   "), SupportedUiLanguage::En);
        assert_eq!(resolve_ui_language("en"), SupportedUiLanguage::En);
        assert_eq!(resolve_ui_language("EN"), SupportedUiLanguage::En);
        assert_eq!(resolve_ui_language("en_US"), SupportedUiLanguage::En);
        assert_eq!(resolve_ui_language("  en-GB  "), SupportedUiLanguage::En);
        assert_eq!(resolve_ui_language("zh-CN"), SupportedUiLanguage::ZhCn);
        assert_eq!(resolve_ui_language("zh_CN"), SupportedUiLanguage::ZhCn);
        assert_eq!(resolve_ui_language(" zh-Hans "), SupportedUiLanguage::ZhCn);
        assert_eq!(resolve_ui_language("zh-TW"), SupportedUiLanguage::ZhCn);
        assert_eq!(resolve_ui_language("ja-JP"), SupportedUiLanguage::En);
    }

    #[test]
    fn settings_language_canonicalizes_builtins_and_preserves_packs() {
        assert_eq!(normalize_settings_language(""), "en");
        assert_eq!(normalize_settings_language("   "), "en");
        assert_eq!(normalize_settings_language("en-US"), "en");
        assert_eq!(normalize_settings_language("  EN_US "), "en");
        assert_eq!(normalize_settings_language("zh"), "zh-CN");
        assert_eq!(normalize_settings_language("zh_CN"), "zh-CN");
        assert_eq!(normalize_settings_language("zh-TW"), "zh-CN");
        assert_eq!(normalize_settings_language(" ja_JP "), "ja-JP");
        assert_eq!(normalize_settings_language("fr-FR"), "fr-FR");
    }
}
