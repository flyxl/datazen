//! The `types/dashboard.ts` refresh vocabulary the T201 drawer edits with:
//! the runtime constants, the preset / interval-unit grammars, and the
//! `secToUnitAndValue` derivation — ported verbatim (the future
//! `types/dashboard.ts` logic task rehomes them). The two `js_number`
//! helpers are the `Number()` semantics the drawer's number inputs parse
//! with.

use crate::views::dashboard::dashboard_panel::{RefreshModeVm, RefreshPolicyVm};

// ---- Refresh vocabulary (types/dashboard.ts, verbatim) ----------------------

/// React `MIN_REFRESH_SEC`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) const MIN_REFRESH_SEC: u32 = 30;
/// React `REFRESH_WARN_BELOW_SEC`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) const REFRESH_WARN_BELOW_SEC: u32 = 60;

/// React `clampRefreshSec`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn clamp_refresh_sec(n: u32) -> u32 {
    n.max(MIN_REFRESH_SEC)
}

/// React `normalizeRefreshPolicy`: an interval policy keeps a clamped
/// `refreshSec` (the minimum standing in for a missing one); every other
/// mode drops it.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn normalize_refresh_policy(refresh: &RefreshPolicyVm) -> RefreshPolicyVm {
    match refresh.mode {
        RefreshModeVm::Interval => RefreshPolicyVm {
            mode: RefreshModeVm::Interval,
            refresh_sec: Some(clamp_refresh_sec(
                refresh.refresh_sec.unwrap_or(MIN_REFRESH_SEC),
            )),
        },
        mode => RefreshPolicyVm { mode, refresh_sec: None },
    }
}

/// React `shouldWarnRefreshSec`: interval mode with a sub-warning interval.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn should_warn_refresh_sec(refresh: &RefreshPolicyVm) -> bool {
    matches!(refresh.mode, RefreshModeVm::Interval)
        && refresh.refresh_sec.is_some_and(|sec| sec < REFRESH_WARN_BELOW_SEC)
}

/// React's `RefreshPreset` — the preset vocabulary the refresh select shows.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefreshPreset {
    Manual,
    OnOpen,
    Daily,
    Hourly,
    Custom,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl RefreshPreset {
    /// The select's option order (React's inline options array).
    pub(crate) const ALL: [Self; 5] =
        [Self::Manual, Self::OnOpen, Self::Daily, Self::Hourly, Self::Custom];

    /// The select's option value (React's wire spelling).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::OnOpen => "onOpen",
            Self::Daily => "daily",
            Self::Hourly => "hourly",
            Self::Custom => "custom",
        }
    }

    /// Parse the select's value; unknown values fall back to `custom` (the
    /// select never offers one).
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "manual" => Self::Manual,
            "onOpen" => Self::OnOpen,
            "daily" => Self::Daily,
            "hourly" => Self::Hourly,
            _ => Self::Custom,
        }
    }

    /// The option's label key.
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Manual => "dashboard.refreshMode.manual",
            Self::OnOpen => "dashboard.refreshMode.onOpen",
            Self::Daily => "dashboard.refreshPreset.daily",
            Self::Hourly => "dashboard.refreshPreset.hourly",
            Self::Custom => "dashboard.refreshPreset.custom",
        }
    }
}

/// React's `refreshPolicyToPreset`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn refresh_policy_to_preset(mode: RefreshModeVm, sec: Option<u32>) -> RefreshPreset {
    match mode {
        RefreshModeVm::Manual => RefreshPreset::Manual,
        RefreshModeVm::OnOpen => RefreshPreset::OnOpen,
        RefreshModeVm::Interval => match sec {
            Some(86400) => RefreshPreset::Daily,
            Some(3600) => RefreshPreset::Hourly,
            _ => RefreshPreset::Custom,
        },
    }
}

/// React's `IntervalUnit`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntervalUnit {
    Days,
    Hours,
    Minutes,
    Seconds,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl IntervalUnit {
    /// The select's option order (React's inline options array).
    pub(crate) const ALL: [Self; 4] =
        [Self::Days, Self::Hours, Self::Minutes, Self::Seconds];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Days => "days",
            Self::Hours => "hours",
            Self::Minutes => "minutes",
            Self::Seconds => "seconds",
        }
    }

    /// Parse the select's value; unknown values fall back to `seconds`.
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "days" => Self::Days,
            "hours" => Self::Hours,
            "minutes" => Self::Minutes,
            _ => Self::Seconds,
        }
    }

    /// The option's label key.
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Days => "dashboard.intervalUnit.days",
            Self::Hours => "dashboard.intervalUnit.hours",
            Self::Minutes => "dashboard.intervalUnit.minutes",
            Self::Seconds => "dashboard.intervalUnit.seconds",
        }
    }

    /// React's `INTERVAL_MULTIPLIERS`.
    pub(crate) fn multiplier(self) -> u32 {
        match self {
            Self::Days => 86400,
            Self::Hours => 3600,
            Self::Minutes => 60,
            Self::Seconds => 1,
        }
    }
}

/// React's `secToUnitAndValue`: the largest unit that divides the interval
/// exactly (days → hours → minutes), else seconds.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn sec_to_unit_and_value(sec: u32) -> (IntervalUnit, u32) {
    if sec >= 86400 && sec % 86400 == 0 {
        (IntervalUnit::Days, sec / 86400)
    } else if sec >= 3600 && sec % 3600 == 0 {
        (IntervalUnit::Hours, sec / 3600)
    } else if sec >= 60 && sec % 60 == 0 {
        (IntervalUnit::Minutes, sec / 60)
    } else {
        (IntervalUnit::Seconds, sec)
    }
}

/// JS `Number(text)`: `None` for text that does not parse (JS's `NaN`);
/// empty and blank text are JS's `0`.
pub(crate) fn js_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(0.0);
    }
    trimmed.parse::<f64>().ok()
}

/// React's interval parse: `Math.max(1, Number(v) || 1)` — `0` and `NaN`
/// both fall to `1`.
pub(crate) fn js_number_or_one(text: &str) -> f64 {
    let n = js_number(text).unwrap_or(0.0);
    let n = if n == 0.0 { 1.0 } else { n };
    n.max(1.0)
}

