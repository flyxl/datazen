use std::collections::{BTreeMap, BTreeSet};

pub const BUILTIN_LOCALES: [&str; 2] = ["en", "zh-CN"];

pub fn is_builtin_locale(locale: &str) -> bool {
    BUILTIN_LOCALES.iter().any(|builtin| *builtin == locale)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LanguagePackMessages {
    pub code: String,
    pub label: String,
    pub messages: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LanguagePackRuntime {
    pub registered: BTreeSet<String>,
    pub registered_calls: Vec<LanguagePackMessages>,
    pub unregistered_translations: Vec<String>,
    pub unregistered_locales: Vec<String>,
}

pub struct LanguagePackDeps {
    pub read_language_pack: Box<dyn Fn(&str) -> Result<LanguagePackMessages, String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnsureOutcome {
    Empty,
    Builtin,
    AlreadyRegistered,
    ReadFailed,
    Registered,
}

pub fn ensure_language_pack_messages(
    locale: &str,
    runtime: &mut LanguagePackRuntime,
    deps: &LanguagePackDeps,
) -> EnsureOutcome {
    if locale.is_empty() || is_builtin_locale(locale) || runtime.registered.contains(locale) {
        if locale.is_empty() {
            return EnsureOutcome::Empty;
        }
        if is_builtin_locale(locale) {
            return EnsureOutcome::Builtin;
        }
        return EnsureOutcome::AlreadyRegistered;
    }
    let pack = match (deps.read_language_pack)(locale) {
        Ok(pack) => pack,
        Err(_) => return EnsureOutcome::ReadFailed,
    };
    runtime.registered_calls.push(pack.clone());
    runtime.registered.insert(pack.code.clone());
    EnsureOutcome::Registered
}

pub fn unload_language_pack(code: &str, runtime: &mut LanguagePackRuntime) {
    runtime.unregistered_translations.push(code.to_owned());
    runtime.unregistered_locales.push(code.to_owned());
    runtime.registered.remove(code);
}

pub fn reset_language_pack_runtime_for_tests(runtime: &mut LanguagePackRuntime) {
    runtime.registered.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn pack(code: &str) -> LanguagePackMessages {
        let mut messages = BTreeMap::new();
        messages.insert("common.ok".to_owned(), format!("OK-{code}"));
        LanguagePackMessages {
            code: code.to_owned(),
            label: format!("Label {code}"),
            messages,
        }
    }

    fn deps(result: Result<LanguagePackMessages, String>) -> LanguagePackDeps {
        LanguagePackDeps {
            read_language_pack: Box::new(move |_| result.clone()),
        }
    }

    #[test]
    fn builtin_locale_set_is_exact_and_case_sensitive() {
        assert_eq!(BUILTIN_LOCALES, ["en", "zh-CN"]);
        assert!(is_builtin_locale("en"));
        assert!(is_builtin_locale("zh-CN"));
        assert!(!is_builtin_locale("zh-cn"));
        assert!(!is_builtin_locale("ja"));
    }

    #[test]
    fn empty_builtin_and_already_registered_locales_never_read() {
        let reads = Rc::new(RefCell::new(Vec::new()));
        let seen = reads.clone();
        let deps = LanguagePackDeps {
            read_language_pack: Box::new(move |code| {
                seen.borrow_mut().push(code.to_owned());
                Ok(pack(code))
            }),
        };
        let mut runtime = LanguagePackRuntime::default();
        assert_eq!(ensure_language_pack_messages("", &mut runtime, &deps), EnsureOutcome::Empty);
        assert_eq!(ensure_language_pack_messages("en", &mut runtime, &deps), EnsureOutcome::Builtin);
        assert_eq!(ensure_language_pack_messages("zh-CN", &mut runtime, &deps), EnsureOutcome::Builtin);
        runtime.registered.insert("ja".to_owned());
        assert_eq!(ensure_language_pack_messages("ja", &mut runtime, &deps), EnsureOutcome::AlreadyRegistered);
        assert!(reads.borrow().is_empty());
    }

    #[test]
    fn an_installed_pack_is_registered_with_its_response_fields() {
        let mut runtime = LanguagePackRuntime::default();
        let outcome = ensure_language_pack_messages("ja", &mut runtime, &deps(Ok(pack("ja"))));
        assert_eq!(outcome, EnsureOutcome::Registered);
        assert_eq!(runtime.registered_calls, vec![pack("ja")]);
        assert!(runtime.registered.contains("ja"));
    }

    #[test]
    fn registration_uses_the_pack_code_not_the_requested_locale() {
        let mut runtime = LanguagePackRuntime::default();
        let outcome = ensure_language_pack_messages("jp", &mut runtime, &deps(Ok(pack("ja"))));
        assert_eq!(outcome, EnsureOutcome::Registered);
        assert!(runtime.registered.contains("ja"));
        assert!(!runtime.registered.contains("jp"));
        assert_eq!(ensure_language_pack_messages("jp", &mut runtime, &deps(Ok(pack("ja")))), EnsureOutcome::Registered);
    }

    #[test]
    fn a_failed_read_is_swallowed_and_registers_nothing() {
        let mut runtime = LanguagePackRuntime::default();
        let outcome = ensure_language_pack_messages("de", &mut runtime, &deps(Err("not installed".to_owned())));
        assert_eq!(outcome, EnsureOutcome::ReadFailed);
        assert!(runtime.registered.is_empty());
        assert!(runtime.registered_calls.is_empty());
        assert_eq!(ensure_language_pack_messages("de", &mut runtime, &deps(Ok(pack("de")))), EnsureOutcome::Registered);
    }

    #[test]
    fn unload_drops_both_registries_even_for_an_unknown_code() {
        let mut runtime = LanguagePackRuntime::default();
        ensure_language_pack_messages("ja", &mut runtime, &deps(Ok(pack("ja"))));
        unload_language_pack("ja", &mut runtime);
        assert!(runtime.registered.is_empty());
        assert_eq!(runtime.unregistered_translations, vec!["ja"]);
        assert_eq!(runtime.unregistered_locales, vec!["ja"]);
        unload_language_pack("never-installed", &mut runtime);
        assert_eq!(runtime.unregistered_locales, vec!["ja", "never-installed"]);
    }

    #[test]
    fn reset_forgets_registrations_but_keeps_past_effects() {
        let mut runtime = LanguagePackRuntime::default();
        ensure_language_pack_messages("ja", &mut runtime, &deps(Ok(pack("ja"))));
        reset_language_pack_runtime_for_tests(&mut runtime);
        assert!(runtime.registered.is_empty());
        assert_eq!(runtime.registered_calls.len(), 1);
        assert_eq!(ensure_language_pack_messages("ja", &mut runtime, &deps(Ok(pack("ja")))), EnsureOutcome::Registered);
    }

    #[test]
    fn the_requested_code_is_passed_to_the_reader() {
        let reads = Rc::new(RefCell::new(Vec::new()));
        let seen = reads.clone();
        let deps = LanguagePackDeps {
            read_language_pack: Box::new(move |code| {
                seen.borrow_mut().push(code.to_owned());
                Ok(pack(code))
            }),
        };
        let mut runtime = LanguagePackRuntime::default();
        ensure_language_pack_messages("pt-BR", &mut runtime, &deps);
        assert_eq!(reads.borrow().as_slice(), &["pt-BR"]);
    }
}
