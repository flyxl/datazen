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
#[ignore = "DEFECT dfbe30895: placeholder bodies splice a Chinese section label into an English sentence"]
fn placeholder_bodies_stay_in_one_language() {
    // `settings.rs` renders `"{label} settings are not available in this Rune
    // build yet."`. Since `label` is now `t(...)`, a not-yet-migrated section
    // (logging, behaviour, AI, prompts, MCP, tunnels, extensions) shows
    // 「日志 settings are not available in this Rune build yet.」. Either
    // translate the template or keep the label in English; do not mix.
    let body = settings_texts("logging")
        .into_iter()
        .find(|text| text.contains("settings are not available"))
        .expect("the logging section shows its placeholder body");
    let english_template = body.contains("settings are not available in this Rune build yet.");
    let has_han = body.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
    assert!(
        !(english_template && has_han),
        "the placeholder body mixes a translated label into an English template: {body:?}"
    );
}
