//! Bridge to `views::onboarding::onboarding_gate`'s inline tests (T268): the
//! first-run gate's suite lives inside the modules it tests (the pure-logic
//! convention); this wrapper lets the pruned-tree probe channel run it like
//! any other test module.

pub(crate) mod tests {
    pub(crate) use datazen_ui::views::onboarding::onboarding_gate::gate::tests::*;
    pub(crate) use datazen_ui::views::onboarding::onboarding_gate::state::tests::*;
}
