//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffWizardProgress.tsx` (T339).
//!
//! The wizard's step rail: the five steps with their numbered
//! circles and chevrons, the current step accented, the completed
//! ones at 80% accent, the future ones muted. The type
//! ([`SchemaDiffWizardStep`]) and the two step lists
//! ([`SCHEMA_DIFF_STEPS`] / [`SCHEMA_DIFF_NARROW_STEPS`]) live here
//! because the React file owns them (the navigation bar imports the
//! step type from this module).
//!
//! ## What maps where
//!
//! - `SCHEMA_DIFF_STEPS.indexOf(step)` is [`SchemaDiffWizardStep::
//!   index_in`]; an unlisted step answers `None` exactly as React's
//!   `-1` (a future-proofing arm no caller exercises).
//! - The rail's geometry (the 24/12pt padding band, the 896pt
//!   `max-w-4xl`, the 4pt gaps, the 20pt numbered circles at the
//!   10px digit, the 12px step labels) is the builder below; the
//!   three-tone state mapping (`bg-accent`/`bg-accent/20`/
//!   `bg-surface-raised` circles over `text-accent`/`text-accent/80`/
//!   `text-fg-muted` labels) reads the same index comparison the
//!   React ternaries do.
//! - `data-testid={`schema-diff-step-${item}`}` stays verbatim as the
//!   per-step key.
//!
//! Known deltas: `flex-wrap` is the flex row's natural wrap; the
//! chevrons between steps are the kit's muted glyph pair (3pt square)
//! — React `lucide-react`'s `ChevronRight` at `h-3 w-3`.

use rune::{col, row, text, tok, El};
use rune_kit::Kit;

use crate::i18n::t;

/// `SchemaDiffWizardStep`: the wizard's five steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaDiffWizardStep {
    Endpoints,
    Objects,
    Compare,
    Plan,
    Deploy,
}

impl SchemaDiffWizardStep {
    /// The React spelling (`'endpoints' | 'objects' | …`).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Endpoints => "endpoints",
            Self::Objects => "objects",
            Self::Compare => "compare",
            Self::Plan => "plan",
            Self::Deploy => "deploy",
        }
    }

    /// `SCHEMA_DIFF_STEPS.indexOf(step)`: the step's position in the
    /// full rail, or `None` when unlisted.
    pub(crate) fn index_in(self, steps: &[SchemaDiffWizardStep]) -> Option<usize> {
        steps.iter().position(|candidate| *candidate == self)
    }

    /// `t(\`schemaDiff.step.${item}\`)`'s key.
    pub(crate) fn label_key(self) -> String {
        format!("schemaDiff.step.{}", self.as_str())
    }
}

/// `SCHEMA_DIFF_STEPS`.
pub(crate) const SCHEMA_DIFF_STEPS: [SchemaDiffWizardStep; 5] = [
    SchemaDiffWizardStep::Endpoints,
    SchemaDiffWizardStep::Objects,
    SchemaDiffWizardStep::Compare,
    SchemaDiffWizardStep::Plan,
    SchemaDiffWizardStep::Deploy,
];

/// `SCHEMA_DIFF_NARROW_STEPS` (the narrow window's three steps).
pub(crate) const SCHEMA_DIFF_NARROW_STEPS: [SchemaDiffWizardStep; 3] = [
    SchemaDiffWizardStep::Endpoints,
    SchemaDiffWizardStep::Objects,
    SchemaDiffWizardStep::Deploy,
];

/// The band's vertical padding (`py-3`) and horizontal padding
/// (`px-6`).
const BAND_PAD: (f32, f32) = (24.0, 12.0);
/// `max-w-4xl`.
const RAIL_MAX_WIDTH: f32 = 896.0;
/// `gap-1` between the step groups.
const STEP_GAP: f32 = 4.0;
/// The numbered circle (`h-5 w-5`) and its 10px digit.
const CIRCLE: f32 = 20.0;
const CIRCLE_TEXT: f32 = 10.0;
/// The step label (`text-xs`).
const LABEL_TEXT: f32 = 12.0;

/// `SchemaDiffWizardProgress({ step })`.
pub(crate) fn schema_diff_wizard_progress(k: &Kit, step: SchemaDiffWizardStep) -> El {
    let step_index = step.index_in(&SCHEMA_DIFF_STEPS);
    let mut rail = row()
        .key("schema-diff-progress")
        .max_w(RAIL_MAX_WIDTH)
        .gap(STEP_GAP)
        .center()
        .flex_wrap();
    for (index, item) in SCHEMA_DIFF_STEPS.iter().enumerate() {
        let mut group = row().gap(STEP_GAP).center_y();
        if index > 0 {
            // The chevron between steps (`h-3 w-3 text-fg-muted`).
            group = group.child(
                text("›".to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.text_muted)
                    .key("schema-diff-step-chevron"),
            );
        }
        // The three-tone label: current accent semibold, completed
        // 80% accent, future muted.
        let label_color = match step_index {
            Some(current) if index == current => k.c.text_accent,
            Some(current) if index < current => k.c.text_accent.alpha(0.8),
            _ => k.c.text_muted,
        };
        let circle_fill = match step_index {
            Some(current) if index == current => k.c.bg_accent,
            Some(current) if index < current => k.c.bg_accent.alpha(0.2),
            _ => k.c.layer_02, // bg-surface-raised
        };
        let circle_ink = match step_index {
            Some(current) if index == current => k.c.text_contrast, // text-on-accent
            Some(current) if index < current => k.c.text_accent,
            _ => k.c.text_muted,
        };
        let badge = row()
            .key(format!("schema-diff-step-{}", item.as_str()))
            .size(CIRCLE, CIRCLE)
            .center()
            .radius(CIRCLE / 2.0)
            .fill(circle_fill)
            .child(
                text((index + 1).to_string())
                    .font(rune::TextStyle::new(CIRCLE_TEXT, 600.0))
                    .color(circle_ink),
            );
        group = group.child(
            row()
                .gap(6.0)
                .center_y()
                .radius(999.0)
                .px(8.0)
                .py(2.0)
                .child(badge)
                .child(
                    text(t(&item.label_key()).to_owned())
                        .font(rune::TextStyle::new(LABEL_TEXT, 500.0))
                        .color(label_color),
                ),
        );
        rail = rail.child(group);
    }
    col()
        .key("schema-diff-progress-band")
        .w_full()
        .px(BAND_PAD.0)
        .py(BAND_PAD.1)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(rail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_full_rail_is_the_five_steps_in_order() {
        assert_eq!(
            SCHEMA_DIFF_STEPS.map(|step| step.as_str()),
            ["endpoints", "objects", "compare", "plan", "deploy"]
        );
    }

    #[test]
    fn the_narrow_rail_drops_compare_and_plan() {
        assert_eq!(
            SCHEMA_DIFF_NARROW_STEPS.map(|step| step.as_str()),
            ["endpoints", "objects", "deploy"]
        );
        // The narrow list is a subsequence of the full one.
        for step in SCHEMA_DIFF_NARROW_STEPS {
            assert!(step.index_in(&SCHEMA_DIFF_STEPS).is_some());
        }
    }

    #[test]
    fn index_in_answers_the_position() {
        assert_eq!(
            SchemaDiffWizardStep::Endpoints.index_in(&SCHEMA_DIFF_STEPS),
            Some(0)
        );
        assert_eq!(SchemaDiffWizardStep::Deploy.index_in(&SCHEMA_DIFF_STEPS), Some(4));
        // The compare step is not in the narrow rail.
        assert_eq!(SchemaDiffWizardStep::Compare.index_in(&SCHEMA_DIFF_NARROW_STEPS), None);
    }

    #[test]
    fn the_label_keys_are_the_react_literals() {
        assert_eq!(
            SchemaDiffWizardStep::Endpoints.label_key(),
            "schemaDiff.step.endpoints"
        );
        assert_eq!(SchemaDiffWizardStep::Deploy.label_key(), "schemaDiff.step.deploy");
        // The keys translate (the packs carry all five).
        for step in SCHEMA_DIFF_STEPS {
            assert!(crate::i18n::has(&step.label_key()), "{} is untranslated", step.label_key());
        }
    }

    #[test]
    fn the_geometry_is_the_react_literal() {
        assert_eq!(BAND_PAD, (24.0, 12.0));
        assert_eq!(RAIL_MAX_WIDTH, 896.0);
        assert_eq!(STEP_GAP, 4.0);
        assert_eq!(CIRCLE, 20.0);
        assert_eq!(CIRCLE_TEXT, 10.0);
        assert_eq!(LABEL_TEXT, 12.0);
    }
}
