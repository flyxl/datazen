//! T068: MigrationJobFailureNotice — the fail-closed refusal notice.
//!
//! Probes the rendered element tree (role, key, i18n text, tone, copy detail,
//! conditional buttons) plus the pure classification helpers that mirror the
//! React `data-failure-kind` / `data-fail-closed` attributes rune cannot set.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::migration::migration_job_failure_notice::{
    is_fail_closed, MigrationJobFailureNotice, TransferRunFailure, TransferRunFailureKind,
};
use crate::i18n::t;
use crate::ui::kit::kit;

const PREFIX: &str = "migration-job";
const MESSAGE: &str = "backend refused: scope not proven";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct Probe {
    kind: TransferRunFailureKind,
    prefix: String,
    wire_review: bool,
    wire_dismiss: bool,
    review_calls: usize,
    dismiss_calls: usize,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            kind: TransferRunFailureKind::Other,
            prefix: PREFIX.to_owned(),
            wire_review: false,
            wire_dismiss: false,
            review_calls: 0,
            dismiss_calls: 0,
        }
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let failure = TransferRunFailure {
        kind: s.kind,
        message: MESSAGE.to_owned(),
    };
    let mut notice = MigrationJobFailureNotice::<Probe>::new(failure)
        .test_id_prefix(s.prefix.clone());
    if s.wire_review {
        notice = notice.on_review(|st: &mut Probe| st.review_calls += 1);
    }
    if s.wire_dismiss {
        notice = notice.on_dismiss(|st: &mut Probe| st.dismiss_calls += 1);
    }
    col().w_full().h_full().padding(16.0).child(notice.build(true))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

// kind, React data-failure-kind value, data-fail-closed, danger tone
const KINDS: [
    (TransferRunFailureKind, &str, bool, bool);
    5
] = [
    (TransferRunFailureKind::BackendScope, "backendScope", true, true),
    (TransferRunFailureKind::PipelineBudget, "pipelineBudget", true, false),
    (TransferRunFailureKind::PlanConsumed, "planConsumed", false, false),
    (TransferRunFailureKind::StalePlan, "stalePlan", false, false),
    (TransferRunFailureKind::Other, "other", false, true),
];

#[test]
fn all_five_kinds_render_exact_i18n_text_tone_and_fail_closed() {
    let k = kit(true);
    for (kind, name, fail_closed, danger) in KINDS {
        let mut rt = Runtime::new(
            Probe {
                kind,
                ..Default::default()
            },
            view,
            (960.0, 640.0),
            env(),
        );
        rt.frame(0.0);

        // Root is an alert with the React data-testid key.
        let root = element(&rt, &format!("{PREFIX}-failure"));
        assert_eq!(root.role, "alert", "{name}: root role");

        // Title/body use the exact ten i18n keys from the locale table.
        let title_key = format!("migration.failure.{name}.title");
        let title = element(&rt, &format!("{PREFIX}-failure-title"));
        assert_eq!(
            title.text.as_deref(),
            Some(t(&title_key)),
            "{name}: title uses {title_key}"
        );
        let body_key = format!("migration.failure.{name}.body");
        let body = element(&rt, &format!("{PREFIX}-failure-body"));
        assert_eq!(
            body.text.as_deref(),
            Some(t(&body_key)),
            "{name}: body uses {body_key}"
        );

        // Tone: backendScope/other danger, the other three warning.
        let expected_tone = if danger { k.c.danger.fg } else { k.c.warning.fg };
        assert_eq!(
            title.text_color,
            Some(expected_tone),
            "{name}: title tone (danger={danger})"
        );

        // Copyable detail carries the exact backend message + copy control.
        let detail = element(&rt, &format!("{PREFIX}-failure-detail-text"));
        assert_eq!(
            detail.text.as_deref(),
            Some(MESSAGE),
            "{name}: copyable detail message"
        );
        assert!(
            has_key(&rt, &format!("{PREFIX}-failure-detail-copy")),
            "{name}: copy control present"
        );

        // data-failure-kind / data-fail-closed mirrors (pure + keyed marker).
        assert_eq!(kind.as_str(), name, "{name}: kind as_str");
        assert_eq!(is_fail_closed(kind), fail_closed, "{name}: fail-closed");
        assert!(
            has_key(
                &rt,
                &format!("{PREFIX}-failure-attrs-{name}-{fail_closed}")
            ),
            "{name}: kind/fail-closed marker observable in the tree"
        );
    }
}

#[test]
fn re_review_is_shown_only_for_two_kinds_and_only_with_a_callback() {
    for (kind, name, _, _) in KINDS {
        let allowed = matches!(
            kind,
            TransferRunFailureKind::PlanConsumed | TransferRunFailureKind::StalePlan
        );
        let mut rt = Runtime::new(
            Probe {
                kind,
                wire_review: true,
                ..Default::default()
            },
            view,
            (960.0, 640.0),
            env(),
        );
        rt.frame(0.0);
        let key = format!("{PREFIX}-re-review");
        assert_eq!(has_key(&rt, &key), allowed, "{name}: re-review presence");
        if allowed {
            assert!(
                texts(&rt).iter().any(|x| x == t("migration.failure.reReview")),
                "{name}: re-review label is the i18n string"
            );
            click(&mut rt, &key);
            assert_eq!(rt.state.review_calls, 1, "{name}: press fires once");
            // A second press still lands on the same handler, never re-applies a plan.
            click(&mut rt, &key);
            assert_eq!(rt.state.review_calls, 2, "{name}: press keeps firing the callback");
        } else {
            assert_eq!(rt.state.review_calls, 0, "{name}: no handler reached");
        }
    }

    // Even an allowed kind mounts nothing when no callback is wired.
    let mut rt = Runtime::new(
        Probe {
            kind: TransferRunFailureKind::PlanConsumed,
            wire_review: false,
            ..Default::default()
        },
        view,
        (960.0, 640.0),
        env(),
    );
    rt.frame(0.0);
    assert!(
        !has_key(&rt, &format!("{PREFIX}-re-review")),
        "no re-review without a handler"
    );
}

#[test]
fn dismiss_is_conditional_and_fires_exactly_once() {
    let mut rt = Runtime::new(
        Probe {
            wire_dismiss: true,
            ..Default::default()
        },
        view,
        (960.0, 640.0),
        env(),
    );
    rt.frame(0.0);
    let key = format!("{PREFIX}-failure-dismiss");
    assert!(has_key(&rt, &key), "dismiss mounted with a handler");
    assert!(
        texts(&rt).iter().any(|x| x == t("common.dismiss")),
        "dismiss label is the i18n string"
    );
    click(&mut rt, &key);
    assert_eq!(rt.state.dismiss_calls, 1, "dismiss press fires once");

    let mut rt = Runtime::new(
        Probe {
            wire_dismiss: false,
            ..Default::default()
        },
        view,
        (960.0, 640.0),
        env(),
    );
    rt.frame(0.0);
    assert!(
        !has_key(&rt, &key),
        "no dismiss control when no handler is wired"
    );
    assert_eq!(rt.state.dismiss_calls, 0, "disconnected handler cannot fire");
}

#[test]
fn a_custom_test_id_prefix_is_honored() {
    let mut rt = Runtime::new(
        Probe {
            kind: TransferRunFailureKind::StalePlan,
            prefix: "sync-diff".to_owned(),
            wire_review: true,
            wire_dismiss: true,
            ..Default::default()
        },
        view,
        (960.0, 640.0),
        env(),
    );
    rt.frame(0.0);
    for key in [
        "sync-diff-failure",
        "sync-diff-failure-title",
        "sync-diff-failure-body",
        "sync-diff-failure-detail",
        "sync-diff-re-review",
        "sync-diff-failure-dismiss",
    ] {
        assert!(has_key(&rt, key), "expected custom-prefix key: {key}");
    }
    assert!(
        !has_key(&rt, &format!("{PREFIX}-failure")),
        "default prefix is not mounted alongside the custom one"
    );
}

#[test]
fn all_kinds_fit_both_window_sizes_with_both_buttons_wired() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for (kind, name, _, _) in KINDS {
            let mut rt = Runtime::new(
                Probe {
                    kind,
                    wire_review: true,
                    wire_dismiss: true,
                    ..Default::default()
                },
                view,
                (w, h),
                env(),
            );
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("failure notice {name} {w}x{h}"));
            // Wiring everything must not change which buttons are mounted.
            let review = matches!(
                kind,
                TransferRunFailureKind::PlanConsumed | TransferRunFailureKind::StalePlan
            );
            assert_eq!(
                has_key(&rt, &format!("{PREFIX}-re-review")),
                review,
                "{name}: re-review still gated at {w}x{h}"
            );
            assert!(
                has_key(&rt, &format!("{PREFIX}-failure-dismiss")),
                "{name}: dismiss present at {w}x{h}"
            );
        }
    }
}
