use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct KvSlotSnapshot {
    pub selected_key: Option<String>,
    pub dirty: bool,
    pub loaded_count: u64,
    pub scan_cursor: String,
    pub scanning: bool,
    pub scan_budget_used: u64,
    pub scan_budget_total: u64,
    pub selection_count: u64,
    pub last_write_command: Option<String>,
    pub last_write_duration_ms: Option<f64>,
}

fn empty_snapshot() -> KvSlotSnapshot {
    KvSlotSnapshot {
        selected_key: None,
        dirty: false,
        loaded_count: 0,
        scan_cursor: "0".to_owned(),
        scanning: false,
        scan_budget_used: 0,
        scan_budget_total: 0,
        selection_count: 0,
        last_write_command: None,
        last_write_duration_ms: None,
    }
}

enum PatchField {
    SelectedKey(Option<String>),
    Dirty(bool),
    LoadedCount(u64),
    ScanCursor(String),
    Scanning(bool),
    ScanBudgetUsed(u64),
    ScanBudgetTotal(u64),
    SelectionCount(u64),
    LastWriteCommand(Option<String>),
    LastWriteDurationMs(Option<f64>),
}

fn object_is_f64(left: f64, right: f64) -> bool {
    if left.is_nan() && right.is_nan() {
        return true;
    }
    left.to_bits() == right.to_bits()
}

fn object_is_opt_f64(left: Option<f64>, right: Option<f64>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => object_is_f64(left, right),
        _ => false,
    }
}

#[derive(Clone)]
pub struct KvSlotState {
    inner: Rc<RefCell<KvSlotStateInner>>,
}

struct KvSlotStateInner {
    snapshot: KvSlotSnapshot,
    listeners: Vec<Rc<dyn Fn()>>,
}

impl KvSlotState {
    pub fn create() -> Self {
        Self {
            inner: Rc::new(RefCell::new(KvSlotStateInner {
                snapshot: empty_snapshot(),
                listeners: Vec::new(),
            })),
        }
    }

    fn commit(&self, fields: Vec<PatchField>) {
        let mut inner = self.inner.borrow_mut();
        let changed = fields.iter().any(|field| match field {
            PatchField::SelectedKey(value) => inner.snapshot.selected_key != *value,
            PatchField::Dirty(value) => inner.snapshot.dirty != *value,
            PatchField::LoadedCount(value) => inner.snapshot.loaded_count != *value,
            PatchField::ScanCursor(value) => inner.snapshot.scan_cursor != *value,
            PatchField::Scanning(value) => inner.snapshot.scanning != *value,
            PatchField::ScanBudgetUsed(value) => inner.snapshot.scan_budget_used != *value,
            PatchField::ScanBudgetTotal(value) => inner.snapshot.scan_budget_total != *value,
            PatchField::SelectionCount(value) => inner.snapshot.selection_count != *value,
            PatchField::LastWriteCommand(value) => inner.snapshot.last_write_command != *value,
            PatchField::LastWriteDurationMs(value) => {
                !object_is_opt_f64(inner.snapshot.last_write_duration_ms, *value)
            }
        });
        if !changed {
            return;
        }
        for field in fields {
            match field {
                PatchField::SelectedKey(value) => inner.snapshot.selected_key = value,
                PatchField::Dirty(value) => inner.snapshot.dirty = value,
                PatchField::LoadedCount(value) => inner.snapshot.loaded_count = value,
                PatchField::ScanCursor(value) => inner.snapshot.scan_cursor = value,
                PatchField::Scanning(value) => inner.snapshot.scanning = value,
                PatchField::ScanBudgetUsed(value) => inner.snapshot.scan_budget_used = value,
                PatchField::ScanBudgetTotal(value) => inner.snapshot.scan_budget_total = value,
                PatchField::SelectionCount(value) => inner.snapshot.selection_count = value,
                PatchField::LastWriteCommand(value) => inner.snapshot.last_write_command = value,
                PatchField::LastWriteDurationMs(value) => inner.snapshot.last_write_duration_ms = value,
            }
        }
        let listeners = inner.listeners.clone();
        drop(inner);
        for listener in listeners {
            listener();
        }
    }

    pub fn subscribe(&self, listener: Rc<dyn Fn()>) -> Box<dyn Fn()> {
        self.inner.borrow_mut().listeners.push(listener.clone());
        let inner = self.inner.clone();
        Box::new(move || {
            let mut state = inner.borrow_mut();
            if let Some(index) = state.listeners.iter().position(|item| Rc::ptr_eq(item, &listener)) {
                state.listeners.remove(index);
            }
        })
    }

    pub fn get_selected_key(&self) -> Option<String> {
        self.inner.borrow().snapshot.selected_key.clone()
    }

    pub fn select_key(&self, key: Option<String>) {
        self.commit(vec![PatchField::SelectedKey(key)]);
    }

    pub fn get_dirty(&self) -> bool {
        self.inner.borrow().snapshot.dirty
    }

    pub fn set_dirty(&self, dirty: bool) {
        self.commit(vec![PatchField::Dirty(dirty)]);
    }

    pub fn get_loaded_count(&self) -> u64 {
        self.inner.borrow().snapshot.loaded_count
    }

    pub fn set_loaded_count(&self, count: u64) {
        self.commit(vec![PatchField::LoadedCount(count)]);
    }

    pub fn get_scan_cursor(&self) -> String {
        self.inner.borrow().snapshot.scan_cursor.clone()
    }

    pub fn set_scan_cursor(&self, cursor: String) {
        self.commit(vec![PatchField::ScanCursor(cursor)]);
    }

    pub fn is_scanning(&self) -> bool {
        self.inner.borrow().snapshot.scanning
    }

    pub fn set_scanning(&self, scanning: bool) {
        self.commit(vec![PatchField::Scanning(scanning)]);
    }

    pub fn get_scan_budget_used(&self) -> u64 {
        self.inner.borrow().snapshot.scan_budget_used
    }

    pub fn get_scan_budget_total(&self) -> u64 {
        self.inner.borrow().snapshot.scan_budget_total
    }

    pub fn set_scan_budget(&self, used: u64, total: u64) {
        self.commit(vec![PatchField::ScanBudgetUsed(used), PatchField::ScanBudgetTotal(total)]);
    }

    pub fn get_selection_count(&self) -> u64 {
        self.inner.borrow().snapshot.selection_count
    }

    pub fn set_selection_count(&self, count: u64) {
        self.commit(vec![PatchField::SelectionCount(count)]);
    }

    pub fn get_last_write_command(&self) -> Option<String> {
        self.inner.borrow().snapshot.last_write_command.clone()
    }

    pub fn get_last_write_duration_ms(&self) -> Option<f64> {
        self.inner.borrow().snapshot.last_write_duration_ms
    }

    pub fn record_write(&self, command: Option<String>, duration_ms: Option<f64>) {
        self.commit(vec![
            PatchField::LastWriteCommand(command),
            PatchField::LastWriteDurationMs(duration_ms),
        ]);
    }
}

pub fn create_kv_slot_state() -> KvSlotState {
    KvSlotState::create()
}

thread_local! {
    static ATOMS: RefCell<HashMap<String, KvSlotState>> = RefCell::new(HashMap::new());
}

pub fn get_kv_slot_state(panel_id: &str) -> KvSlotState {
    ATOMS.with(|atoms| {
        let mut map = atoms.borrow_mut();
        map.entry(panel_id.to_owned()).or_insert_with(KvSlotState::create).clone()
    })
}

pub fn prune_kv_slot_states(live_panel_ids: &HashSet<String>) {
    ATOMS.with(|atoms| {
        atoms.borrow_mut().retain(|panel_id, _| live_panel_ids.contains(panel_id));
    });
}

pub fn reset_kv_slot_states_for_tests() {
    ATOMS.with(|atoms| atoms.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn a_fresh_atom_has_the_empty_snapshot() {
        let state = KvSlotState::create();
        assert_eq!(state.get_selected_key(), None);
        assert!(!state.get_dirty());
        assert_eq!(state.get_loaded_count(), 0);
        assert_eq!(state.get_scan_cursor(), "0");
        assert!(!state.is_scanning());
        assert_eq!(state.get_scan_budget_used(), 0);
        assert_eq!(state.get_scan_budget_total(), 0);
        assert_eq!(state.get_selection_count(), 0);
        assert_eq!(state.get_last_write_command(), None);
        assert_eq!(state.get_last_write_duration_ms(), None);
    }

    #[test]
    fn setters_update_the_scalar_getters() {
        let state = KvSlotState::create();
        state.select_key(Some("user:1".to_owned()));
        state.set_dirty(true);
        state.set_loaded_count(12);
        state.set_scan_cursor("42".to_owned());
        state.set_scanning(true);
        state.set_scan_budget(3, 10);
        state.set_selection_count(2);
        state.record_write(Some("SET".to_owned()), Some(7.5));
        assert_eq!(state.get_selected_key().as_deref(), Some("user:1"));
        assert!(state.get_dirty());
        assert_eq!(state.get_loaded_count(), 12);
        assert_eq!(state.get_scan_cursor(), "42");
        assert!(state.is_scanning());
        assert_eq!((state.get_scan_budget_used(), state.get_scan_budget_total()), (3, 10));
        assert_eq!(state.get_selection_count(), 2);
        assert_eq!(state.get_last_write_command().as_deref(), Some("SET"));
        assert_eq!(state.get_last_write_duration_ms(), Some(7.5));
    }

    #[test]
    fn no_op_writes_notify_nobody_and_real_writes_notify_once() {
        let state = KvSlotState::create();
        let calls = Rc::new(Cell::new(0usize));
        let sink = calls.clone();
        let _unsubscribe = state.subscribe(Rc::new(move || sink.set(sink.get() + 1)));
        state.set_dirty(false);
        state.set_loaded_count(0);
        assert_eq!(calls.get(), 0);
        state.set_dirty(true);
        assert_eq!(calls.get(), 1);
        state.set_dirty(true);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn multi_field_writes_apply_all_fields_with_one_notification() {
        let state = KvSlotState::create();
        let calls = Rc::new(Cell::new(0usize));
        let sink = calls.clone();
        let _unsubscribe = state.subscribe(Rc::new(move || sink.set(sink.get() + 1)));
        state.set_scan_budget(5, 20);
        assert_eq!((state.get_scan_budget_used(), state.get_scan_budget_total()), (5, 20));
        assert_eq!(calls.get(), 1);
        state.record_write(Some("GET".to_owned()), Some(1.25));
        assert_eq!((state.get_last_write_command().as_deref(), state.get_last_write_duration_ms()), (Some("GET"), Some(1.25)));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let state = KvSlotState::create();
        let calls = Rc::new(Cell::new(0usize));
        let sink = calls.clone();
        let unsubscribe = state.subscribe(Rc::new(move || sink.set(sink.get() + 1)));
        state.set_dirty(true);
        unsubscribe();
        state.set_dirty(false);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_listener_may_unsubscribe_during_notification() {
        let state = KvSlotState::create();
        let calls = Rc::new(Cell::new(0usize));
        let sink = calls.clone();
        let slot: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
        let inner_slot = slot.clone();
        let listener: Rc<dyn Fn()> = Rc::new(move || {
            sink.set(sink.get() + 1);
            if let Some(unsubscribe) = inner_slot.borrow_mut().take() {
                unsubscribe();
            }
        });
        let unsubscribe = state.subscribe(listener);
        *slot.borrow_mut() = Some(unsubscribe);
        state.set_dirty(true);
        state.set_dirty(false);
        assert_eq!(calls.get(), 1, "the listener removed itself on the first notification");
    }

    #[test]
    fn object_is_treats_nan_as_equal_and_minus_zero_as_different() {
        let state = KvSlotState::create();
        state.record_write(None, Some(f64::NAN));
        let calls = Rc::new(Cell::new(0usize));
        let sink = calls.clone();
        let _unsubscribe = state.subscribe(Rc::new(move || sink.set(sink.get() + 1)));
        state.record_write(None, Some(f64::NAN));
        assert_eq!(calls.get(), 0, "NaN equals NaN under Object.is");
        state.record_write(None, Some(-0.0));
        assert_eq!(calls.get(), 1);
        state.record_write(None, Some(-0.0));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn atoms_are_keyed_by_panel_id_and_pruned_by_liveness() {
        reset_kv_slot_states_for_tests();
        let first = get_kv_slot_state("p1");
        let again = get_kv_slot_state("p1");
        let second = get_kv_slot_state("p2");
        assert!(Rc::ptr_eq(&first.inner, &again.inner));
        assert!(!Rc::ptr_eq(&first.inner, &second.inner));
        first.select_key(Some("k".to_owned()));

        let live = HashSet::from(["p2".to_owned()]);
        prune_kv_slot_states(&live);
        assert_eq!(first.get_selected_key().as_deref(), Some("k"), "existing handles survive pruning");
        let fresh = get_kv_slot_state("p1");
        assert!(!Rc::ptr_eq(&first.inner, &fresh.inner), "a pruned panel is recreated");
        assert_eq!(fresh.get_selected_key(), None);
        reset_kv_slot_states_for_tests();
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::create_kv_slot_state;
        let _ = crate::get_kv_slot_state;
        let _ = crate::prune_kv_slot_states;
        let _ = crate::reset_kv_slot_states_for_tests;
        let _: Option<crate::KvSlotState> = None;
        let _: Option<crate::KvSlotSnapshot> = None;
    }
}
