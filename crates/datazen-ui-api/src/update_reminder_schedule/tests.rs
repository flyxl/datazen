use super::*;
use std::cell::RefCell;
use std::rc::Rc;

struct OwnedDeps {
    calls: Rc<RefCell<Vec<String>>>,
    visibility: Rc<RefCell<String>>,
    check: Box<dyn Fn()>,
    add: Box<dyn Fn(&str)>,
    remove: Box<dyn Fn(&str)>,
    set_interval: Box<dyn Fn(u64) -> u64>,
    clear_interval: Box<dyn Fn(u64)>,
    visibility_state: Box<dyn Fn() -> String>,
}

impl OwnedDeps {
    fn new() -> Self {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let visibility = Rc::new(RefCell::new("visible".to_owned()));

        let check_calls = Rc::clone(&calls);
        let add_calls = Rc::clone(&calls);
        let remove_calls = Rc::clone(&calls);
        let interval_calls = Rc::clone(&calls);
        let clear_calls = Rc::clone(&calls);
        let visibility_state = Rc::clone(&visibility);

        Self {
            check: Box::new(move || check_calls.borrow_mut().push("check".to_owned())),
            add: Box::new(move |name: &str| {
                add_calls.borrow_mut().push(format!("add:{name}"));
            }),
            remove: Box::new(move |name: &str| {
                remove_calls.borrow_mut().push(format!("remove:{name}"));
            }),
            set_interval: Box::new(move |millis: u64| {
                interval_calls.borrow_mut().push(format!("interval:{millis}"));
                42
            }),
            clear_interval: Box::new(move |id: u64| {
                clear_calls.borrow_mut().push(format!("clear:{id}"));
            }),
            visibility_state: Box::new(move || visibility_state.borrow().clone()),
            calls,
            visibility,
        }
    }

    fn view(&self) -> ScheduleDeps<'_> {
        ScheduleDeps {
            check_if_due: self.check.as_ref(),
            add_listener: self.add.as_ref(),
            remove_listener: self.remove.as_ref(),
            set_interval: self.set_interval.as_ref(),
            clear_interval: self.clear_interval.as_ref(),
            visibility_state: self.visibility_state.as_ref(),
        }
    }

    fn recorded(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

#[test]
fn constants_are_pinned() {
    assert_eq!(CHANNEL_AUTO, "auto");
    assert_eq!(CHECK_INTERVAL_MS, 60_000);
    assert_eq!(EVENT_FOCUS, "focus");
    assert_eq!(EVENT_ONLINE, "online");
    assert_eq!(EVENT_VISIBILITY_CHANGE, "visibilitychange");
    assert_eq!(VISIBILITY_VISIBLE, "visible");
}

#[test]
fn a_non_auto_channel_schedules_nothing() {
    for channel in ["manual", "none", "", "AUTO"] {
        let deps = OwnedDeps::new();
        let handle = start_update_reminder_schedule(channel, &deps.view());
        assert!(handle.is_none());
        assert!(deps.recorded().is_empty());
    }
}

#[test]
fn an_auto_channel_checks_on_launch_and_registers_exactly_three_listeners() {
    let deps = OwnedDeps::new();
    let handle = start_update_reminder_schedule(CHANNEL_AUTO, &deps.view());
    assert_eq!(
        handle,
        Some(ScheduleHandle {
            interval_id: 42,
            active: true
        })
    );
    assert_eq!(
        deps.recorded(),
        [
            "check",
            "add:focus",
            "add:online",
            "add:visibilitychange",
            "interval:60000"
        ]
    );
}

#[test]
fn focus_and_online_events_trigger_a_check() {
    let deps = OwnedDeps::new();
    let handle = match start_update_reminder_schedule(CHANNEL_AUTO, &deps.view()) {
        Some(value) => value,
        None => panic!("auto channel must start"),
    };
    dispatch_event(&handle, EVENT_FOCUS, &deps.view());
    dispatch_event(&handle, EVENT_ONLINE, &deps.view());
    assert_eq!(
        deps.recorded()
            .iter()
            .filter(|call| call.as_str() == "check")
            .count(),
        3
    );
}

#[test]
fn visibility_events_only_check_when_the_document_is_visible() {
    let deps = OwnedDeps::new();
    let handle = match start_update_reminder_schedule(CHANNEL_AUTO, &deps.view()) {
        Some(value) => value,
        None => panic!("auto channel must start"),
    };
    *deps.visibility.borrow_mut() = "hidden".to_owned();
    dispatch_event(&handle, EVENT_VISIBILITY_CHANGE, &deps.view());
    assert_eq!(
        deps.recorded()
            .iter()
            .filter(|call| call.as_str() == "check")
            .count(),
        1
    );
    *deps.visibility.borrow_mut() = "visible".to_owned();
    dispatch_event(&handle, EVENT_VISIBILITY_CHANGE, &deps.view());
    assert_eq!(
        deps.recorded()
            .iter()
            .filter(|call| call.as_str() == "check")
            .count(),
        2
    );
}

#[test]
fn unknown_events_and_inactive_handles_do_not_check() {
    let deps = OwnedDeps::new();
    let handle = match start_update_reminder_schedule(CHANNEL_AUTO, &deps.view()) {
        Some(value) => value,
        None => panic!("auto channel must start"),
    };
    dispatch_event(&handle, "resize", &deps.view());
    assert_eq!(
        deps.recorded()
            .iter()
            .filter(|call| call.as_str() == "check")
            .count(),
        1
    );
    let stopped = ScheduleHandle {
        interval_id: 42,
        active: false,
    };
    dispatch_event(&stopped, EVENT_FOCUS, &deps.view());
    on_interval_tick(&stopped, &deps.view());
    assert_eq!(
        deps.recorded()
            .iter()
            .filter(|call| call.as_str() == "check")
            .count(),
        1
    );
}

#[test]
fn an_interval_tick_uses_the_same_check_callback() {
    let deps = OwnedDeps::new();
    let handle = match start_update_reminder_schedule(CHANNEL_AUTO, &deps.view()) {
        Some(value) => value,
        None => panic!("auto channel must start"),
    };
    on_interval_tick(&handle, &deps.view());
    on_interval_tick(&handle, &deps.view());
    assert_eq!(
        deps.recorded()
            .iter()
            .filter(|call| call.as_str() == "check")
            .count(),
        3
    );
}

#[test]
fn stopping_clears_the_interval_removes_all_listeners_and_disables_dispatch() {
    let deps = OwnedDeps::new();
    let mut handle = match start_update_reminder_schedule(CHANNEL_AUTO, &deps.view()) {
        Some(value) => value,
        None => panic!("auto channel must start"),
    };
    stop_update_reminder_schedule(&mut handle, &deps.view());
    assert!(!handle.active);
    assert_eq!(
        deps.recorded(),
        [
            "check",
            "add:focus",
            "add:online",
            "add:visibilitychange",
            "interval:60000",
            "clear:42",
            "remove:focus",
            "remove:online",
            "remove:visibilitychange"
        ]
    );
    dispatch_event(&handle, EVENT_FOCUS, &deps.view());
    dispatch_event(&handle, EVENT_ONLINE, &deps.view());
    dispatch_event(&handle, EVENT_VISIBILITY_CHANGE, &deps.view());
    on_interval_tick(&handle, &deps.view());
    assert_eq!(
        deps.recorded()
            .iter()
            .filter(|call| call.as_str() == "check")
            .count(),
        1
    );
}

#[test]
fn crate_root_reexports_the_schedule_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::ScheduleDeps<'static>>();
    resolves::<crate::ScheduleHandle>();
    assert_eq!(crate::CHECK_INTERVAL_MS, 60_000);
    let deps = OwnedDeps::new();
    assert!(crate::start_update_reminder_schedule("manual", &deps.view()).is_none());
}
