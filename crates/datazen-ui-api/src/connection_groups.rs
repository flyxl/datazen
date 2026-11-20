use crate::Translate;

pub const PRESET_PRODUCTION: &str = "preset:production";
pub const PRESET_DEVELOPMENT: &str = "preset:development";
pub const PRESET_TESTING: &str = "preset:testing";

pub const KEY_GROUP_PROD: &str = "newConn.groupProd";
pub const KEY_GROUP_DEV: &str = "newConn.groupDev";
pub const KEY_GROUP_TEST: &str = "newConn.groupTest";

pub const PRESET_GROUP_OPTIONS: [(&str, &str); 3] = [
    (PRESET_PRODUCTION, KEY_GROUP_PROD),
    (PRESET_DEVELOPMENT, KEY_GROUP_DEV),
    (PRESET_TESTING, KEY_GROUP_TEST),
];

pub fn format_group_label(key: &str, t: &dyn Translate) -> String {
    match PRESET_GROUP_OPTIONS
        .iter()
        .find(|(preset, _)| *preset == key)
    {
        Some((_, i18n_key)) => t.translate(i18n_key, &[]),
        None => key.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn echo(key: &str, _params: &[(&str, String)]) -> String {
        key.to_owned()
    }

    struct Recorder<'a> {
        calls: &'a RefCell<Vec<String>>,
    }

    impl Translate for Recorder<'_> {
        fn translate(&self, key: &str, params: &[(&str, String)]) -> String {
            assert!(params.is_empty());
            self.calls.borrow_mut().push(key.to_owned());
            format!("label({key})")
        }
    }

    #[test]
    fn preset_keys_are_locale_independent_literals() {
        assert_eq!(PRESET_PRODUCTION, "preset:production");
        assert_eq!(PRESET_DEVELOPMENT, "preset:development");
        assert_eq!(PRESET_TESTING, "preset:testing");
        assert_eq!(PRESET_GROUP_OPTIONS.len(), 3);
        assert_eq!(PRESET_GROUP_OPTIONS[0], (PRESET_PRODUCTION, KEY_GROUP_PROD));
        assert_eq!(PRESET_GROUP_OPTIONS[1], (PRESET_DEVELOPMENT, KEY_GROUP_DEV));
        assert_eq!(PRESET_GROUP_OPTIONS[2], (PRESET_TESTING, KEY_GROUP_TEST));
    }

    #[test]
    fn preset_keys_resolve_to_their_i18n_keys() {
        assert_eq!(format_group_label(PRESET_PRODUCTION, &echo), KEY_GROUP_PROD);
        assert_eq!(format_group_label(PRESET_DEVELOPMENT, &echo), KEY_GROUP_DEV);
        assert_eq!(format_group_label(PRESET_TESTING, &echo), KEY_GROUP_TEST);
    }

    #[test]
    fn custom_and_empty_keys_are_returned_verbatim() {
        assert_eq!(format_group_label("my-team", &echo), "my-team");
        assert_eq!(format_group_label("", &echo), "");
        assert_eq!(
            format_group_label("preset:unknown", &echo),
            "preset:unknown"
        );
    }

    #[test]
    fn translation_receives_no_params_for_presets_only() {
        let calls = RefCell::new(Vec::new());
        let translator = Recorder { calls: &calls };
        assert_eq!(
            format_group_label(PRESET_PRODUCTION, &translator as &dyn Translate),
            format!("label({KEY_GROUP_PROD})")
        );
        assert_eq!(calls.borrow().as_slice(), &[KEY_GROUP_PROD.to_owned()]);
        assert_eq!(format_group_label("custom", &translator), "custom");
        assert_eq!(calls.borrow().len(), 1);
    }
}
