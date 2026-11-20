//! Bridge to `views::onboarding::wizard_state`'s inline tests (T323): the
//! wizard-state machine's suite lives inside the module it tests, the pure-
//! logic convention; this wrapper lets the pruned-tree probe channel run it
//! like any other test module.

pub(crate) mod tests {
    pub(crate) use crate::views::onboarding::wizard_state::tests::*;
}
