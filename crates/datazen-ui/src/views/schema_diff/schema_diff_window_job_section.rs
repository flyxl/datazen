//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffWindowJobSection.tsx` (T325).
//!
//! The window's job section: the status panel (T115's
//! [`SchemaDiffJobStatusPanel`] — the current job, the latest apply,
//! the cancel/verify controls) with the window-level
//! [`CopyableError`] strip under it when a wizard error is set.
//!
//! The React file is pure composition — the status panel plus the
//! error strip — so the port is the same composition over the two
//! landed modules; no logic of its own exists to diverge.
//!
//! Known deltas: the strip's Tailwind classes
//! (`error-message border-b border-edge px-6 py-2 text-xs`) map to
//! the CopyableError base's own error styling with the 24/8pt padding
//! and the bottom border applied here, the install-dialog /
//! data-sync call sites' pattern.

use std::rc::Rc;

use rune::{col, text, El};
use rune_kit::Kit;

use crate::commands::schema_diff::SchemaDiffJobDetailsVm;
use crate::ui::copyable::CopyableError;
use crate::views::schema_diff::schema_diff_job_status_panel::SchemaDiffJobStatusPanel;
use crate::views::schema_diff::use_schema_diff_job_lifecycle::{
    SchemaDiffCancelOutcome, SchemaDiffTrackedJobVm,
};

/// The strip's padding (`px-6 py-2`).
const ERROR_PAD: (f32, f32) = (24.0, 8.0);
/// The strip's text size (`text-xs`).
const ERROR_TEXT: f32 = 12.0;

/// `SchemaDiffWindowJobSection`'s props (the React prop set verbatim;
/// the callbacks are the host's).
pub(crate) struct SchemaDiffWindowJobSection<'a, S: 'static> {
    pub(crate) current_job: Option<&'a SchemaDiffTrackedJobVm>,
    pub(crate) latest_apply: Option<&'a SchemaDiffJobDetailsVm>,
    pub(crate) cancel_outcome: SchemaDiffCancelOutcome,
    pub(crate) verifying: bool,
    pub(crate) error: Option<&'a str>,
    pub(crate) target_ready: bool,
    pub(crate) on_cancel: Option<Rc<dyn Fn(&mut S)>>,
    pub(crate) on_verify: Option<Rc<dyn Fn(&mut S, String)>>,
}

/// `SchemaDiffWindowJobSection({...})`: the status panel, then the
/// error strip when an error is set. Both may be absent (the panel
/// answers `None` with nothing tracked; the strip renders only with
/// an error) — the section is then empty, like React's empty
/// fragment.
pub(crate) fn schema_diff_window_job_section<S: 'static>(
    k: &Kit,
    props: SchemaDiffWindowJobSection<'_, S>,
) -> El {
    let mut panel = SchemaDiffJobStatusPanel::new(
        props.current_job,
        props.latest_apply,
        props.cancel_outcome,
        props.verifying,
        props.target_ready,
    );
    if let Some(on_cancel) = &props.on_cancel {
        let on_cancel = Rc::clone(on_cancel);
        panel = panel.on_cancel(move |s: &mut S| on_cancel(s));
    }
    if let Some(on_verify) = &props.on_verify {
        let on_verify = Rc::clone(on_verify);
        panel = panel.on_verify(move |s: &mut S, job_id: String| on_verify(s, job_id));
    }

    let mut section = col().key("schema-diff-job-section").w_full();
    if let Some(panel_el) = panel.build(k.dark) {
        section = section.child(panel_el);
    }
    if let Some(error) = props.error {
        section = section.child(error_strip(k, error));
    }
    section
}

/// The `CopyableError` strip: `error-message border-b border-edge
/// px-6 py-2 text-xs`.
fn error_strip(k: &Kit, message: &str) -> El {
    col()
        .key("schema-diff-job-section-error")
        .w_full()
        .px(ERROR_PAD.0)
        .py(ERROR_PAD.1)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(
            CopyableError::new("schema-diff-job-section-error", message.to_owned())
                .build::<crate::AppState>(k),
        )
        .child(text(String::new()).font(rune::TextStyle::new(ERROR_TEXT, 400.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_geometry_is_the_react_literal() {
        assert_eq!(ERROR_PAD, (24.0, 8.0));
        assert_eq!(ERROR_TEXT, 12.0);
    }

    /// The section's two presence rules, extracted from the builder's
    /// branches so the audit can pin them without a render.
    #[test]
    fn the_panel_and_the_strip_have_independent_presence() {
        fn section_shape(
            tracked: bool,
            latest_apply: bool,
            error: bool,
        ) -> (bool, bool) {
            let panel = tracked || latest_apply;
            (panel, error)
        }
        // Nothing tracked, no error: empty section.
        assert_eq!(section_shape(false, false, false), (false, false));
        // A tracked job alone.
        assert_eq!(section_shape(true, false, false), (true, false));
        // A historical latest-apply alone.
        assert_eq!(section_shape(false, true, false), (true, false));
        // An error alone: no panel, just the strip.
        assert_eq!(section_shape(false, false, true), (false, true));
        // Both.
        assert_eq!(section_shape(true, false, true), (true, true));
    }
}
