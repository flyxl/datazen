//! Fail-closed refusal notice for the P5 migration tools.
//!
//! Ported 1:1 from
//! `packages/frontend/src/components/migration/MigrationJobFailureNotice.tsx`.
//! This is a refusal notice, not a soft failure: `backendScope` and
//! `pipelineBudget` are backend refusals that must never be offered as a
//! warning or a retry, and a consumed/stale plan must only be re-reviewed
//! (a new plan is minted) — never "apply the same plan again".
//!
//! The component currently has no production consumer; the Schema Diff / Data
//! Sync view tracks will wire it. Reusable API items therefore carry per-item
//! `#[allow(dead_code)]` (not file-level) until they are consumed.
//!
//! # Key convention
//!
//! React `data-testid` becomes the element key (the convention used across
//! this crate). With the default prefix `migration-job`:
//!
//! | part                  | key                                   |
//! |-----------------------|---------------------------------------|
//! | root (`role="alert"`) | `migration-job-failure`               |
//! | title                 | `migration-job-failure-title`         |
//! | body                  | `migration-job-failure-body`          |
//! | copyable detail       | `migration-job-failure-detail`        |
//! | re-review button      | `migration-job-re-review`             |
//! | dismiss button        | `migration-job-failure-dismiss`       |
//!
//! # Rune limitation: `data-*` attributes
//!
//! Rune has no API for arbitrary DOM-style `data-*` attributes (the same gap
//! the DataTable pagination port documents for `aria-busy`). The React root's
//! `data-failure-kind` and `data-fail-closed` are therefore preserved through
//! the two channels rune *does* expose:
//!
//! 1. [`TransferRunFailureKind::as_str`] and [`is_fail_closed`] are pure,
//!    directly testable mirrors of the two attributes.
//! 2. A zero-size, `a11y_hidden` keyed marker child
//!    `<prefix>-failure-attrs-<kind>-<fail_closed>` carries both values in the
//!    rendered element tree, so probes can read them via `ElementInfo::key`.
//!
//! No colour literal appears here: danger/warning tones come from `kit(dark)`
//! (`k.c.danger.fg` / `k.c.warning.fg`).

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, Kit};

use crate::i18n::t;
use crate::ui::copyable::CopyableError;

/// The five failure classifications, domain-free and shared by Schema Diff and
/// Data Sync (mirrors the React `TransferRunFailureKind` union).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum TransferRunFailureKind {
    BackendScope,
    PipelineBudget,
    PlanConsumed,
    StalePlan,
    Other,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl TransferRunFailureKind {
    /// The exact React `data-failure-kind` value and i18n key segment.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::BackendScope => "backendScope",
            Self::PipelineBudget => "pipelineBudget",
            Self::PlanConsumed => "planConsumed",
            Self::StalePlan => "stalePlan",
            Self::Other => "other",
        }
    }

    /// `TONE_CLASS` in React: backendScope/other are danger, the rest warning.
    pub(crate) fn is_danger(self) -> bool {
        matches!(self, Self::BackendScope | Self::Other)
    }
}

/// React's `data-fail-closed`: only backend refusals carry it. These are never
/// presented as a warning/retry — the flag is kept separate from the visual
/// tone on purpose (`pipelineBudget` is warning-toned *and* fail-closed).
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) fn is_fail_closed(kind: TransferRunFailureKind) -> bool {
    matches!(
        kind,
        TransferRunFailureKind::BackendScope | TransferRunFailureKind::PipelineBudget
    )
}

/// React's `offersReReview`: a consumed or stale plan may mint a new one; the
/// two backend refusals and `other` never may.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) fn offers_re_review(kind: TransferRunFailureKind) -> bool {
    matches!(
        kind,
        TransferRunFailureKind::PlanConsumed | TransferRunFailureKind::StalePlan
    )
}

/// View model for one failure (React `TransferRunFailure`).
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct TransferRunFailure {
    pub kind: TransferRunFailureKind,
    pub message: String,
}

/// i18n title key for a kind (`titleKey` in React).
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) fn title_key(kind: TransferRunFailureKind) -> String {
    format!("migration.failure.{}.title", kind.as_str())
}

/// i18n body key for a kind (`bodyKey` in React).
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) fn body_key(kind: TransferRunFailureKind) -> String {
    format!("migration.failure.{}.body", kind.as_str())
}

type Callback<S> = Box<dyn Fn(&mut S) + 'static>;

/// Builder for the fail-closed refusal notice.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct MigrationJobFailureNotice<S: 'static> {
    failure: TransferRunFailure,
    test_id_prefix: String,
    on_review: Option<Callback<S>>,
    on_dismiss: Option<Callback<S>>,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl<S: 'static> MigrationJobFailureNotice<S> {
    pub(crate) fn new(failure: TransferRunFailure) -> Self {
        Self {
            failure,
            test_id_prefix: "migration-job".to_owned(),
            on_review: None,
            on_dismiss: None,
        }
    }

    pub(crate) fn test_id_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.test_id_prefix = prefix.into();
        self
    }

    /// Mint a NEW plan. Never "apply the same plan again". Only mounted for
    /// `planConsumed` / `stalePlan` and only when a callback is present.
    pub(crate) fn on_review(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_review = Some(Box::new(f));
        self
    }

    pub(crate) fn on_dismiss(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss = Some(Box::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k: &'static Kit = crate::ui::kit::kit(dark);
        let MigrationJobFailureNotice {
            failure,
            test_id_prefix,
            on_review,
            on_dismiss,
        } = self;

        let root_key = format!("{test_id_prefix}-failure");
        let kind = failure.kind;

        // Title tone: danger for backendScope/other, warning for the rest.
        let tone = if kind.is_danger() {
            k.c.danger.fg
        } else {
            k.c.warning.fg
        };

        let mut root = col()
            .key(root_key.clone())
            .role("alert")
            .w_full()
            .gap(8.0)
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_02)
            .padding(16.0);

        // Rune has no data-* API: mirror data-failure-kind / data-fail-closed
        // as a zero-size, hidden keyed marker (see module doc).
        root = root.child(
            row()
                .key(format!(
                    "{root_key}-attrs-{}-{}",
                    kind.as_str(),
                    is_fail_closed(kind)
                ))
                .size(0.0, 0.0)
                .absolute(0.0, 0.0)
                .a11y_hidden(),
        );

        root = root.child(
            text(t(&title_key(kind)))
                .key(format!("{root_key}-title"))
                .font(k.ty.ui_medium)
                .color(tone)
                .wrap(),
        );

        root = root.child(
            text(t(&body_key(kind)))
                .key(format!("{root_key}-body"))
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .wrap(),
        );

        root = root.child(
            CopyableError::new(format!("{root_key}-detail"), failure.message)
                .copy_button(true)
                .build::<S>(k),
        );

        // Re-review is a retry-shaped affordance: only the two plan-related
        // kinds may offer it, and only when the caller wired a handler.
        if offers_re_review(kind) {
            if let Some(on_review) = on_review {
                let btn = Button::new(t("migration.failure.reReview"))
                    .variant(ButtonVariant::Contrast)
                    .on_press(move |s: &mut S| on_review(s))
                    .build(k);
                root = root.child(btn.key(format!("{test_id_prefix}-re-review")));
            }
        }

        if let Some(on_dismiss) = on_dismiss {
            let btn = Button::new(t("common.dismiss"))
                .variant(ButtonVariant::Ghost)
                .on_press(move |s: &mut S| on_dismiss(s))
                .build(k);
            root = root.child(btn.key(format!("{root_key}-dismiss")));
        }

        root
    }
}
