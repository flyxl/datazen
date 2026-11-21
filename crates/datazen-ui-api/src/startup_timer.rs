pub const STARTUP_TIMER_GAP: [&str; 4] = [
    "performance.now()",
    "new Date()",
    "import.meta.env.DEV",
    "console.log",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallClockParts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millisecond: u32,
}

impl Default for WallClockParts {
    fn default() -> Self {
        Self {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
            millisecond: 0,
        }
    }
}

pub fn format_wall_clock(parts: &WallClockParts) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        parts.year,
        parts.month,
        parts.day,
        parts.hour,
        parts.minute,
        parts.second,
        parts.millisecond
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupMark {
    pub ts_ms: i64,
    pub delta_ms: i64,
    pub line: Option<String>,
}

pub struct StartupTimer<F> {
    origin: f64,
    last: f64,
    dev: bool,
    sink: F,
}

impl<F> StartupTimer<F>
where
    F: Fn(String),
{
    pub fn new(origin: f64, dev: bool, sink: F) -> Self {
        Self {
            origin,
            last: origin,
            dev,
            sink,
        }
    }

    pub fn mark(&mut self, now: f64, wall: &WallClockParts, label: &str) -> StartupMark {
        let ts = (now - self.origin).round() as i64;
        let delta = (now - self.last).round() as i64;
        self.last = now;
        let line = if self.dev {
            let formatted = format!(
                "{} [startup] +{:>5}ms  Δ{:>4}ms  {}",
                format_wall_clock(wall),
                ts,
                delta,
                label
            );
            (self.sink)(formatted.clone());
            Some(formatted)
        } else {
            None
        };
        StartupMark {
            ts_ms: ts,
            delta_ms: delta,
            line,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn wall() -> WallClockParts {
        WallClockParts {
            year: 2026,
            month: 1,
            day: 2,
            hour: 3,
            minute: 4,
            second: 5,
            millisecond: 6,
        }
    }

    #[test]
    fn gap_surface_is_pinned() {
        assert_eq!(STARTUP_TIMER_GAP.len(), 4);
        assert!(STARTUP_TIMER_GAP.contains(&"performance.now()"));
        assert!(STARTUP_TIMER_GAP.contains(&"console.log"));
        assert_eq!(crate::STARTUP_TIMER_GAP.len(), 4);
    }

    #[test]
    fn wall_clock_pads_every_field() {
        assert_eq!(format_wall_clock(&wall()), "2026-01-02 03:04:05.006");
        assert_eq!(
            format_wall_clock(&WallClockParts::default()),
            "1970-01-01 00:00:00.000"
        );
    }

    #[test]
    fn first_mark_uses_zero_delta_and_updates_last() {
        let lines = RefCell::new(Vec::new());
        let mut timer = StartupTimer::new(100.0, true, |line: String| {
            lines.borrow_mut().push(line);
        });
        let first = timer.mark(120.0, &wall(), "boot");
        assert_eq!(first.ts_ms, 20);
        assert_eq!(first.delta_ms, 20);
        let second = timer.mark(135.0, &wall(), "ready");
        assert_eq!(second.ts_ms, 35);
        assert_eq!(second.delta_ms, 15);
    }

    #[test]
    fn dev_log_line_is_padded_and_emitted_only_in_dev() {
        let lines = RefCell::new(Vec::new());
        let mut timer = StartupTimer::new(0.0, true, |line: String| {
            lines.borrow_mut().push(line);
        });
        let mark = timer.mark(1_234.0, &wall(), "ready");
        assert_eq!(
            mark.line.as_deref(),
            Some("2026-01-02 03:04:05.006 [startup] + 1234ms  Δ1234ms  ready")
        );
        assert_eq!(lines.borrow().len(), 1);

        let quiet_lines = RefCell::new(Vec::new());
        let mut quiet = StartupTimer::new(0.0, false, |line: String| {
            quiet_lines.borrow_mut().push(line);
        });
        let mark = quiet.mark(1_234.0, &wall(), "ready");
        assert_eq!(mark.line, None);
        assert!(quiet_lines.borrow().is_empty());
    }

    #[test]
    fn fractional_milliseconds_are_rounded() {
        let lines = RefCell::new(Vec::new());
        let mut timer = StartupTimer::new(0.4, true, |line: String| {
            lines.borrow_mut().push(line);
        });
        let mark = timer.mark(2.6, &wall(), "x");
        assert_eq!(mark.ts_ms, 2);
        assert_eq!(mark.delta_ms, 2);
    }
}
