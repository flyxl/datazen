use std::rc::Rc;

use crate::SessionLocalStorage;

pub struct DismissPrefs {
    is_dismissed: Box<dyn Fn() -> bool>,
    set_dismissed: Box<dyn Fn()>,
    clear_dismissed: Box<dyn Fn()>,
}

impl DismissPrefs {
    pub fn is_dismissed(&self) -> bool {
        (self.is_dismissed)()
    }

    pub fn set_dismissed(&self) {
        (self.set_dismissed)()
    }

    pub fn clear_dismissed(&self) {
        (self.clear_dismissed)()
    }
}

pub fn create_dismiss_prefs(
    storage_key: String,
    storage: Rc<dyn SessionLocalStorage>,
) -> DismissPrefs {
    let read_key = storage_key.clone();
    let read_storage = storage.clone();
    let write_key = storage_key.clone();
    let write_storage = storage.clone();
    DismissPrefs {
        is_dismissed: Box::new(move || match read_storage.get(&read_key) {
            Ok(Some(value)) => value == "1",
            Ok(None) | Err(_) => false,
        }),
        set_dismissed: Box::new(move || {
            let _ = write_storage.set(&write_key, "1");
        }),
        clear_dismissed: Box::new(move || {
            let _ = storage.remove(&storage_key);
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemoryStorage {
        values: RefCell<HashMap<String, String>>,
        calls: RefCell<Vec<(String, String)>>,
        fail_get: RefCell<bool>,
        fail_set: RefCell<bool>,
        fail_remove: RefCell<bool>,
    }

    impl MemoryStorage {
        fn set_value(&self, key: &str, value: &str) {
            self.values.borrow_mut().insert(key.to_owned(), value.to_owned());
        }

        fn get_value(&self, key: &str) -> Option<String> {
            self.values.borrow().get(key).cloned()
        }
    }

    impl SessionLocalStorage for MemoryStorage {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            self.calls.borrow_mut().push(("get".to_owned(), key.to_owned()));
            if *self.fail_get.borrow() {
                return Err("get failed".to_owned());
            }
            Ok(self.get_value(key))
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(("set".to_owned(), format!("{key}={value}")));
            if *self.fail_set.borrow() {
                return Err("set failed".to_owned());
            }
            self.set_value(key, value);
            Ok(())
        }

        fn remove(&self, key: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(("remove".to_owned(), key.to_owned()));
            if *self.fail_remove.borrow() {
                return Err("remove failed".to_owned());
            }
            self.values.borrow_mut().remove(key);
            Ok(())
        }
    }

    fn prefs(key: &str) -> (DismissPrefs, Rc<MemoryStorage>) {
        let storage = Rc::new(MemoryStorage::default());
        let erased: Rc<dyn SessionLocalStorage> = storage.clone();
        (create_dismiss_prefs(key.to_owned(), erased), storage)
    }

    #[test]
    fn fresh_prefs_are_not_dismissed() {
        let (prefs, storage) = prefs("onboarding");
        assert!(!prefs.is_dismissed());
        assert_eq!(storage.calls.borrow().as_slice(), &[("get".to_owned(), "onboarding".to_owned())]);
    }

    #[test]
    fn set_then_clear_round_trip() {
        let (prefs, storage) = prefs("onboarding");
        prefs.set_dismissed();
        assert!(prefs.is_dismissed());
        assert_eq!(storage.get_value("onboarding").as_deref(), Some("1"));
        prefs.clear_dismissed();
        assert!(!prefs.is_dismissed());
        assert_eq!(storage.get_value("onboarding"), None);
        assert_eq!(
            storage.calls.borrow().as_slice(),
            &[
                ("set".to_owned(), "onboarding=1".to_owned()),
                ("get".to_owned(), "onboarding".to_owned()),
                ("remove".to_owned(), "onboarding".to_owned()),
                ("get".to_owned(), "onboarding".to_owned()),
            ]
        );
    }

    #[test]
    fn only_the_exact_marker_counts_as_dismissed() {
        let (prefs, storage) = prefs("k");
        for value in ["1 ", "true", "0", "2", "", "yes"] {
            storage.set_value("k", value);
            assert!(!prefs.is_dismissed(), "{value} is not the marker");
        }
        storage.set_value("k", "1");
        assert!(prefs.is_dismissed());
    }

    #[test]
    fn storage_errors_are_swallowed() {
        let (prefs, storage) = prefs("k");
        *storage.fail_get.borrow_mut() = true;
        assert!(!prefs.is_dismissed());
        *storage.fail_get.borrow_mut() = false;

        *storage.fail_set.borrow_mut() = true;
        prefs.set_dismissed();
        assert_eq!(storage.get_value("k"), None);
        *storage.fail_set.borrow_mut() = false;

        storage.set_value("k", "1");
        *storage.fail_remove.borrow_mut() = true;
        prefs.clear_dismissed();
        assert_eq!(storage.get_value("k").as_deref(), Some("1"));
    }

    #[test]
    fn distinct_keys_are_independent_and_shared_keys_agree() {
        let storage = Rc::new(MemoryStorage::default());
        let erased: Rc<dyn SessionLocalStorage> = storage.clone();
        let first = create_dismiss_prefs("a".to_owned(), erased.clone());
        let second = create_dismiss_prefs("b".to_owned(), erased);
        first.set_dismissed();
        assert!(first.is_dismissed());
        assert!(!second.is_dismissed());

        let third = create_dismiss_prefs("a".to_owned(), Rc::new(MemoryStorage::default()));
        assert!(!third.is_dismissed(), "a separate storage has its own state");
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::create_dismiss_prefs;
        let _: Option<crate::DismissPrefs> = None;
    }
}
