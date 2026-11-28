//! Pure validation for the currently mounted workflow-only cleanup form.
//! A bad finite draft must never default to a broader or unbounded deletion.

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode { Preset(u32), Custom, ClearAll }

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Selection { Finite(u32), ClearAll }

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Invalid { NoScope, Days }

pub(super) fn selection(selected: bool, mode: Mode, draft: &str) -> Result<Selection, Invalid> {
    if !selected { return Err(Invalid::NoScope); }
    match mode {
        Mode::ClearAll => Ok(Selection::ClearAll),
        Mode::Preset(days @ (7 | 30 | 90)) => Ok(Selection::Finite(days)),
        Mode::Preset(_) => Err(Invalid::Days),
        Mode::Custom => match draft.trim().parse::<u32>() {
            Ok(days @ 7..=365) => Ok(Selection::Finite(days)),
            _ => Err(Invalid::Days),
        },
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
    fn no_scope_wins_even_when_clear_all_or_a_valid_preset_is_selected() {
        for mode in [Mode::ClearAll, Mode::Preset(30), Mode::Custom] {
            assert_eq!(selection(false, mode, "12"), Err(Invalid::NoScope));
        }
    }
}
