//! T141 audit: the ModelProfileListItem Rune port.
//!
//! Tester-written: every assertion below comes from re-reading
//! `packages/frontend/src/windows/settings/ai/ModelProfileListItem.tsx` —
//! the mounted tree (the name, the provider chip · model · endpoint ·
//! egress meta line, the safety-preset shield + label badge, the default
//! star badge, the active badge), React's gating rules (the file has no
//! disabled buttons: its rules are structural — the set-default action
//! only on a non-default profile, the active badge only when
//! `isActive && !isDefault`, the endpoint span on `profile.endpoint`
//! truthiness, and the `name || model || 'Unnamed Model'` fallback), the
//! four action slots each firing their own callback and nothing else, and
//! the geometry/fill contracts (the 200/220pt truncation caps, the h-7
//! 28pt buttons, the preset badges' 10% tints, the active row's 60%
//! surface-raised wash). Element evidence comes from `Runtime::elements()`
//! (runtime layout), never from a value the component reports about
//! itself; presses go through real pointer events.

use datazen_ui_api::{AiModelProfile, AiProviderType};
use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::palette::{
    ai_preset_amber_border, ai_preset_blue_border, ai_preset_emerald_border, ai_preset_purple_border,
};
use crate::views::ai::model_profile_list_item::ModelProfileListItem;
use crate::views::ai::use_model_profile_draft::{
    default_safety_gate, provider_preset, SafetyPresetKey,
};

/// The row root key (React's double `tid` spread collapses to the
/// `model-profile-item-{id}` contract documented in the port).
const ROW: &str = "model-profile-item-p1";

#[derive(Clone)]
struct Probe {
    profile: AiModelProfile,
    is_default: bool,
    is_active: bool,
    dark: bool,
    set_defaults: usize,
    edits: usize,
    duplicates: usize,
    deletes: usize,
}

/// React's `profile` prop: an Anthropic row over the given preset's gate.
fn profile(kind: SafetyPresetKey) -> AiModelProfile {
    AiModelProfile {
        id: "p1".to_owned(),
        name: "Work profile".to_owned(),
        provider_type: AiProviderType::Anthropic,
        api_key: Some("sk-1".to_owned()),
        endpoint: Some("https://api.anthropic.com".to_owned()),
        model: "claude-sonnet-4-20250514".to_owned(),
        max_tokens: Some(8192),
        extra: None,
        safety_gate: default_safety_gate(kind),
        is_default: Some(false),
    }
}

fn probe(profile: AiModelProfile) -> Probe {
    Probe {
        profile,
        is_default: false,
        is_active: false,
        dark: true,
        set_defaults: 0,
        edits: 0,
        duplicates: 0,
        deletes: 0,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let item = ModelProfileListItem::<Probe>::new(&s.profile)
        .is_default(s.is_default)
        .is_active(s.is_active)
        .on_set_default(|s: &mut Probe| s.set_defaults += 1)
        .on_edit(|s: &mut Probe| s.edits += 1)
        .on_duplicate(|s: &mut Probe| s.duplicates += 1)
        .on_delete(|s: &mut Probe| s.deletes += 1);
    col().key("page").w_full().h_full().child(item.build(s.dark))
}

fn new_rt(scene: Probe) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Probe>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has(rt: &Runtime<Probe>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<Probe>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// `path` names a child of `prefix` (a whole segment, not a longer key).
fn within(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The first text element inside the keyed container — the badge labels
/// and the provider chip ride unkeyed text children (the dialog audit's
/// `button_label` fallback technique).
fn text_under(rt: &Runtime<Probe>, key: &str) -> String {
    let Some(container) = maybe(rt, key) else {
        return String::new();
    };
    rt.elements()
        .into_iter()
        .find(|e| {
            !e.exiting && e.role == "text" && e.text.is_some() && within(&e.path, &container.path)
        })
        .and_then(|e| e.text)
        .unwrap_or_default()
}

/// The icon elements inside the keyed container (the shield, the stars).
fn icons_under(rt: &Runtime<Probe>, key: &str) -> Vec<rune::ElementInfo> {
    let Some(container) = maybe(rt, key) else {
        return Vec::new();
    };
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting && e.role == "icon" && within(&e.path, &container.path))
        .collect()
}

/// Advance the clock past the kit's enter/hover motion (~0.4s) so freshly
/// mounted controls are pressable and fills finish cross-fading (the
/// sibling audits' helper).
fn settle(rt: &mut Runtime<Probe>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

// ---- the mounted tree ---------------------------------------------------------

#[test]
fn mounts_the_identity_and_meta_lines() {
    let rt = new_rt(probe(profile(SafetyPresetKey::CloudStrict)));

    for key in [
        ROW,
        "model-profile-item-p1-main",
        "model-profile-item-p1-title",
        "model-profile-item-p1-meta",
        "model-profile-item-p1-preset-slot",
        "model-profile-item-p1-preset",
        "model-profile-item-p1-provider",
        "model-profile-item-p1-model",
        "model-profile-item-p1-endpoint",
        "model-profile-item-p1-egress",
        "model-profile-item-p1-actions",
        "model-profile-item-p1-set-default",
        "model-profile-item-p1-duplicate",
        "model-profile-item-p1-edit",
        "model-profile-item-p1-delete",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    // React's name span, meta spans and their verbatim content.
    assert_eq!(text_of(&rt, "model-profile-item-p1-name"), "Work profile");
    assert_eq!(
        text_under(&rt, "model-profile-item-p1-provider"),
        provider_preset(AiProviderType::Anthropic).display_name,
        "PROVIDER_PRESETS[providerType]?.displayName"
    );
    assert_eq!(
        text_of(&rt, "model-profile-item-p1-model"),
        "claude-sonnet-4-20250514"
    );
    assert_eq!(
        text_of(&rt, "model-profile-item-p1-endpoint"),
        "https://api.anthropic.com"
    );
    assert_eq!(text_of(&rt, "model-profile-item-p1-egress"), "Strict Egress");
    // The preset badge: the shield glyph plus its label.
    assert_eq!(
        text_under(&rt, "model-profile-item-p1-preset"),
        t("settings.ai.presetCloudStrict")
    );
    let shields = icons_under(&rt, "model-profile-item-p1-preset");
    assert_eq!(shields.len(), 1, "one shield in the preset badge");
    assert!(shields[0].rect[2] > 0.0, "the shield is laid out");
    // React's plain row (no flags): neither badge, all four actions.
    assert!(!has(&rt, "model-profile-item-p1-default"));
    assert!(!has(&rt, "model-profile-item-p1-active"));
}

#[test]
fn each_safety_preset_gets_its_own_label_tint_and_egress_line() {
    // (gate, i18n key, egress summary, the 500-hue behind the 10% tint)
    let cases: [(SafetyPresetKey, &str, &str, fn(bool) -> rune::Color); 3] = [
        (
            SafetyPresetKey::CloudStrict,
            "settings.ai.presetCloudStrict",
            "Strict Egress",
            ai_preset_blue_border,
        ),
        (
            SafetyPresetKey::EnterpriseBalanced,
            "settings.ai.presetEnterpriseBalanced",
            "Sample Masked (≤ 3 rows)",
            ai_preset_amber_border,
        ),
        (
            SafetyPresetKey::LocalTrust,
            "settings.ai.presetLocalTrust",
            "Unrestricted Egress",
            ai_preset_emerald_border,
        ),
    ];
    let k = kit(true);
    for (kind, key, egress, hue) in cases {
        let mut rt = new_rt(probe(profile(kind)));
        settle(&mut rt);
        assert_eq!(
            text_under(&rt, "model-profile-item-p1-preset"),
            t(key),
            "{kind:?} label"
        );
        assert_eq!(text_of(&rt, "model-profile-item-p1-egress"), egress, "{kind:?} egress");
        assert_eq!(
            element(&rt, "model-profile-item-p1-preset").fill,
            k.alpha(hue(true), 0.10),
            "{kind:?} preset tint"
        );
    }

    // A gate matching no preset conjunction is `custom`: the purple pair.
    let mut custom = profile(SafetyPresetKey::CloudStrict);
    custom.safety_gate.require_sql_confirm = false;
    let mut rt = new_rt(probe(custom));
    settle(&mut rt);
    assert_eq!(
        text_under(&rt, "model-profile-item-p1-preset"),
        t("settings.ai.presetCustom")
    );
    assert_eq!(
        element(&rt, "model-profile-item-p1-preset").fill,
        k.alpha(ai_preset_purple_border(true), 0.10),
        "bg-purple-500/10 tint"
    );
}

#[test]
fn the_name_falls_back_to_the_model_then_the_react_literal() {
    let mut named = profile(SafetyPresetKey::CloudStrict);
    named.name = String::new();
    let mut unnamed = named.clone();
    unnamed.model = String::new();
    let rt = new_rt(probe(named));
    assert_eq!(
        text_of(&rt, "model-profile-item-p1-name"),
        "claude-sonnet-4-20250514",
        "profile.name || profile.model"
    );

    let rt = new_rt(probe(unnamed));
    assert_eq!(text_of(&rt, "model-profile-item-p1-name"), "Unnamed Model");
}

#[test]
fn the_endpoint_span_follows_react_truthiness() {
    // An empty string is falsy in React: no span, no separator pair.
    let mut blank = profile(SafetyPresetKey::CloudStrict);
    blank.endpoint = Some(String::new());
    let rt = new_rt(probe(blank));
    assert!(!has(&rt, "model-profile-item-p1-endpoint"));

    let mut none = profile(SafetyPresetKey::CloudStrict);
    none.endpoint = None;
    let rt = new_rt(probe(none));
    assert!(!has(&rt, "model-profile-item-p1-endpoint"));
    // The rest of the meta line still mounts (the egress span always does).
    assert_eq!(text_of(&rt, "model-profile-item-p1-egress"), "Strict Egress");
}

// ---- React's gating rules (no disabled buttons; structural mounts) -------------

#[test]
fn the_default_flag_swaps_the_star_badge_for_the_set_default_action() {
    // A default profile: the amber star badge, no set-default action, and
    // no active badge even when isActive (React `isActive && !isDefault`).
    for is_active in [false, true] {
        let mut scene = probe(profile(SafetyPresetKey::CloudStrict));
        scene.is_default = true;
        scene.is_active = is_active;
        let rt = new_rt(scene);
        assert!(has(&rt, "model-profile-item-p1-default"), "default badge");
        assert_eq!(
            text_under(&rt, "model-profile-item-p1-default"),
            t("settings.ai.defaultBadge")
        );
        assert_eq!(
            icons_under(&rt, "model-profile-item-p1-default").len(),
            1,
            "the badge's filled star"
        );
        assert!(
            !has(&rt, "model-profile-item-p1-active"),
            "isActive && !isDefault is false"
        );
        assert!(
            !has(&rt, "model-profile-item-p1-set-default"),
            "!isDefault gates the action"
        );
    }

    // An active non-default profile: the accent badge with React's raw
    // 'Active' literal, no star badge, the set-default action present.
    let mut scene = probe(profile(SafetyPresetKey::CloudStrict));
    scene.is_active = true;
    let rt = new_rt(scene);
    assert!(has(&rt, "model-profile-item-p1-active"));
    assert_eq!(text_under(&rt, "model-profile-item-p1-active"), "Active");
    assert_eq!(
        icons_under(&rt, "model-profile-item-p1-active").len(),
        1,
        "the badge's check glyph"
    );
    assert!(!has(&rt, "model-profile-item-p1-default"));
    assert!(has(&rt, "model-profile-item-p1-set-default"));
}

#[test]
fn the_action_buttons_carry_react_titles_as_their_accessible_names() {
    let rt = new_rt(probe(profile(SafetyPresetKey::CloudStrict)));

    // React's `title` attributes (the port's `label`/`help`).
    assert_eq!(
        element(&rt, "model-profile-item-p1-set-default").label.as_deref(),
        Some(t("settings.ai.setDefault"))
    );
    assert_eq!(
        element(&rt, "model-profile-item-p1-duplicate").label.as_deref(),
        Some(t("settings.ai.duplicate"))
    );
    assert_eq!(
        element(&rt, "model-profile-item-p1-edit").label.as_deref(),
        Some(t("settings.ai.editModel"))
    );
    assert_eq!(
        element(&rt, "model-profile-item-p1-delete").label.as_deref(),
        Some(t("common.delete"))
    );
    // The set-default button shows its label next to the star (React's
    // icon + span); the icon-only actions carry no text child.
    assert_eq!(
        text_under(&rt, "model-profile-item-p1-set-default"),
        t("settings.ai.setDefault")
    );
    assert_eq!(text_under(&rt, "model-profile-item-p1-duplicate"), "");
    assert_eq!(text_under(&rt, "model-profile-item-p1-edit"), "");
    assert_eq!(text_under(&rt, "model-profile-item-p1-delete"), "");
}

#[test]
fn making_a_row_default_drops_the_action_and_mounts_the_badge() {
    // The host flow React's list owns: press set-default, the host flips
    // isDefault, the re-rendered row trades the action for the badge.
    let mut rt = new_rt(probe(profile(SafetyPresetKey::CloudStrict)));
    settle(&mut rt);
    press(&mut rt, "model-profile-item-p1-set-default");
    assert_eq!(rt.state.set_defaults, 1);
    rt.update(|s, _| s.is_default = true);
    settle(&mut rt);
    assert!(!has(&rt, "model-profile-item-p1-set-default"));
    assert!(has(&rt, "model-profile-item-p1-default"));
    assert_eq!(
        text_under(&rt, "model-profile-item-p1-default"),
        t("settings.ai.defaultBadge")
    );
}

// ---- the four action slots ------------------------------------------------------

#[test]
fn each_action_fires_its_own_slot_and_nothing_else() {
    let mut rt = new_rt(probe(profile(SafetyPresetKey::CloudStrict)));
    settle(&mut rt);

    press(&mut rt, "model-profile-item-p1-set-default");
    assert_eq!(rt.state.set_defaults, 1, "onSetDefault");
    assert_eq!(rt.state.duplicates, 0);
    assert_eq!(rt.state.edits, 0);
    assert_eq!(rt.state.deletes, 0);

    press(&mut rt, "model-profile-item-p1-duplicate");
    assert_eq!(rt.state.duplicates, 1, "onDuplicate");
    assert_eq!(rt.state.set_defaults, 1);
    assert_eq!(rt.state.edits, 0);
    assert_eq!(rt.state.deletes, 0);

    press(&mut rt, "model-profile-item-p1-edit");
    assert_eq!(rt.state.edits, 1, "onEdit");
    assert_eq!(rt.state.set_defaults, 1);
    assert_eq!(rt.state.duplicates, 1);
    assert_eq!(rt.state.deletes, 0);

    press(&mut rt, "model-profile-item-p1-delete");
    assert_eq!(rt.state.deletes, 1, "onDelete");
    assert_eq!(rt.state.set_defaults, 1);
    assert_eq!(rt.state.duplicates, 1);
    assert_eq!(rt.state.edits, 1);
}

// ---- geometry and fills ---------------------------------------------------------

#[test]
fn the_truncation_caps_and_button_sizes_hold() {
    let mut long = profile(SafetyPresetKey::CloudStrict);
    long.model = "a-very-long-model-identifier-that-runs-well-past-two-hundred-points-of-mono-text"
        .to_owned();
    long.endpoint = Some(
        "https://api.anthropic.com/v1/extra/long/endpoint/path/that/runs/past/two/hundred/twenty/points"
            .to_owned(),
    );
    let mut rt = new_rt(probe(long));
    settle(&mut rt);

    // React `max-w-[200px]` / `max-w-[220px]` on the truncated spans.
    let model = element(&rt, "model-profile-item-p1-model").rect;
    assert!(model[2] <= 200.5, "model span width {}", model[2]);
    let endpoint = element(&rt, "model-profile-item-p1-endpoint").rect;
    assert!(endpoint[2] <= 220.5, "endpoint span width {}", endpoint[2]);

    // React's `h-7 w-7 p-0` icon actions are the kit's Large square.
    for key in [
        "model-profile-item-p1-duplicate",
        "model-profile-item-p1-edit",
        "model-profile-item-p1-delete",
    ] {
        let rect = element(&rt, key).rect;
        assert_eq!(rect[2], 28.0, "{key} width");
        assert_eq!(rect[3], 28.0, "{key} height");
    }
    // The set-default button is `h-7` tall (its width hugs the label).
    assert_eq!(element(&rt, "model-profile-item-p1-set-default").rect[3], 28.0);
}

#[test]
fn the_row_paints_the_active_wash_and_the_resting_base() {
    let k = kit(true);

    let mut scene = probe(profile(SafetyPresetKey::CloudStrict));
    scene.is_active = true;
    let mut rt = new_rt(scene);
    settle(&mut rt);
    settle(&mut rt); // fills finish their cross-fade
    assert_eq!(
        element(&rt, ROW).fill,
        k.alpha(k.c.surface_raised, 0.60),
        "isActive paints bg-surface-raised/60"
    );

    // The resting row paints React's `bg-surface` (move the pointer off
    // the row first so the hover tint cannot interfere).
    let mut rt = new_rt(probe(profile(SafetyPresetKey::CloudStrict)));
    rt.pointer_move(480.0, 620.0);
    settle(&mut rt);
    settle(&mut rt);
    assert_eq!(element(&rt, ROW).fill, k.c.bg_base, "the plain row's bg-surface");
}

#[test]
fn every_key_is_unique_among_siblings() {
    // The kit reuses "icon"/"label" keys in different subtrees, so the
    // contract is uniqueness among siblings (the column-mapping row
    // audit's technique); React's per-row testids are unique per row.
    use std::collections::HashMap;
    let rt = new_rt(probe(profile(SafetyPresetKey::CloudStrict)));
    let mut by_parent: HashMap<String, Vec<String>> = HashMap::new();
    for e in rt.elements() {
        if let Some(key) = e.key {
            let parent = match e.path.rfind('/') {
                Some(i) => e.path[..i].to_owned(),
                None => String::new(),
            };
            by_parent.entry(parent).or_default().push(key);
        }
    }
    for (parent, keys) in &by_parent {
        let mut sorted = keys.clone();
        sorted.sort();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            before,
            "duplicate keys under parent {parent}: {keys:?}"
        );
    }
}
