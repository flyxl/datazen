use crate::FkTextKind;

pub const FK_SELECTOR_PICK_FROM_KEY: &str = "fkSelector.pickFrom";

pub trait FkSelectorTranslator {
    fn translate(&self, key: &str, target: &str) -> String;
}

pub fn fk_selector_label(translator: &dyn FkSelectorTranslator, target_label: &str) -> String {
    translator.translate(FK_SELECTOR_PICK_FROM_KEY, target_label)
}

pub fn fk_text_with_selector<'a>(
    translator: &'a dyn FkSelectorTranslator,
    fallback: &'a dyn Fn(FkTextKind, &str) -> String,
) -> impl Fn(FkTextKind, &str) -> String + 'a {
    move |kind, target| {
        if matches!(kind, FkTextKind::Selector) {
            fk_selector_label(translator, target)
        } else {
            fallback(kind, target)
        }
    }
}
