//! Independent audit of the settings/rail i18n batch (`dfbe30895`).
//!
//! The dependency-free translation table only has a zh-CN pack, so migrating a
//! view necessarily renders Chinese. These checks care about what the page
//! actually shows: no raw keys, and no sentence that has been half translated
//! by splicing a translated label into an untranslated template.

use crate::types::Section;

fn settings_texts(section: &str) -> Vec<String> {
    let mut runtime = super::runtime();
    runtime.state.active = Section::Settings;
    runtime.state.set_settings_section(section);
    runtime.frame(0.0);
    runtime
        .elements()
        .into_iter()
        .filter_map(|element| element.text)
        .collect()
}

/// The texts laid out under `settings-section-<id>`, read off the real layout.
fn section_texts(section: &str) -> Vec<String> {
    let mut runtime = super::runtime();
    runtime.state.active = Section::Settings;
    runtime.state.set_settings_section(section);
    runtime.frame(0.0);
    let elements = runtime.elements();
    let path = elements
        .iter()
        .find(|element| element.key.as_deref() == Some(&format!("settings-section-{section}")))
        .unwrap_or_else(|| panic!("{section}: the settings section is laid out"))
        .path
        .clone();
    elements
        .into_iter()
        .filter(|element| element.path.starts_with(&path))
        .filter_map(|element| element.text)
        .collect()
}

#[test]
fn the_settings_page_does_not_show_raw_translation_keys() {
    // Every section in the nav should render its label from the pack. A key
    // that is missing (or misspelt) falls back to the key itself, which is the
    // first thing a botched migration puts on screen.
    for section in crate::settings_state::SETTINGS_SECTIONS
        .iter()
        .map(|section| section.id)
        .chain(["general", "logging"])
    {
        let texts = settings_texts(section);
        assert!(
            texts.iter().all(|text| !text.starts_with("settings.")
                && !text.starts_with("common.")
                && !text.starts_with("nav.")),
            "section {section:?} shows a raw key: {texts:?}"
        );
    }
}

#[test]
fn placeholder_sections_speak_one_chinese_sentence() {
    // `settings.rs` renders `"{label}设置在此 Rune 构建中尚不可用"`. The label
    // is `t(...)`, so the whole body has to stay Chinese: a half-translated
    // template shows up here as an English fragment or a raw key.
    for section in [
        "behavior",
        "logging",
        "ai",
        "prompts",
        "mcpServer",
        "mcpClient",
        "tunnels",
        "extensions",
    ] {
        let texts = section_texts(section);
        let title = texts
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("{section}: the section shows a title"));
        assert!(
            !texts
                .iter()
                .any(|text| text.contains("settings are not available")),
            "{section}: an English template survived the migration: {texts:?}"
        );
        let body = texts
            .iter()
            .find(|text| text.ends_with("尚不可用"))
            .unwrap_or_else(|| panic!("{section}: no Chinese placeholder body in {texts:?}"));
        assert!(
            body.starts_with(&title),
            "{section}: body {body:?} does not start with its translated title {title:?}"
        );
        assert!(
            body.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "{section}: the placeholder body is not Chinese: {body:?}"
        );
        assert!(
            !texts.iter().any(|text| text.contains("settings.")
                || text.contains("newConn.")
                || text.contains("common.")
                || text.contains("nav.")),
            "{section}: a raw key fell through to the page: {texts:?}"
        );
    }
}
