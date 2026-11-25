//! T306: bridge to `views::onboarding::use_detected_import_source`'s inline
//! suite (the pure-logic convention — the tests live inside the module they
//! test, `wizard_state.rs`'s bridge pattern). Every assertion comes from
//! re-reading the React hook: the pre-check order, the allSettled hits
//! filter and its degrade-to-nothing rule, the enabled effect's two
//! branches, and the `cancelled` round guard.

pub(crate) mod tests {
    pub(crate) use datazen_ui::views::onboarding::use_detected_import_source::tests::*;
}
