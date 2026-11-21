#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateChangeEffects {
    pub changed: bool,
    pub release_pending: bool,
    pub reset_queue: bool,
}

pub fn gate_change_effects(same_host: bool, had_pending: bool) -> GateChangeEffects {
    if same_host {
        GateChangeEffects {
            changed: false,
            release_pending: false,
            reset_queue: false,
        }
    } else {
        GateChangeEffects {
            changed: true,
            release_pending: had_pending,
            reset_queue: true,
        }
    }
}

pub fn no_gate_answer(host_present: bool) -> bool {
    !host_present
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskResolution {
    AskNow,
    RetryLive,
    Allowed,
}

pub fn ask_now_resolution(gate_still_current: bool, replacement_present: bool) -> AskResolution {
    if gate_still_current {
        AskResolution::AskNow
    } else if replacement_present {
        AskResolution::RetryLive
    } else {
        AskResolution::Allowed
    }
}

pub fn host_error_answer() -> bool {
    false
}

pub fn settled_once(is_current_pending: bool) -> bool {
    is_current_pending
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_the_host_releases_a_stale_prompt_and_resets_the_queue() {
        assert_eq!(
            gate_change_effects(true, true),
            GateChangeEffects {
                changed: false,
                release_pending: false,
                reset_queue: false
            }
        );
        assert_eq!(
            gate_change_effects(false, true),
            GateChangeEffects {
                changed: true,
                release_pending: true,
                reset_queue: true
            }
        );
        assert_eq!(
            gate_change_effects(false, false),
            GateChangeEffects {
                changed: true,
                release_pending: false,
                reset_queue: true
            }
        );
    }

    #[test]
    fn no_gate_and_withdrawn_gate_answer_allowed() {
        assert!(no_gate_answer(false));
        assert!(!no_gate_answer(true));
        assert_eq!(ask_now_resolution(false, false), AskResolution::Allowed);
        assert_eq!(ask_now_resolution(false, true), AskResolution::RetryLive);
        assert_eq!(ask_now_resolution(true, false), AskResolution::AskNow);
        assert_eq!(ask_now_resolution(true, true), AskResolution::AskNow);
    }

    #[test]
    fn a_failing_host_keeps_the_tab_and_only_the_current_prompt_settles() {
        assert!(!host_error_answer());
        assert!(settled_once(true));
        assert!(!settled_once(false));
    }
}
