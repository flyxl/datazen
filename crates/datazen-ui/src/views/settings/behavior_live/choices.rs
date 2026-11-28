//! Pure validation for the currently mounted workflow-only cleanup form.
//! A bad finite draft must never default to a broader or unbounded deletion.

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode { Preset(u32), Custom, ClearAll }

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Selection { Finite(u32), ClearAll }

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Invalid { NoScope, Days }

const CUSTOM_MIN: u32 = 7;
const CUSTOM_MAX: u32 = 365;

fn custom_value(draft: &str) -> Option<u32> {
    let days = draft.trim().parse().ok()?;
    (CUSTOM_MIN..=CUSTOM_MAX).contains(&days).then_some(days)
}

/// Step only a valid finite value. Invalid text stays invalid until corrected,
/// never silently turning into a wider deletion plan.
pub(super) fn step_custom_days(draft: &str, increment: bool) -> Option<u32> {
    let days = custom_value(draft)?;
    Some(if increment { (days + 1).min(CUSTOM_MAX) } else { (days - 1).max(CUSTOM_MIN) })
}

pub(super) fn selection(selected: bool, mode: Mode, draft: &str) -> Result<Selection, Invalid> {
    if !selected { return Err(Invalid::NoScope); }
    match mode {
        Mode::ClearAll => Ok(Selection::ClearAll),
        Mode::Preset(days @ (7 | 30 | 90)) => Ok(Selection::Finite(days)),
        Mode::Preset(_) => Err(Invalid::Days),
        Mode::Custom => custom_value(draft).map(Selection::Finite).ok_or(Invalid::Days),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_input_obeys_the_real_store_floor_and_never_becomes_clear_all() {
        for value in ["", "0", "-8", "1", "6", "366", "1.5", "nan", "4294967296", "later"] {
            assert_eq!(selection(true, Mode::Custom, value), Err(Invalid::Days), "{value}");
        }
        for days in [7, 8, 12, 30, 90, 365] {
            assert_eq!(selection(true, Mode::Custom, &days.to_string()), Ok(Selection::Finite(days)));
        }
        assert_eq!(selection(true, Mode::Preset(1), "12"), Err(Invalid::Days));
        assert_eq!(selection(true, Mode::Preset(7), "bad"), Ok(Selection::Finite(7)));
        assert_eq!(selection(true, Mode::ClearAll, "7"), Ok(Selection::ClearAll));
    }

    #[test]
    fn keyboard_steps_clamp_without_repairing_invalid_drafts() {
        for (draft, increment, expected) in [
            ("30", false, Some(29)), ("30", true, Some(31)), ("7", false, Some(7)),
            ("365", true, Some(365)), ("6", true, None), ("366", false, None),
            ("1.5", true, None), ("", false, None), ("oops", false, None),
        ] {
            assert_eq!(step_custom_days(draft, increment), expected, "{draft}");
        }
    }

    #[test]
    fn no_scope_wins_even_when_clear_all_or_a_valid_preset_is_selected() {
        for mode in [Mode::ClearAll, Mode::Preset(30), Mode::Custom] {
            assert_eq!(selection(false, mode, "12"), Err(Invalid::NoScope));
        }
    }
}
