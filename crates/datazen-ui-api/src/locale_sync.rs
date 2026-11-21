use std::cell::RefCell;
use std::rc::Rc;

pub const DEFAULT_LOCALE: &str = "en";

type LanguageListener = Rc<RefCell<dyn FnMut(String, Option<String>)>>;

#[derive(Clone)]
pub struct SettingsLanguageStore {
    language: Rc<RefCell<Option<String>>>,
    listeners: Rc<RefCell<Vec<LanguageListener>>>,
}

impl SettingsLanguageStore {
    pub fn new(language: Option<String>) -> Self {
        Self {
            language: Rc::new(RefCell::new(language)),
            listeners: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn language(&self) -> Option<String> {
        self.language.borrow().clone()
    }

    pub fn current_language(&self) -> String {
        self.language().unwrap_or_else(|| DEFAULT_LOCALE.to_owned())
    }

    pub fn set_language(&self, next: Option<String>) {
        let previous = self.language.borrow().clone();
        *self.language.borrow_mut() = next.clone();
        let normalized = next.unwrap_or_else(|| DEFAULT_LOCALE.to_owned());
        let listeners = self.listeners.borrow().clone();
        for listener in listeners {
            (listener.borrow_mut())(normalized.clone(), previous.clone());
        }
    }

    pub fn subscribe(&self, listener: LanguageListener) -> Box<dyn Fn()> {
        self.listeners.borrow_mut().push(listener.clone());
        let listeners = self.listeners.clone();
        Box::new(move || {
            let mut current = listeners.borrow_mut();
            if let Some(index) = current.iter().position(|item| Rc::ptr_eq(item, &listener)) {
                current.remove(index);
            }
        })
    }
}

pub struct LocaleEnginePorts {
    pub is_builtin_locale: Rc<dyn Fn(&str) -> bool>,
    pub ensure_pack_messages: Rc<RefCell<dyn FnMut(&str) -> Result<(), String>>>,
    pub set_locale: Rc<RefCell<dyn FnMut(&str)>>,
}

pub type Deferred = Box<dyn FnOnce()>;
pub type Defer = Rc<dyn Fn(Deferred)>;

pub fn current_language(store: &SettingsLanguageStore) -> String {
    store.current_language()
}

pub fn apply_language(
    next: &str,
    store: &SettingsLanguageStore,
    ports: &mut LocaleEnginePorts,
    defer: &Defer,
) {
    if (ports.is_builtin_locale)(next) {
        (ports.set_locale.borrow_mut())(next);
        return;
    }
    let next_owned = next.to_owned();
    let store = store.clone();
    let ensure = ports.ensure_pack_messages.clone();
    let set = ports.set_locale.clone();
    defer(Box::new(move || {
        if ensure.borrow_mut()(&next_owned).is_err() {
            return;
        }
        if store.current_language() == next_owned {
            set.borrow_mut()(&next_owned);
        }
    }));
}

pub fn start_locale_sync(
    store: &SettingsLanguageStore,
    mut ports: LocaleEnginePorts,
    defer: Defer,
) -> Box<dyn Fn()> {
    let initial = current_language(store);
    apply_language(&initial, store, &mut ports, &defer);

    let change_store = store.clone();
    let change_ports: Rc<RefCell<LocaleEnginePorts>> = Rc::new(RefCell::new(ports));
    let change_defer = defer;
    let listener: LanguageListener = Rc::new(RefCell::new(move |next: String, previous: Option<String>| {
        if Some(next.as_str()) == previous.as_deref() {
            return;
        }
        let mut ports = change_ports.borrow_mut();
        apply_language(&next, &change_store, &mut ports, &change_defer);
    }));
    store.subscribe(listener)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn ports(log: &Rc<RefCell<Vec<String>>>, builtins: &[&str]) -> LocaleEnginePorts {
        let builtins: Vec<String> = builtins.iter().map(|value| (*value).to_owned()).collect();
        let set_log = log.clone();
        LocaleEnginePorts {
            is_builtin_locale: Rc::new(move |locale| builtins.contains(&locale.to_owned())),
            ensure_pack_messages: Rc::new(RefCell::new(|_: &str| Ok(()))),
            set_locale: Rc::new(RefCell::new(move |locale: &str| set_log.borrow_mut().push(format!("set:{locale}")))),
        }
    }

    fn immediate() -> (Rc<RefCell<Vec<Deferred>>>, Defer) {
        let queue = Rc::new(RefCell::new(Vec::new()));
        let sink = queue.clone();
        (queue, Rc::new(move |task| sink.borrow_mut().push(task)))
    }

    fn drain(queue: &Rc<RefCell<Vec<Deferred>>>) {
        let tasks = std::mem::take(&mut *queue.borrow_mut());
        for task in tasks {
            task();
        }
    }

    #[test]
    fn current_language_defaults_to_english() {
        let store = SettingsLanguageStore::new(None);
        assert_eq!(current_language(&store), "en");
        let store = SettingsLanguageStore::new(Some("zh".to_owned()));
        assert_eq!(current_language(&store), "zh");
    }

    #[test]
    fn builtin_locales_switch_synchronously_without_deferring() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let store = SettingsLanguageStore::new(Some("en".to_owned()));
        let (queue, defer) = immediate();
        let mut ports = ports(&log, &["en", "zh"]);
        apply_language("zh", &store, &mut ports, &defer);
        assert_eq!(log.borrow().as_slice(), &["set:zh".to_owned()]);
        assert!(queue.borrow().is_empty());
    }

    #[test]
    fn pack_locales_defer_and_respect_last_writer_wins() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let store = SettingsLanguageStore::new(Some("en".to_owned()));
        let (queue, defer) = immediate();
        let mut ports = ports(&log, &["en"]);
        apply_language("pack-a", &store, &mut ports, &defer);
        assert!(log.borrow().is_empty(), "no synchronous switch for a pack");
        store.set_language(Some("pack-b".to_owned()));
        apply_language("pack-b", &store, &mut ports, &defer);
        drain(&queue);
        assert_eq!(log.borrow().as_slice(), &["set:pack-b".to_owned()], "the stale pack-a switch is dropped");
    }

    #[test]
    fn pack_message_failure_switches_nothing() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let store = SettingsLanguageStore::new(Some("en".to_owned()));
        let (queue, defer) = immediate();
        let mut ports = ports(&log, &["en"]);
        ports.ensure_pack_messages = Rc::new(RefCell::new(|_: &str| Err::<(), String>("pack read failed".to_owned())));
        apply_language("pack-x", &store, &mut ports, &defer);
        drain(&queue);
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn start_seeds_then_follows_language_changes_until_unsubscribed() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let store = SettingsLanguageStore::new(Some("en".to_owned()));
        let (queue, defer) = immediate();
        let mut ports = ports(&log, &["en", "zh"]);
        let unsubscribe = start_locale_sync(&store, ports, defer);
        assert_eq!(log.borrow().as_slice(), &["set:en".to_owned()]);
        store.set_language(Some("zh".to_owned()));
        assert_eq!(log.borrow().as_slice(), &["set:en".to_owned(), "set:zh".to_owned()]);
        unsubscribe();
        store.set_language(Some("en".to_owned()));
        assert_eq!(log.borrow().len(), 2, "no switch after unsubscribe");
    }

    #[test]
    fn a_change_to_the_same_language_is_ignored() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let store = SettingsLanguageStore::new(Some("en".to_owned()));
        let (queue, defer) = immediate();
        let mut ports = ports(&log, &["en"]);
        let _unsubscribe = start_locale_sync(&store, ports, defer);
        store.set_language(Some("en".to_owned()));
        assert_eq!(log.borrow().len(), 1);
    }

    #[test]
    fn an_undefined_previous_language_still_applies_the_change() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let store = SettingsLanguageStore::new(None);
        let (queue, defer) = immediate();
        let mut ports = ports(&log, &["en", "zh"]);
        let _unsubscribe = start_locale_sync(&store, ports, defer);
        store.set_language(Some("zh".to_owned()));
        assert_eq!(log.borrow().as_slice(), &["set:en".to_owned(), "set:zh".to_owned()]);
        let _ = queue;
    }

    #[test]
    fn subscribe_unsubscribes_are_independent() {
        let store = SettingsLanguageStore::new(None);
        let calls = Rc::new(Cell::new(0usize));
        let first_sink = calls.clone();
        let first: LanguageListener = Rc::new(RefCell::new(move |_, _| first_sink.set(first_sink.get() + 1)));
        let second_sink = calls.clone();
        let second: LanguageListener = Rc::new(RefCell::new(move |_, _| second_sink.set(second_sink.get() + 1)));
        let first_off = store.subscribe(first);
        let _second_off = store.subscribe(second);
        store.set_language(Some("zh".to_owned()));
        assert_eq!(calls.get(), 2);
        first_off();
        store.set_language(Some("en".to_owned()));
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::current_language;
        let _ = crate::apply_language;
        let _ = crate::start_locale_sync;
        assert_eq!(crate::DEFAULT_LOCALE, "en");
        let _: Option<crate::SettingsLanguageStore> = None;
        let _: Option<crate::LocaleEnginePorts> = None;
    }
}
