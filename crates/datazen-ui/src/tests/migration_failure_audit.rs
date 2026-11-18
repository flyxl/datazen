//! Independent audit for T068 (`MigrationJobFailureNotice`).
//!
//! Coverage-driven tests for the branches the port's own tests leave open:
//!
//! 1. all ten per-kind i18n keys (plus the two button labels) actually resolve
//!    to localized text — the port asserts `rendered == t(key)`, which is a
//!    vacuous comparison when `t` echo-falls-back on a missing key;
//! 2. component-owned keys are stable and never duplicated (`#dupN`);
//! 3. rect-based geometry at 960×640 and 1440×816 in BOTH themes, including a
//!    long adversarial detail message (the port only renders dark + a short
//!    message);
//! 4. a fail-closed refusal never carries a re-review affordance or leaks the
//!    backend message into an element key.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::migration::migration_job_failure_notice::{
    is_fail_closed, offers_re_review, MigrationJobFailureNotice, TransferRunFailure,
    TransferRunFailureKind,
};
use crate::i18n::{has, t};
use crate::ui::kit::kit;

const PREFIX: &str = "migration-job";

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

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct AuditProbe {
    kind: TransferRunFailureKind,
    message: String,
    dark: bool,
    wire_review: bool,
    wire_dismiss: bool,
    review_calls: usize,
    dismiss_calls: usize,
}

impl Default for AuditProbe {
    fn default() -> Self {
        Self {
            kind: TransferRunFailureKind::Other,
            message: "backend refused: proof missing".to_owned(),
            dark: true,
            wire_review: false,
            wire_dismiss: false,
            review_calls: 0,
            dismiss_calls: 0,
        }
    }
}

fn view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    let failure = TransferRunFailure {
        kind: s.kind,
        message: s.message.clone(),
    };
    let mut notice = MigrationJobFailureNotice::<AuditProbe>::new(failure)
        .test_id_prefix(PREFIX);
    if s.wire_review {
        notice = notice.on_review(|st: &mut AuditProbe| st.review_calls += 1);
    }
    if s.wire_dismiss {
        notice = notice.on_dismiss(|st: &mut AuditProbe| st.dismiss_calls += 1);
    }
    col().w_full().h_full().padding(8.0).child(notice.build(s.dark))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn key_count<S>(rt: &Runtime<S>, key: &str) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some(key))
        .count()
}

fn is_cjk(ch: char) -> bool {
    (0x4E00..=0x9FFF).contains(&(ch as u32))
}

/// Rect-based observation: every laid-out node must sit inside the window.
/// This reads runtime-computed rects, never a value the component reports.
fn assert_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn all_ten_per_kind_i18n_keys_and_both_button_labels_resolve_to_localized_text() {
    for (kind, name, _, _) in KINDS {
        for part in ["title", "body"] {
            let key = format!("migration.failure.{name}.{part}");
            assert!(has(&key), "{key} must exist in the zh-CN locale table");
            let text = t(&key);
            assert_ne!(text, key, "{key} must not echo the key (missing translation)");
            assert!(
                text.chars().any(is_cjk),
                "{key} must resolve to localized CJK text, got {text:?}"
            );
        }
        // The pure key builders must agree with the per-kind key spelling.
        assert_eq!(
            crate::components::migration::migration_job_failure_notice::title_key(kind),
            format!("migration.failure.{name}.title")
        );
        assert_eq!(
            crate::components::migration::migration_job_failure_notice::body_key(kind),
            format!("migration.failure.{name}.body")
        );
    }
    for key in ["migration.failure.reReview", "common.dismiss"] {
        assert!(has(key), "{key} must exist in the zh-CN locale table");
        assert_ne!(t(key), key, "{key} must not echo the key");
        assert!(t(key).chars().any(is_cjk), "{key} must be localized text");
    }
}

#[test]
fn rendered_title_and_body_are_localized_text_not_key_echo() {
    for (kind, name, _, _) in KINDS {
        let mut rt = Runtime::new(
            AuditProbe {
                kind,
                ..Default::default()
            },
            view,
            (960.0, 640.0),
            env(),
        );
        rt.frame(0.0);
        for (part, suffix) in [("title", "title"), ("body", "body")] {
            let key = format!("migration.failure.{name}.{part}");
            let node = rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(&format!("{PREFIX}-failure-{suffix}")))
                .unwrap_or_else(|| panic!("{PREFIX}-failure-{suffix} laid out"));
            let shown = node.text.unwrap_or_default();
            assert_eq!(shown, t(&key), "{name}: rendered {part} equals locale text");
            assert_ne!(shown, key, "{name}: rendered {part} is not a key echo");
            assert!(shown.chars().any(is_cjk), "{name}: rendered {part} is localized");
        }
    }
}

#[test]
fn component_owned_keys_are_stable_and_never_duplicated() {
    // StalePlan with every callback wired mounts the maximum number of owned
    // keys: root/marker/title/body/detail/detail-text/detail-copy/re-review/dismiss.
    let mut rt = Runtime::new(
        AuditProbe {
            kind: TransferRunFailureKind::StalePlan,
            wire_review: true,
            wire_dismiss: true,
            ..Default::default()
        },
        view,
        (960.0, 640.0),
        env(),
    );
    rt.frame(0.0);

    let owned = [
        format!("{PREFIX}-failure"),
        format!("{PREFIX}-failure-attrs-stalePlan-false"),
        format!("{PREFIX}-failure-title"),
        format!("{PREFIX}-failure-body"),
        format!("{PREFIX}-failure-detail"),
        format!("{PREFIX}-failure-detail-text"),
        format!("{PREFIX}-failure-detail-copy"),
        format!("{PREFIX}-re-review"),
        format!("{PREFIX}-failure-dismiss"),
    ];
    for key in &owned {
        assert_eq!(key_count(&rt, key), 1, "owned key appears exactly once: {key}");
    }
    // The runtime suffixes later duplicate keys with #dupN; none of our owned
    // keys may be so renamed (kit-internal hidden `label`/`icon` are out of scope).
    for e in rt.elements() {
        if let Some(k) = e.key.as_deref() {
            if owned.iter().any(|o| k.starts_with(o.as_str())) {
                assert!(!k.contains("#dup"), "component key was duplicated: {k}");
            }
        }
    }
}

#[test]
fn geometry_holds_in_both_themes_and_both_sizes_with_a_long_detail_message() {
    // Long, unbroken, token-like content stresses wrap_anywhere on the detail
    // and guards against a horizontal overflow that a short message hides.
    let long_message = format!(
        "error=SECRET_TOKEN_{} path={}",
        "A".repeat(120),
        "/datazen/very/long/nested/path/to/a/job/artifact".repeat(4)
    );
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            for (kind, name, _, _) in KINDS {
                let mut rt = Runtime::new(
                    AuditProbe {
                        kind,
                        message: long_message.clone(),
                        dark,
                        wire_review: true,
                        wire_dismiss: true,
                        ..Default::default()
                    },
                    view,
                    (w, h),
                    env(),
                );
                rt.frame(0.0);
                assert_inside(&rt, w, h, &format!("{name} dark={dark} {w}x{h}"));
                // The message must never be embedded into an element key.
                for e in rt.elements() {
                    if let Some(k) = e.key.as_deref() {
                        assert!(
                            !k.contains("SECRET_TOKEN"),
                            "backend message leaked into an element key: {k}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn fail_closed_refusals_never_offer_or_fire_a_re_review() {
    let k = kit(true);
    for (kind, name, fail_closed, danger) in KINDS {
        let mut rt = Runtime::new(
            AuditProbe {
                kind,
                wire_review: true,
                ..Default::default()
            },
            view,
            (960.0, 640.0),
            env(),
        );
        rt.frame(0.0);
        let expected = offers_re_review(kind);
        assert_eq!(!fail_closed || !expected, true, "{name}: a fail-closed refusal offers re-review");
        assert_eq!(has_key(&rt, &format!("{PREFIX}-re-review")), expected, "{name}: re-review mount");
        if !expected {
            // No affordance to click; the callback can never be reached.
            assert_eq!(rt.state.review_calls, 0, "{name}: no re-review handler fired");
        }

        // Tone polarity: danger tokens for backendScope/other, warning for rest.
        let title = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(&format!("{PREFIX}-failure-title")))
            .unwrap();
        let expected_tone = if danger { k.c.danger.fg } else { k.c.warning.fg };
        assert_eq!(title.text_color, Some(expected_tone), "{name}: tone");
        // A fail-closed flag must be independent of the visual tone: pipelineBudget
        // is the one warning-toned refusal that is still fail-closed.
        assert_eq!(is_fail_closed(kind), fail_closed, "{name}: fail-closed mirror");
    }
}
