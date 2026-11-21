use std::rc::Rc;

use crate::{create_dismiss_prefs, DismissPrefs, SessionLocalStorage};

pub const TRANSFER_LIMITATIONS_DISMISSED_KEY: &str = "datazen:transfer-limitations-dismissed";

pub fn create_transfer_limitations_dismiss_prefs(
    storage: Rc<dyn SessionLocalStorage>,
) -> DismissPrefs {
    create_dismiss_prefs(TRANSFER_LIMITATIONS_DISMISSED_KEY.to_owned(), storage)
}

pub fn is_transfer_limitations_dismissed(prefs: &DismissPrefs) -> bool {
    prefs.is_dismissed()
}

pub fn set_transfer_limitations_dismissed(prefs: &DismissPrefs) {
    prefs.set_dismissed()
}

pub fn clear_transfer_limitations_dismissed(prefs: &DismissPrefs) {
    prefs.clear_dismissed()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemoryStorage {
        values: RefCell<HashMap<String, String>>,
        calls: RefCell<Vec<String>>,
        fail_get: RefCell<bool>,
        fail_set: RefCell<bool>,
        fail_remove: RefCell<bool>,
    }

    impl SessionLocalStorage for MemoryStorage {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            self.calls.borrow_mut().push(format!("get:{key}"));
            if *self.fail_get.borrow() {
                return Err("get failed".to_owned());
            }
            Ok(self.values.borrow().get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("set:{key}={value}"));
            if *self.fail_set.borrow() {
                return Err("set failed".to_owned());
            }
            self.values
                .borrow_mut()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }

        fn remove(&self, key: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("remove:{key}"));
            if *self.fail_remove.borrow() {
                return Err("remove failed".to_owned());
            }
            self.values.borrow_mut().remove(key);
            Ok(())
        }
    }

    fn prefs() -> (DismissPrefs, Rc<MemoryStorage>) {
        let storage = Rc::new(MemoryStorage::default());
        let erased: Rc<dyn SessionLocalStorage> = storage.clone();
        (
            create_transfer_limitations_dismiss_prefs(erased),
            storage,
        )
    }

    #[test]
    fn storage_key_is_pinned() {
        assert_eq!(
            TRANSFER_LIMITATIONS_DISMISSED_KEY,
            "datazen:transfer-limitations-dismissed"
        );
    }

    #[test]
    fn fresh_prefs_read_only_the_transfer_key() {
        let (prefs, storage) = prefs();
        assert!(!is_transfer_limitations_dismissed(&prefs));
        assert_eq!(
            storage.calls.borrow().as_slice(),
            [format!("get:{TRANSFER_LIMITATIONS_DISMISSED_KEY}")]
        );
    }

    #[test]
    fn named_exports_round_trip_through_storage() {
        let (prefs, storage) = prefs();
        set_transfer_limitations_dismissed(&prefs);
        assert!(is_transfer_limitations_dismissed(&prefs));
        assert_eq!(
            storage
                .values
                .borrow()
                .get(TRANSFER_LIMITATIONS_DISMISSED_KEY)
                .map(String::as_str),
            Some("1")
        );
        clear_transfer_limitations_dismissed(&prefs);
        assert!(!is_transfer_limitations_dismissed(&prefs));
        assert!(storage
            .values
            .borrow()
            .get(TRANSFER_LIMITATIONS_DISMISSED_KEY)
            .is_none());
        assert!(storage
            .calls
            .borrow()
            .contains(&format!("set:{TRANSFER_LIMITATIONS_DISMISSED_KEY}=1")));
        assert!(storage
            .calls
            .borrow()
            .contains(&format!("remove:{TRANSFER_LIMITATIONS_DISMISSED_KEY}")));
    }

    #[test]
    fn only_the_exact_marker_counts_as_dismissed() {
        let (prefs, storage) = prefs();
        for value in ["1 ", "true", "0", "", "yes"] {
            storage.values.borrow_mut().insert(
                TRANSFER_LIMITATIONS_DISMISSED_KEY.to_owned(),
                value.to_owned(),
            );
            assert!(!is_transfer_limitations_dismissed(&prefs));
        }
        storage.values.borrow_mut().insert(
            TRANSFER_LIMITATIONS_DISMISSED_KEY.to_owned(),
            "1".to_owned(),
        );
        assert!(is_transfer_limitations_dismissed(&prefs));
    }

    #[test]
    fn storage_errors_are_swallowed() {
        let (prefs, storage) = prefs();
        *storage.fail_get.borrow_mut() = true;
        assert!(!is_transfer_limitations_dismissed(&prefs));
        *storage.fail_get.borrow_mut() = false;

        *storage.fail_set.borrow_mut() = true;
        set_transfer_limitations_dismissed(&prefs);
        assert!(storage
            .values
            .borrow()
            .get(TRANSFER_LIMITATIONS_DISMISSED_KEY)
            .is_none());
        *storage.fail_set.borrow_mut() = false;

        storage.values.borrow_mut().insert(
            TRANSFER_LIMITATIONS_DISMISSED_KEY.to_owned(),
            "1".to_owned(),
        );
        *storage.fail_remove.borrow_mut() = true;
        clear_transfer_limitations_dismissed(&prefs);
        assert_eq!(
            storage
                .values
                .borrow()
                .get(TRANSFER_LIMITATIONS_DISMISSED_KEY)
                .map(String::as_str),
            Some("1")
        );
    }
}
