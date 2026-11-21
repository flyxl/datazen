pub const HIDE_SPLASH_DELAY_MS: i64 = 350;

pub const SPLASH_DOM_GAP: [&str; 5] = [
    "HTMLElement",
    "splash.classList.add('hide')",
    "splash.remove",
    "window.setTimeout",
    "window.clearTimeout",
];

pub const STARTUP_TASK_GAP: [&str; 3] = [
    "Promise<unknown> task",
    "Promise.race",
    "task left running after timeout deadline",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HideSplashPlan {
    Noop,
    HideThenRemove { delay_ms: i64 },
}

pub fn hide_splash_plan(has_splash: bool) -> HideSplashPlan {
    if has_splash {
        HideSplashPlan::HideThenRemove {
            delay_ms: HIDE_SPLASH_DELAY_MS,
        }
    } else {
        HideSplashPlan::Noop
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupWaitResult {
    Completed,
    TimedOut,
}

impl StartupWaitResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::TimedOut => "timed-out",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupSettlement {
    TaskCompleted,
    TimedOut,
}

pub fn run_startup_wait_task<S, C>(
    timeout_ms: i64,
    settle: S,
    clear_timeout: C,
) -> StartupWaitResult
where
    S: FnOnce(i64) -> StartupSettlement,
    C: FnOnce(),
{
    let settlement = settle(timeout_ms);
    clear_timeout();
    match settlement {
        StartupSettlement::TaskCompleted => StartupWaitResult::Completed,
        StartupSettlement::TimedOut => StartupWaitResult::TimedOut,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn gap_surfaces_are_pinned() {
        assert_eq!(HIDE_SPLASH_DELAY_MS, 350);
        assert_eq!(SPLASH_DOM_GAP.len(), 5);
        assert!(SPLASH_DOM_GAP.contains(&"splash.classList.add('hide')"));
        assert!(SPLASH_DOM_GAP.contains(&"splash.remove"));
        assert!(SPLASH_DOM_GAP.contains(&"window.clearTimeout"));
        assert_eq!(STARTUP_TASK_GAP.len(), 3);
        assert!(STARTUP_TASK_GAP.contains(&"Promise.race"));
        assert_eq!(crate::SPLASH_DOM_GAP.len(), 5);
        assert_eq!(crate::STARTUP_TASK_GAP.len(), 3);
    }

    #[test]
    fn null_splash_is_a_noop_and_present_splash_hides_then_removes() {
        assert_eq!(hide_splash_plan(false), HideSplashPlan::Noop);
        assert_eq!(
            hide_splash_plan(true),
            HideSplashPlan::HideThenRemove { delay_ms: 350 }
        );
    }

    #[test]
    fn startup_result_literals_are_pinned() {
        assert_eq!(StartupWaitResult::Completed.as_str(), "completed");
        assert_eq!(StartupWaitResult::TimedOut.as_str(), "timed-out");
    }

    #[test]
    fn startup_wait_uses_the_timeout_but_clears_it_on_both_outcomes() {
        let scheduled = RefCell::new(Vec::new());
        let cleared = RefCell::new(0);
        let result = run_startup_wait_task(
            1_500,
            |timeout_ms| {
                scheduled.borrow_mut().push(timeout_ms);
                StartupSettlement::TaskCompleted
            },
            || *cleared.borrow_mut() += 1,
        );
        assert_eq!(result, StartupWaitResult::Completed);
        assert_eq!(scheduled.into_inner(), [1_500]);
        assert_eq!(cleared.into_inner(), 1);

        let cleared = RefCell::new(0);
        let result = run_startup_wait_task(
            250,
            |_| StartupSettlement::TimedOut,
            || *cleared.borrow_mut() += 1,
        );
        assert_eq!(result, StartupWaitResult::TimedOut);
        assert_eq!(cleared.into_inner(), 1);
    }
}
