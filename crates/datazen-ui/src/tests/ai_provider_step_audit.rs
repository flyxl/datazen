//! Independent audit for T148 (the AiProviderStep Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/steps/AiProviderStep.tsx` (plus
//! the `StepTag` export of `WelcomeStep.tsx` it imports and the wizard's
//! `{state.step === 's2' && <AiProviderStep draft={aiDraft} />}` mount):
//! the `w-full max-w-[680px]` step column, the accent step-tag pill, the
//! heading / subtitle / security-note / optional-hint i18n, the shared
//! `ModelProfileForm` mounted in its `inline` variant inside the bordered
//! `rounded-xl border border-edge bg-surface p-5` card (no Cancel/Save —
//! the journey footer's Finish owns the primary action), the accent
//! security note with its 15px Info glyph, and the journey link itself:
//! typing credentials into the step's form flips `aiDraft.hasCredentials`,
//! the branch the wizard's `handleFinish` reads. Runtime evidence comes
//! from `Runtime::elements()` (runtime layout) with the step mounted
//! exactly as production mounts it — as the wizard's S2 seat inside the
//! T072 window — never from a value the component reports about itself.
//! The form's own field/tab/safety behaviour is audited by
//! `model_profile_form_audit` (T107); this file pins the step framing and
//! the composition seam.

use datazen_ui_api::{ModelInfo, ProviderListItem};
use rune::{Cx, El, Env, Runtime, TextEdit};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::ai::model_profile_form::ModelFormSelect;
use crate::views::ai::model_profile_form::ModelProfileFormSlots;
use crate::views::ai::use_model_profile_draft::{
    provider_preset, ModelProfileDraft, SaveRequest, PROVIDER_ORDER,
};
use crate::views::onboarding::onboarding_window::OnboardingWindow;
use crate::views::onboarding::onboarding_wizard::{Entry, FinishPath, OnboardingWizard, Step};
use crate::views::onboarding::steps::ai_provider_step::{AiProviderStep, STEP_MAX_W};

/// The default onboarding window's width and a height that fits the whole
/// step unscrolled (the T021/T072 audits' 960-wide default; the form audit's
/// 1400-tall page, so every part lays out at full size).
const W: f32 = 960.0;
const H: f32 = 1400.0;

#[derive(Clone)]
struct Scene {
    step: Step,
    dark: bool,
    draft: ModelProfileDraft,
    open: Option<ModelFormSelect>,
    validating: bool,
    config_error: Option<String>,
    fetching: bool,
    remote_models: Vec<ModelInfo>,
    providers: Vec<ProviderListItem>,
    /// The clock the caller reads the 3s badge deadline against.
    now: f64,
    validates: usize,
    saves: Vec<&'static str>,
}

fn base() -> Scene {
    let providers: Vec<ProviderListItem> =
        PROVIDER_ORDER.iter().map(|ty| provider_preset(*ty)).collect();
    let mut draft = ModelProfileDraft::default();
    // React's wizard: `useModelProfileDraft({ profile: null, … })` — the
    // fresh profile the journey marks as the default one on save.
    draft.reseed(None, "journey-1", &providers);
    Scene {
        step: Step::S2,
        dark: true,
        draft,
        open: None,
        validating: false,
        config_error: None,
        fetching: false,
        remote_models: Vec::new(),
        providers,
        now: 100.0,
        validates: 0,
        saves: Vec::new(),
    }
}

/// The wizard's own slot wiring over its one draft machine — the same list
/// the settings form reads (the T107/T479 contract this step composes).
fn slots() -> ModelProfileFormSlots<Scene> {
    ModelProfileFormSlots {
        draft: |s: &Scene| &s.draft,
        draft_mut: |s: &mut Scene| &mut s.draft,
        open: |s: &Scene| s.open,
        set_open: |s: &mut Scene, which| s.open = which,
        validating: |s: &Scene| s.validating,
        config_error: |s: &Scene| s.config_error.clone(),
        fetching_remote_models: |s: &Scene| s.fetching,
        remote_models: |s: &Scene| &s.remote_models,
        providers: |s: &Scene| &s.providers,
        validate_ok: |s: &Scene| s.draft.validate_ok(s.now),
        on_set_tab: |s: &mut Scene, tab| s.draft.tab = tab,
        on_provider_change: |s: &mut Scene, provider_type| {
            s.draft.handle_provider_change(provider_type, &s.providers)
        },
        on_protocol_change: |s: &mut Scene, value| {
            s.draft.handle_protocol_change(&value);
        },
        on_fetch_models: |s: &mut Scene| s.fetching = !s.fetching,
        on_toggle_manual_model: |s: &mut Scene| {
            s.draft.manual_model_input = !s.draft.manual_model_input;
        },
        on_model_change: |s: &mut Scene, value| {
            s.draft.model = TextEdit::new(value);
        },
        on_validate: |s: &mut Scene| s.validates += 1,
        on_apply_preset: |s: &mut Scene, kind| s.draft.apply_preset(kind),
        on_egress_change: |s: &mut Scene, level| s.draft.set_gate_data_egress(level),
        on_gate_max_sample_rows: |s: &mut Scene, rows| {
            s.draft.set_gate_max_sample_rows(rows)
        },
        on_gate_max_tool_rounds: |s: &mut Scene, rounds| {
            s.draft.set_gate_max_tool_rounds(rounds)
        },
        on_gate_api_retries: |s: &mut Scene, retries| {
            s.draft.set_gate_api_retries(retries)
        },
        on_gate_max_tool_tokens: |s: &mut Scene, tokens| {
            s.draft.set_gate_max_tool_tokens(tokens)
        },
        on_gate_max_tool_result_bytes: |s: &mut Scene, bytes| {
            s.draft.set_gate_max_tool_result_bytes(bytes)
        },
        on_gate_max_tool_duration_secs: |s: &mut Scene, secs| {
            s.draft.set_gate_max_tool_duration_secs(secs)
        },
        on_db_policy_change: |s: &mut Scene, policy| {
            s.draft.set_gate_db_tool_policy(policy)
        },
        on_mcp_policy_change: |s: &mut Scene, policy| {
            s.draft.set_gate_mcp_tool_policy(policy)
        },
        on_toggle_default: |s: &mut Scene| s.draft.is_default = !s.draft.is_default,
        on_toggle_sql_confirm: |s: &mut Scene| {
            let next = !s.draft.safety_gate.require_sql_confirm;
            s.draft.set_gate_require_sql_confirm(next);
        },
        on_cancel: |_s: &mut Scene| {}, // the inline variant renders no cancel
        on_save: |s: &mut Scene| match s.draft.request_save(&s.providers) {
            SaveRequest::ConfirmHighRisk => {}
            SaveRequest::Save(_) => s.saves.push("save"),
        },
        on_cancel_high_risk: |s: &mut Scene| s.draft.show_high_risk_modal = false,
        on_confirm_high_risk: |s: &mut Scene| {
            if let SaveRequest::Save(_) = s.draft.confirm_high_risk(&s.providers) {
                s.saves.push("confirm");
            }
        },
        name_edit: |s: &mut Scene| &mut s.draft.name,
        api_key_edit: |s: &mut Scene| &mut s.draft.api_key,
        endpoint_edit: |s: &mut Scene| &mut s.draft.endpoint,
        model_edit: |s: &mut Scene| &mut s.draft.model,
        max_tokens_edit: |s: &mut Scene| &mut s.draft.max_tokens,
        cli_path_edit: |s: &mut Scene| &mut s.draft.local_cli_path,
        cli_workspace_edit: |s: &mut Scene| &mut s.draft.local_cli_workspace,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    // The step the wizard hands its draft machine, built per frame.
    let step = AiProviderStep::new(slots()).build(s, s.dark, &mut layer);
    // The production mounting: the wizard's S2 seat inside the T072 window
    // (the seat itself gates on the step, exactly as React's conditional).
    let wizard = OnboardingWizard::<Scene>::new()
        .step(s.step)
        .entry(Entry::Manual)
        .ai(step);
    OnboardingWindow::<Scene>::new().wizard(wizard.build()).build(s.dark)
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (W, H),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<Scene>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// The first text node under the element keyed `key` (the pill's unkeyed
/// label — the `model_profile_form_audit` helper).
fn text_under(rt: &Runtime<Scene>, key: &str) -> String {
    let Some(el) = maybe(rt, key) else {
        return String::new();
    };
    let prefix = el.path;
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.role == "text" && e.path.starts_with(&prefix))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

/// The first text element under the element keyed `key`, for ink asserts.
fn text_el_under(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    let el = maybe(rt, key)?;
    let prefix = el.path;
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.role == "text" && e.path.starts_with(&prefix))
}

/// Advance the clock past the kit's enter/hover motion (~0.4s) so freshly
/// mounted parts read their full geometry (the sibling audits' helper).
fn settle(rt: &mut Runtime<Scene>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn type_into(rt: &mut Runtime<Scene>, key: &str, text: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.key(rune::KeyEvent {
        key: rune::Key::Command('a'),
        mods: rune::Mods { cmd: true, ..rune::Mods::default() },
    });
    rt.frame(0.0);
    rt.paste(text);
    rt.frame(0.0);
}

// ---- the wizard's S2 mount ---------------------------------------------------

/// React: `{state.step === 's2' && <AiProviderStep draft={aiDraft} />}` —
/// the step (and its form) exist only on S2, inside the wizard's S2 seat.
#[test]
fn the_step_renders_only_on_s2() {
    for step in [Step::S0, Step::S1, Step::S3] {
        let rt = rt(Scene { step, ..base() });
        assert!(!has(&rt, "onboarding-step-s2-ai"), "{step:?} does not mount the AI step");
        assert!(!has(&rt, "onboarding-ai-form"), "{step:?} does not mount the form either");
    }
    let rt = rt(Scene { step: Step::S2, ..base() });
    let root = maybe(&rt, "onboarding-step-s2-ai").expect("S2 mounts the AI step");
    let seat = maybe(&rt, "onboarding-wizard-step-s2").expect("the wizard's S2 seat");
    assert!(
        root.path.starts_with(&seat.path),
        "the step is mounted inside the wizard's S2 seat"
    );
}

// ---- the step column ---------------------------------------------------------

/// React root: `w-full max-w-[680px]` — full width up to the 680px (510pt)
/// cap; the card and the note fill the capped column.
#[test]
fn the_step_column_caps_at_680px() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let root = maybe(&rt, "onboarding-step-s2-ai").expect("the step root");
        assert!(
            (root.rect[2] - STEP_MAX_W).abs() < 0.5,
            "dark={dark}: the column caps at 680px = {}pt, not {}",
            STEP_MAX_W,
            root.rect[2]
        );
        let card = maybe(&rt, "onboarding-step-s2-ai-card").expect("the form card");
        let note = maybe(&rt, "onboarding-step-s2-ai-security-note").expect("the note");
        assert_eq!(card.rect[0], root.rect[0], "dark={dark}: the card is left-aligned");
        assert!((card.rect[2] - STEP_MAX_W).abs() < 0.5, "dark={dark}: the card fills the column");
        assert!((note.rect[2] - STEP_MAX_W).abs() < 0.5, "dark={dark}: the note fills the column");
        // The column starts at the seat's left edge (React's content row
        // left-aligns its single child).
        let seat = maybe(&rt, "onboarding-wizard-step-s2").expect("the S2 seat");
        assert_eq!(root.rect[0], seat.rect[0], "dark={dark}: left-aligned in the seat");
    }
}

/// Nothing renders outside the window horizontally, in either theme.
#[test]
fn every_part_stays_inside_the_window_horizontally() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let mut outside = Vec::new();
        for element in rt.elements() {
            let Some(key) = &element.key else { continue };
            let [x, _y, w, _h] = element.rect;
            if x < -0.5 || x + w > W + 0.5 {
                outside.push(format!("{key} [{x}, {w}]"));
            }
        }
        assert!(
            outside.is_empty(),
            "dark={dark}: parts leave the window: {outside:?}"
        );
    }
}

// ---- the header --------------------------------------------------------------

/// The step's own texts are the `onboarding.s2.*` pack entries, in React's
/// order: the tag label, the heading, the subtitle, the security note and
/// the optional hint.
#[test]
fn the_header_reads_the_s2_pack() {
    for dark in [true, false] {
        let rt = rt(Scene { dark, ..base() });
        // `uppercase` renders the label through `str::to_uppercase` (a
        // no-op for the zh-CN value — the column_mapping_editor ruling).
        assert_eq!(text_under(&rt, "onboarding-step-tag-pill"), t("onboarding.s2.stepLabel").to_uppercase());
        assert_eq!(text_of(&rt, "onboarding-step-s2-ai-title"), t("onboarding.s2.title"));
        assert_eq!(text_of(&rt, "onboarding-step-s2-ai-subtitle"), t("onboarding.s2.subtitle"));
        assert_eq!(
            text_of(&rt, "onboarding-step-s2-ai-security-note-text"),
            t("onboarding.s2.securityNote")
        );
        assert_eq!(text_of(&rt, "onboarding-step-s2-ai-optional-hint"), t("onboarding.s2.optionalHint"));
    }
}

/// `StepTag` (the WelcomeStep export): the accent pill above the heading —
/// `rounded-full`, `bg-accent/12`, `text-accent` — with the tag row's own
/// `mb-4` (16px = 12pt) gap below it and the heading's `mb-2.5` (10px =
/// 7.5pt) gap above the subtitle.
#[test]
fn the_step_tag_is_the_accent_pill() {
    for dark in [true, false] {
        let mut rt = rt(Scene { dark, ..base() });
        settle(&mut rt);
        let k = kit(dark);
        let tag = maybe(&rt, "onboarding-step-tag").expect("the tag row");
        let pill = maybe(&rt, "onboarding-step-tag-pill").expect("the pill");
        assert_eq!(pill.radius, 999.0, "dark={dark}: rounded-full");
        assert!(
            pill.radius >= pill.rect[3] / 2.0,
            "dark={dark}: the radius covers half the pill's height (a pill)"
        );
        assert!(
            (pill.fill.a - 0.12).abs() < 0.02,
            "dark={dark}: bg-accent/12 paints at 12% (got {})",
            pill.fill.a
        );
        let label = text_el_under(&rt, "onboarding-step-tag-pill").expect("the pill's label");
        assert_eq!(label.text_color, Some(k.c.text_accent), "dark={dark}: text-accent");
        // The tag row sits above the heading, its own mb-4 (16px = 12pt)
        // away (StepTag's class, not the heading's).
        let title = maybe(&rt, "onboarding-step-s2-ai-title").expect("the heading");
        assert!(
            (title.rect[1] - (tag.rect[1] + tag.rect[3]) - 12.0).abs() < 0.5,
            "dark={dark}: the heading starts mb-4 (12pt) below the tag row"
        );
        // The heading is the ink of `text-fg`.
        assert_eq!(title.text_color, Some(k.c.text_base), "dark={dark}: text-fg");
        // The heading's own mb-2.5 (10px = 7.5pt) gap above the subtitle.
        let subtitle = maybe(&rt, "onboarding-step-s2-ai-subtitle").expect("the subtitle");
        assert!(
            (subtitle.rect[1] - (title.rect[1] + title.rect[3]) - 7.5).abs() < 0.5,
            "dark={dark}: the subtitle starts mb-2.5 (7.5pt) below the heading"
        );
    }
}

/// The subtitle wraps at React's `max-w-[56ch]` — narrower than the step
/// column — and the card starts `mb-6` (24px = 18pt) below it.
#[test]
fn the_subtitle_caps_at_56ch_above_the_card() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let subtitle = maybe(&rt, "onboarding-step-s2-ai-subtitle").expect("the subtitle");
    let card = maybe(&rt, "onboarding-step-s2-ai-card").expect("the form card");
    assert!(
        subtitle.rect[2] < card.rect[2] - 100.0,
        "the 56ch cap binds well below the 680px column (subtitle {} vs card {})",
        subtitle.rect[2],
        card.rect[2]
    );
    assert!(
        (card.rect[1] - (subtitle.rect[1] + subtitle.rect[3]) - 18.0).abs() < 0.5,
        "the card starts mb-6 (18pt) below the subtitle"
    );
    assert_eq!(subtitle.text_color, Some(kit(true).c.text_muted), "text-fg-secondary");
}

// ---- the shared form, inline inside the card ----------------------------------

/// The card carries the settings' `ModelProfileForm` in its `inline`
/// variant: the reserved `onboarding-ai-form` root key (never the dialog's
/// `ai-profile-form`), no Cancel/Save of its own (the journey footer's
/// Finish owns the primary action), the validate action still there, and
/// the `p-5` (20px = 15pt) padding around it.
#[test]
fn the_shared_form_mounts_inline_inside_the_card() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let card = maybe(&rt, "onboarding-step-s2-ai-card").expect("the form card");
    let form = maybe(&rt, "onboarding-ai-form").expect("the inline form's reserved root key");
    assert!(!has(&rt, "ai-profile-form"), "the dialog variant's root is not mounted");
    assert!(
        form.path.starts_with(&card.path),
        "the form is mounted inside the card, not beside it"
    );
    // The inline variant's field set (the T107 inline contract).
    for key in [
        "onboarding-ai-form-name",
        "onboarding-ai-form-api-key",
        "onboarding-ai-form-endpoint",
        "onboarding-ai-form-model",
        "onboarding-ai-form-max-tokens",
        "ai-provider-select-trigger",
        "ai-profile-validate",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    // No primary action of its own — the footer's Finish is the way out.
    assert!(!has(&rt, "ai-profile-save"), "the journey owns the save");
    assert!(!has(&rt, "ai-profile-cancel"), "no cancel on a first-run step");
    // React `p-5`: the form starts 15pt inside the card on every side.
    assert!((form.rect[0] - card.rect[0] - 15.0).abs() < 0.5, "the card's left padding");
    assert!((form.rect[1] - card.rect[1] - 15.0).abs() < 0.5, "the card's top padding");
    assert!((form.rect[2] - (card.rect[2] - 30.0)).abs() < 0.5, "the card's right padding");
    // The card's chrome: rounded-xl border-edge bg-surface.
    let k = kit(true);
    assert_eq!(card.radius, k.radius.lg, "rounded-xl is the kit's lg token");
    assert!(
        same_color(card.fill, k.c.surface_base),
        "bg-surface (got {:?}, want {:?})",
        card.fill,
        k.c.surface_base
    );
}

/// Channel-wise colour equality with a small tolerance (any solving the
/// paint pipeline performs stays inside it).
fn same_color(a: rune::Color, b: rune::Color) -> bool {
    (a.r - b.r).abs() < 0.01
        && (a.g - b.g).abs() < 0.01
        && (a.b - b.b).abs() < 0.01
        && (a.a - b.a).abs() < 0.01
}

// ---- the security note and the hint -------------------------------------------

/// The security note: the accent-tinted box `mt-5` (20px = 15pt) below the
/// card, with the 15px (11.25pt) Info glyph at its start and the note text
/// beside it; the optional hint `mt-3` (12px = 9pt) below the note.
#[test]
fn the_security_note_and_hint_frame_the_card() {
    let mut rt = rt(Scene { ..base() });
    settle(&mut rt);
    let card = maybe(&rt, "onboarding-step-s2-ai-card").expect("the form card");
    let note = maybe(&rt, "onboarding-step-s2-ai-security-note").expect("the security note");
    let icon = maybe(&rt, "onboarding-step-s2-ai-security-note-icon").expect("the Info glyph");
    let text = maybe(&rt, "onboarding-step-s2-ai-security-note-text").expect("the note text");
    let hint = maybe(&rt, "onboarding-step-s2-ai-optional-hint").expect("the optional hint");

    assert!(
        (note.rect[1] - (card.rect[1] + card.rect[3]) - 15.0).abs() < 0.5,
        "the note starts mt-5 (15pt) below the card"
    );
    assert!(
        (note.fill.a - 0.05).abs() < 0.02,
        "bg-accent/5 paints at 5% (got {})",
        note.fill.a
    );
    // <Info className="mt-0.5 h-[15px] w-[15px] shrink-0 text-accent" />.
    assert!(
        (icon.rect[2] - 11.25).abs() < 0.5 && (icon.rect[3] - 11.25).abs() < 0.5,
        "the 15px glyph is 11.25pt (got {}×{})",
        icon.rect[2],
        icon.rect[3]
    );
    assert!(icon.rect[0] < text.rect[0], "the glyph leads the text (items-start row)");
    assert!(
        text.rect[1] >= note.rect[1] && text.rect[1] < icon.rect[1] + icon.rect[3] + 5.0,
        "the text rides the note's first line, beside the glyph"
    );
    assert_eq!(text.text_color, Some(kit(true).c.text_muted), "text-fg-secondary");
    // The hint: mt-3 (9pt) below the note, muted.
    assert!(
        (hint.rect[1] - (note.rect[1] + note.rect[3]) - 9.0).abs() < 0.5,
        "the hint starts mt-3 (9pt) below the note"
    );
    assert_eq!(hint.text_color, Some(kit(true).c.text_muted), "text-fg-muted");
}

// ---- the journey link ---------------------------------------------------------

/// The journey's own rule: the step's form edits the wizard's one draft
/// machine, so typing credentials flips `aiDraft.hasCredentials` — the
/// exact branch `handleFinish` reads (`finish_path`: nothing typed → finish
/// unconfigured; credentials → the save path) — and the validate action
/// fires through the same slots.
#[test]
fn typing_credentials_flips_the_wizards_finish_branch() {
    let mut rt = rt(Scene { ..base() });
    assert!(!rt.state.draft.has_credentials(), "the fresh journey draft has no key");
    assert_eq!(
        OnboardingWizard::<Scene>::new()
            .has_credentials(rt.state.draft.has_credentials())
            .finish_path(),
        FinishPath::FinishUnconfigured,
        "nothing typed: the journey finishes with AI unconfigured"
    );

    type_into(&mut rt, "onboarding-ai-form-api-key", "sk-journey");
    assert_eq!(rt.state.draft.api_key.text, "sk-journey");
    assert!(rt.state.draft.has_credentials(), "the typed key is the wizard's draft");
    assert_eq!(
        OnboardingWizard::<Scene>::new()
            .has_credentials(rt.state.draft.has_credentials())
            .finish_path(),
        FinishPath::RequestSave,
        "credentials typed: the footer's Finish requests the save"
    );

    // The validate action fires through the step's own form.
    press(&mut rt, "ai-profile-validate");
    assert_eq!(rt.state.validates, 1, "the inline form's validate action is wired");
}
