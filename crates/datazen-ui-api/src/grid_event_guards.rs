use crate::DomElement;

pub const EDITABLE_TARGET_SELECTOR: &str =
    "input, textarea, select, [contenteditable=\"true\"], [data-dt-editing=\"true\"]";

fn tag(element: &DomElement) -> &str {
    element.attributes.get("tag").map(String::as_str).unwrap_or("")
}

fn matches_editable(element: &DomElement) -> bool {
    matches!(tag(element), "input" | "textarea" | "select")
        || element.attributes.get("contenteditable").map(String::as_str) == Some("true")
        || element.attributes.get("data-dt-editing").map(String::as_str) == Some("true")
}

pub fn is_editable_target(target: Option<&DomElement>) -> bool {
    let Some(target) = target else {
        return false;
    };
    let mut current = Some(target);
    while let Some(element) = current {
        if matches_editable(element) {
            return true;
        }
        current = element.parent.as_deref();
    }
    false
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComposingEventLike {
    pub is_composing: Option<bool>,
    pub key_code: Option<i32>,
    pub native_event: Option<Box<ComposingEventLike>>,
}

pub fn is_composing_event(event: &ComposingEventLike) -> bool {
    if event.is_composing == Some(true) {
        return true;
    }
    if event.key_code == Some(229) {
        return true;
    }
    let Some(native) = event.native_event.as_ref() else {
        return false;
    };
    native.is_composing == Some(true) || native.key_code == Some(229)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(attrs: Vec<(&str, &str)>) -> DomElement {
        DomElement::leaf(attrs)
    }

    #[test]
    fn selector_constant_is_pinned() {
        assert_eq!(
            EDITABLE_TARGET_SELECTOR,
            "input, textarea, select, [contenteditable=\"true\"], [data-dt-editing=\"true\"]"
        );
    }

    #[test]
    fn non_element_targets_are_not_editable() {
        assert!(!is_editable_target(None));
    }

    #[test]
    fn native_editable_tags_and_attributes_are_recognized() {
        for attrs in [
            vec![("tag", "input")],
            vec![("tag", "textarea")],
            vec![("tag", "select")],
            vec![("contenteditable", "true")],
            vec![("data-dt-editing", "true")],
        ] {
            assert!(is_editable_target(Some(&el(attrs))));
        }
    }

    #[test]
    fn false_attributes_and_plain_elements_are_not_editable() {
        assert!(!is_editable_target(Some(&el(vec![("tag", "div")]))));
        assert!(!is_editable_target(Some(&el(vec![("contenteditable", "false")]))));
        assert!(!is_editable_target(Some(&el(vec![("data-dt-editing", "false")]))));
    }

    #[test]
    fn editable_target_matches_through_the_ancestor_chain() {
        let editor = el(vec![("data-dt-editing", "true")]);
        let padding = DomElement::child_of(vec![("tag", "div")], editor);
        let icon = DomElement::child_of(vec![("tag", "span")], padding);
        assert!(is_editable_target(Some(&icon)));
    }

    #[test]
    fn composing_detection_checks_both_signals() {
        assert!(!is_composing_event(&ComposingEventLike::default()));
        assert!(is_composing_event(&ComposingEventLike {
            is_composing: Some(true),
            ..Default::default()
        }));
        assert!(is_composing_event(&ComposingEventLike {
            key_code: Some(229),
            ..Default::default()
        }));
        assert!(!is_composing_event(&ComposingEventLike {
            key_code: Some(228),
            ..Default::default()
        }));
        assert!(!is_composing_event(&ComposingEventLike {
            is_composing: Some(false),
            key_code: Some(65),
            ..Default::default()
        }));
    }

    #[test]
    fn composing_detection_checks_one_native_level() {
        let native = ComposingEventLike {
            is_composing: Some(true),
            ..Default::default()
        };
        let synthetic = ComposingEventLike {
            native_event: Some(Box::new(native)),
            ..Default::default()
        };
        assert!(is_composing_event(&synthetic));

        let native = ComposingEventLike {
            key_code: Some(229),
            ..Default::default()
        };
        let synthetic = ComposingEventLike {
            native_event: Some(Box::new(native)),
            ..Default::default()
        };
        assert!(is_composing_event(&synthetic));

        let plain_native = ComposingEventLike {
            key_code: Some(65),
            ..Default::default()
        };
        let synthetic = ComposingEventLike {
            native_event: Some(Box::new(plain_native)),
            ..Default::default()
        };
        assert!(!is_composing_event(&synthetic));

        let grandparent = ComposingEventLike {
            is_composing: Some(true),
            ..Default::default()
        };
        let native = ComposingEventLike {
            native_event: Some(Box::new(grandparent)),
            ..Default::default()
        };
        let synthetic = ComposingEventLike {
            native_event: Some(Box::new(native)),
            ..Default::default()
        };
        assert!(!is_composing_event(&synthetic), "only one native level is unwrapped");
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::is_editable_target;
        let _ = crate::is_composing_event;
        let _: Option<crate::ComposingEventLike> = None;
        assert!(crate::EDITABLE_TARGET_SELECTOR.contains("data-dt-editing"));
    }
}
