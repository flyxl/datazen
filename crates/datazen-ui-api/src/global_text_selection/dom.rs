use crate::DomElement;

pub const NATIVE_SELECTABLE_SELECTOR: &str = "input, textarea, select, [contenteditable=\"true\"], [contenteditable=\"plaintext-only\"], .cm-editor";
pub const INTERACTIVE_CONTROL_SELECTOR: &str = "button, [role=\"button\"], [role=\"menuitem\"], [role=\"menuitemcheckbox\"], [role=\"menuitemradio\"], [role=\"tab\"], [role=\"checkbox\"], [role=\"radio\"], [role=\"switch\"], [role=\"slider\"], [role=\"option\"], [role=\"combobox\"]";
pub const SELECTABLE_HINT_SELECTOR: &str = ".selectable, .select-text, .copyable, .error-message, [role=\"alert\"], [data-error], [data-testid*=\"error\"], [aria-invalid=\"true\"], pre, code";

const NATIVE_TAGS: [&str; 3] = ["input", "textarea", "select"];
const INTERACTIVE_ROLES: [&str; 11] = [
    "button", "menuitem", "menuitemcheckbox", "menuitemradio", "tab", "checkbox",
    "radio", "switch", "slider", "option", "combobox",
];
const SELECTABLE_CLASSES: [&str; 4] = ["selectable", "select-text", "copyable", "error-message"];
const SELECTABLE_TAGS: [&str; 2] = ["pre", "code"];

pub fn tag(element: &DomElement) -> &str {
    element.attributes.get("tag").map(String::as_str).unwrap_or("")
}

pub fn attr<'a>(element: &'a DomElement, key: &str) -> Option<&'a str> {
    element.attributes.get(key).map(String::as_str)
}

pub fn is_body(element: &DomElement) -> bool {
    tag(element) == "body" || attr(element, "body") == Some("true")
}

fn has_class(element: &DomElement, name: &str) -> bool {
    attr(element, "class").is_some_and(|classes| classes.split_whitespace().any(|value| value == name))
}

fn matches_native_selectable(element: &DomElement) -> bool {
    NATIVE_TAGS.contains(&tag(element))
        || matches!(attr(element, "contenteditable"), Some("true" | "plaintext-only"))
        || has_class(element, "cm-editor")
}

fn matches_interactive_control(element: &DomElement) -> bool {
    tag(element) == "button"
        || attr(element, "role").is_some_and(|role| INTERACTIVE_ROLES.contains(&role))
}

fn matches_selectable_hint(element: &DomElement) -> bool {
    SELECTABLE_CLASSES.iter().any(|class| has_class(element, class))
        || attr(element, "role") == Some("alert")
        || attr(element, "data-error").is_some()
        || attr(element, "data-testid").is_some_and(|value| value.contains("error"))
        || attr(element, "aria-invalid") == Some("true")
        || SELECTABLE_TAGS.contains(&tag(element))
}

fn closest_with(element: &DomElement, predicate: fn(&DomElement) -> bool) -> bool {
    let mut current = Some(element);
    while let Some(node) = current {
        if predicate(node) {
            return true;
        }
        current = node.parent.as_deref();
    }
    false
}

pub fn is_native_selectable(element: &DomElement) -> bool {
    closest_with(element, matches_native_selectable)
}

pub fn is_interactive_control(element: &DomElement) -> bool {
    closest_with(element, matches_interactive_control)
}

pub fn is_selectable_hint(element: &DomElement) -> bool {
    closest_with(element, matches_selectable_hint)
}

pub fn is_selectable(element: Option<&DomElement>) -> bool {
    let Some(element) = element else {
        return false;
    };
    if is_interactive_control(element) {
        return false;
    }
    if is_selectable_hint(element) {
        return true;
    }
    let mut current = Some(element);
    while let Some(node) = current {
        if attr(node, "userSelect") == Some("text") {
            return true;
        }
        if is_body(node) {
            break;
        }
        current = node.parent.as_deref();
    }
    false
}
