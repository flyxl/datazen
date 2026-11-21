#[cfg(test)]
mod tests;

pub const CHANNEL_AUTO: &str = "auto";
pub const CHECK_INTERVAL_MS: u64 = 60_000;
pub const EVENT_FOCUS: &str = "focus";
pub const EVENT_ONLINE: &str = "online";
pub const EVENT_VISIBILITY_CHANGE: &str = "visibilitychange";
pub const VISIBILITY_VISIBLE: &str = "visible";

pub struct ScheduleDeps<'a> {
    pub check_if_due: &'a dyn Fn(),
    pub add_listener: &'a dyn Fn(&str),
    pub remove_listener: &'a dyn Fn(&str),
    pub set_interval: &'a dyn Fn(u64) -> u64,
    pub clear_interval: &'a dyn Fn(u64),
    pub visibility_state: &'a dyn Fn() -> String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleHandle {
    pub interval_id: u64,
    pub active: bool,
}

pub fn start_update_reminder_schedule(
    channel: &str,
    deps: &ScheduleDeps<'_>,
) -> Option<ScheduleHandle> {
    if channel != CHANNEL_AUTO {
        return None;
    }
    (deps.check_if_due)();
    (deps.add_listener)(EVENT_FOCUS);
    (deps.add_listener)(EVENT_ONLINE);
    (deps.add_listener)(EVENT_VISIBILITY_CHANGE);
    let interval_id = (deps.set_interval)(CHECK_INTERVAL_MS);
    Some(ScheduleHandle {
        interval_id,
        active: true,
    })
}

pub fn dispatch_event(handle: &ScheduleHandle, event: &str, deps: &ScheduleDeps<'_>) {
    if !handle.active {
        return;
    }
    match event {
        EVENT_FOCUS | EVENT_ONLINE => (deps.check_if_due)(),
        EVENT_VISIBILITY_CHANGE => {
            if (deps.visibility_state)() == VISIBILITY_VISIBLE {
                (deps.check_if_due)();
            }
        }
        _ => {}
    }
}

pub fn on_interval_tick(handle: &ScheduleHandle, deps: &ScheduleDeps<'_>) {
    if handle.active {
        (deps.check_if_due)();
    }
}

pub fn stop_update_reminder_schedule(handle: &mut ScheduleHandle, deps: &ScheduleDeps<'_>) {
    (deps.clear_interval)(handle.interval_id);
    (deps.remove_listener)(EVENT_FOCUS);
    (deps.remove_listener)(EVENT_ONLINE);
    (deps.remove_listener)(EVENT_VISIBILITY_CHANGE);
    handle.active = false;
}
