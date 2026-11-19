//! Port of `packages/frontend/src/windows/settings/ai/ModelProfileListItem.tsx`.
//!
//! One configured model profile row in Settings → AI's "已配置模型" list:
//! the identity block (name, the default star badge, the active badge, the
//! safety-preset shield badge) over a meta line (provider chip · model ·
//! endpoint · egress summary), with the row actions on the right — set
//! default (only on a non-default profile), duplicate, edit, delete.
//!
//! ## What maps where
//!
//! - Props → builder fields and callbacks (the
//!   `GlobalQueryHistoryDialog`/`AddToDashboardDialog` pattern): `profile`
//!   is a borrowed [`AiModelProfile`], `isDefault` / `isActive` flags, and
//!   `onSetDefault` / `onEdit` / `onDuplicate` / `onDelete` are
//!   `Rc<dyn Fn(&mut S)>` the owning list host wires per row.
//! - `detectSafetyPreset(profile.safetyGate)` and
//!   `PROVIDER_PRESETS[profile.providerType]?.displayName` are the T479
//!   [`detect_safety_preset`] / [`provider_preset`] — reused, not
//!   duplicated. The `?? profile.providerType` fallback is unreachable on
//!   the typed mirror (every [`AiProviderType`] has a preset; the api
//!   crate's serde rejects unknown literals at the host's seam).
//! - `getPresetBadge` / `getEgressSummary` are [`Self::preset_badge`] /
//!   [`Self::egress_summary`]; the four preset hues are the palette's
//!   `ai_preset_*_border` (500-hue) fills at 10% over `ai_shield_*`
//!   (400-hue) inks, this port's additions being the purple pair.
//! - React's double `tid` spread (`model-profile-item-{id}` then
//!   `model-profile-card-{id}`) leaves only the LAST `data-testid` in the
//!   DOM; Rune keys the row `model-profile-item-{id}` and this table is
//!   the contract for either name (see the deltas below).
//! - The `sm:` breakpoints never break: the window is never below sm
//!   (960pt ≥ 640px), so the row is always the wide layout
//!   (`sm:flex-row sm:items-center`, `sm:self-center`).
//! - `title` attributes (the action buttons' tooltips, the model and
//!   endpoint spans' hover texts) ride `El::help`.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | row root | `model-profile-item-{id}` |
//! | name / default badge / active badge | `model-profile-item-{id}-name`, `-default`, `-active` |
//! | preset badge (and its `shrink-0` slot) | `model-profile-item-{id}-preset`, `-preset-slot` |
//! | identity block / title row / meta row | `model-profile-item-{id}-main`, `-title`, `-meta` |
//! | provider chip / model / endpoint / egress | `model-profile-item-{id}-provider`, `-model`, `-endpoint`, `-egress` |
//! | actions row / set default | `model-profile-item-{id}-actions`, `-set-default` |
//! | duplicate / edit / delete | `model-profile-item-{id}-duplicate`, `-edit`, `-delete` |
//!
//! ## Known deltas (documented, not silent)
//!
//! - The double `tid` collapses to the one `model-profile-item-{id}` key
//!   (Rune elements carry a single key; React's own DOM kept only the
//!   later `model-profile-card-{id}` spread).
//! - Kit button sizes stand in for Tailwind's pixel classes (the
//!   `ai_context_settings.rs` mapping): the h-7 (28pt) buttons render
//!   `ControlSize::Large`, whose 16pt glyphs stand in for React's h-3 /
//!   h-3.5 (12/14pt) icons; the set-default button's `mr-1` (4pt) icon gap
//!   is the kit's 6pt.
//! - The action buttons' per-hue hover inks (`hover:text-amber-400` /
//!   `-fg` / `-accent` / `-red-400`) are the kit ghost's own hover —
//!   `IconButtonVariant` has no per-hue variants (the `ai_context_settings`
//!   ruling); the base inks are `GhostMuted` (React's `text-fg-muted`).
//! - `transition-colors` and `group` have no Rune counterpart; the row
//!   hover is `El::hover` with the surface-raised fill.
//! - `text-fg-subtle` maps to the kit's `text_faint` (the step below the
//!   `fg-secondary`/`fg-muted` → `text_muted` ruling); `bg-surface` →
//!   `bg_base`, `bg-surface-alt` → `layer_01`, `border-edge` →
//!   `border_base`; `bg-surface-raised/60` → `Kit::alpha(surface_raised,
//!   0.6)`.
//! - 'Unnamed Model', 'Active', 'Strict Egress', `Sample Masked (≤ N
//!   rows)` and 'Unrestricted Egress' are React's raw English literals (no
//!   i18n key exists) — kept verbatim per the rules. The egress switch's
//!   `default: ''` arm is unreachable on the typed three-variant mirror;
//!   the span still always mounts (possibly empty), as React's does.
//! - The default badge's `fill-current` star is the registry's filled-star
//!   body (`ICON_STAR_FILLED`, this port's addition).

// The settings AI section's profile list (T172) is the production consumer;
// until it lands this crate's tests are the only callers.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

use datazen_ui_api::{AiDataEgressLevel, AiModelProfile};

use crate::components::ai::AiSafetyPreset;
use crate::i18n::t;
use crate::icons::{
    ICON_CIRCLE_CHECK, ICON_COPY, ICON_PENCIL, ICON_SHIELD, ICON_STAR, ICON_STAR_FILLED, ICON_TRASH,
};
use crate::ui::kit::kit;
use crate::ui::palette::{
    ai_preset_amber_border, ai_preset_blue_border, ai_preset_emerald_border,
    ai_preset_purple_border, ai_shield_cloud_strict, ai_shield_custom,
    ai_shield_enterprise_balanced, ai_shield_local_trust,
};
use crate::views::settings::ai::use_model_profile_draft::{detect_safety_preset, provider_preset};

/// A row action (React's `onSetDefault` / `onEdit` / `onDuplicate` /
/// `onDelete` props).
type Act<S> = Rc<dyn Fn(&mut S) + 'static>;

/// React `text-sm font-semibold` (the profile name).
const NAME: TextStyle = TextStyle::new(14.0, 600.0);
/// React `text-[11px]` (the default badge's semibold label).
const TEXT_11_SEMIBOLD: TextStyle = TextStyle::new(11.0, 600.0);
/// React `text-[11px] font-medium` (the active badge's label).
const TEXT_11_MEDIUM: TextStyle = TextStyle::new(11.0, 500.0);
/// React `text-[11px]` (the egress summary).
const TEXT_11: TextStyle = TextStyle::new(11.0, 400.0);
/// React `text-xs font-medium` (the preset badge's label).
const TEXT_12_MEDIUM: TextStyle = TextStyle::new(12.0, 500.0);
/// React `text-xs` (the meta row's separators and endpoint).
const TEXT_12: TextStyle = TextStyle::new(12.0, 400.0);
/// React `font-mono text-[11px]` (the provider chip).
const MONO_11: TextStyle = TextStyle::new(11.0, 400.0).mono();
/// React `font-mono text-xs` (the model id).
const MONO_12: TextStyle = TextStyle::new(12.0, 400.0).mono();
/// React `text-fg-muted`-toned separators could share one text element; the
/// bullet is its own child per React span, kept keyed off the row.
const SEP_GAP: f32 = 8.0; // React `gap-2` on both rows.

/// One model-profile row, generic over the owning state `S` the callbacks
/// write (React's `ModelProfileListItemProps`).
pub(crate) struct ModelProfileListItem<'a, S: 'static> {
    profile: &'a AiModelProfile,
    is_default: bool,
    is_active: bool,
    on_set_default: Option<Act<S>>,
    on_edit: Option<Act<S>>,
    on_duplicate: Option<Act<S>>,
    on_delete: Option<Act<S>>,
}

impl<'a, S: 'static> ModelProfileListItem<'a, S> {
    pub(crate) fn new(profile: &'a AiModelProfile) -> Self {
        Self {
            profile,
            is_default: false,
            is_active: false,
            on_set_default: None,
            on_edit: None,
            on_duplicate: None,
            on_delete: None,
        }
    }

    /// React's `isDefault` prop (the amber star badge; hides the set-default
    /// action).
    pub(crate) fn is_default(mut self, is_default: bool) -> Self {
        self.is_default = is_default;
        self
    }

    /// React's `isActive` prop (the accent badge, only when not default).
    pub(crate) fn is_active(mut self, is_active: bool) -> Self {
        self.is_active = is_active;
        self
    }

    pub(crate) fn on_set_default(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_set_default = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_edit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_edit = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_duplicate(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_duplicate = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_delete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_delete = Some(Rc::new(f));
        self
    }

    /// The row: `group flex sm:flex-row sm:items-center justify-between
    /// gap-3 p-3.5 hover:bg-surface-raised` with the active tint.
    pub(crate) fn build(&self, dark: bool) -> El {
        let k = kit(dark);
        row()
            .key(format!("model-profile-item-{}", self.profile.id))
            .w_full()
            .justify_between()
            .center_y()
            .gap(12.0) // React `gap-3`
            .px(14.0) // React `p-3.5`
            .py(14.0)
            // React `hover:bg-surface-raised` (no transition counterpart).
            .hover(k.c.surface_raised)
            // React `isActive ? 'bg-surface-raised/60' : 'bg-surface'`.
            .fill(if self.is_active {
                k.alpha(k.c.surface_raised, 0.60)
            } else {
                k.c.bg_base
            })
            .child(self.identity_block(&k, dark))
            .child(self.actions(&k))
    }

    // ---- the identity block ---------------------------------------------------

    /// The left column: the title row over the meta row
    /// (`flex min-w-0 flex-1 flex-col gap-1.5`).
    fn identity_block(&self, k: &Kit, dark: bool) -> El {
        let id = &self.profile.id;
        let mut title = row()
            .key(format!("model-profile-item-{id}-title"))
            .flex_wrap()
            .center_y()
            .gap(SEP_GAP)
            .min_w(0.0)
            .child(
                text(self.display_name())
                    .key(format!("model-profile-item-{id}-name"))
                    .font(NAME)
                    .color(k.c.text_base)
                    .truncate(),
            );
        // React's `{isDefault && ...}` / `{isActive && !isDefault && ...}`.
        if self.is_default {
            title = title.child(self.default_badge(k, dark));
        }
        if self.is_active && !self.is_default {
            title = title.child(self.active_badge(k));
        }
        // React wraps the badge in a `shrink-0` div.
        title = title.child(
            col()
                .key(format!("model-profile-item-{id}-preset-slot"))
                .child(self.preset_badge(k, dark)),
        );

        let mut meta = row()
            .key(format!("model-profile-item-{id}-meta"))
            .flex_wrap()
            .center_y()
            .gap(SEP_GAP)
            .min_w(0.0)
            .child(self.provider_chip(k))
            .child(text("·").font(TEXT_12).color(k.c.text_muted));
        meta = meta.child(
            text(self.profile.model.clone())
                .key(format!("model-profile-item-{id}-model"))
                .font(MONO_12)
                .color(k.c.text_muted) // React `text-fg-secondary` → text_muted
                .truncate()
                .max_w(200.0) // React `max-w-[200px]`
                .help(self.profile.model.clone()), // React `title`
        );
        // React's `{profile.endpoint && ...}` is a truthiness check: an
        // empty string renders nothing either.
        if let Some(endpoint) = self
            .profile
            .endpoint
            .as_deref()
            .filter(|endpoint| !endpoint.is_empty())
        {
            meta = meta.child(text("·").font(TEXT_12).color(k.c.text_muted));
            meta = meta.child(
                text(endpoint.to_owned())
                    .key(format!("model-profile-item-{id}-endpoint"))
                    .font(TEXT_12)
                    .color(k.c.text_muted)
                    .truncate()
                    .max_w(220.0) // React `max-w-[220px]`
                    .help(endpoint.to_owned()),
            );
        }
        // The separator and the egress span always mount (React renders an
        // empty span for an unknown level; the typed mirror has three).
        meta = meta.child(text("·").font(TEXT_12).color(k.c.text_muted));
        meta = meta.child(
            text(self.egress_summary())
                .key(format!("model-profile-item-{id}-egress"))
                .font(TEXT_11)
                .color(k.c.text_faint), // React `text-fg-subtle` → text_faint
        );

        col()
            .key(format!("model-profile-item-{id}-main"))
            .grow(1.0)
            .min_w(0.0)
            .gap(6.0) // React `gap-1.5`
            .child(title)
            .child(meta)
    }

    /// React's `profile.name || profile.model || 'Unnamed Model'` (JS
    /// truthiness on the two strings).
    fn display_name(&self) -> String {
        if !self.profile.name.is_empty() {
            self.profile.name.clone()
        } else if !self.profile.model.is_empty() {
            self.profile.model.clone()
        } else {
            "Unnamed Model".to_owned()
        }
    }

    /// The amber star badge (`bg-amber-500/20 text-amber-400`, `fill-current`
    /// star at h-2.5).
    fn default_badge(&self, k: &Kit, dark: bool) -> El {
        let ink = ai_shield_enterprise_balanced(dark); // amber-400
        row()
            .key(format!("model-profile-item-{}-default", self.profile.id))
            .center_y()
            .gap(2.0) // React `gap-0.5`
            .px(6.0) // React `px-1.5`
            .py(2.0)
            .radius(k.radius.sm) // React `rounded` (4pt)
            .fill(k.alpha(ai_preset_amber_border(dark), 0.20)) // `bg-amber-500/20`
            .child(rune::icon(&ICON_STAR_FILLED, 10.0).color(ink))
            .child(text(t("settings.ai.defaultBadge")).font(TEXT_11_SEMIBOLD).color(ink))
    }

    /// The accent badge (`bg-accent/20 text-accent`, CheckCircle2 at h-2.5,
    /// the raw 'Active' literal).
    fn active_badge(&self, k: &Kit) -> El {
        row()
            .key(format!("model-profile-item-{}-active", self.profile.id))
            .center_y()
            .gap(2.0)
            .px(6.0)
            .py(2.0)
            .radius(k.radius.sm)
            .fill(k.alpha(k.c.bg_accent, 0.20)) // `bg-accent/20`
            .child(rune::icon(&ICON_CIRCLE_CHECK, 10.0).color(k.c.text_accent))
            .child(text("Active").font(TEXT_11_MEDIUM).color(k.c.text_accent))
    }

    /// React's `getPresetBadge`: the shield at h-3 with the preset's label,
    /// each preset a 10% fill of its 500-hue under its 400-hue ink.
    fn preset_badge(&self, k: &Kit, dark: bool) -> El {
        let (label, fill, ink) = match detect_safety_preset(&self.profile.safety_gate) {
            AiSafetyPreset::CloudStrict => (
                t("settings.ai.presetCloudStrict"),
                k.alpha(ai_preset_blue_border(dark), 0.10),
                ai_shield_cloud_strict(dark),
            ),
            AiSafetyPreset::EnterpriseBalanced => (
                t("settings.ai.presetEnterpriseBalanced"),
                k.alpha(ai_preset_amber_border(dark), 0.10),
                ai_shield_enterprise_balanced(dark),
            ),
            AiSafetyPreset::LocalTrust => (
                t("settings.ai.presetLocalTrust"),
                k.alpha(ai_preset_emerald_border(dark), 0.10),
                ai_shield_local_trust(dark),
            ),
            AiSafetyPreset::Custom => (
                t("settings.ai.presetCustom"),
                k.alpha(ai_preset_purple_border(dark), 0.10),
                ai_shield_custom(dark),
            ),
        };
        row()
            .key(format!("model-profile-item-{}-preset", self.profile.id))
            .center_y()
            .gap(4.0) // React `gap-1`
            .px(8.0) // React `px-2`
            .py(2.0)
            .radius(k.radius.sm)
            .fill(fill)
            .child(rune::icon(&ICON_SHIELD, 12.0).color(ink)) // h-3 w-3
            .child(text(label).font(TEXT_12_MEDIUM).color(ink))
    }

    /// The provider chip (`rounded bg-surface-alt px-1.5 py-0.5 font-mono
    /// text-[11px] text-fg-subtle border border-edge`).
    fn provider_chip(&self, k: &Kit) -> El {
        row()
            .key(format!("model-profile-item-{}-provider", self.profile.id))
            .px(6.0)
            .py(2.0)
            .radius(k.radius.sm)
            .fill(k.c.layer_01) // `bg-surface-alt`
            .border(1.0, k.c.border_base) // `border border-edge`
            .child(
                text(provider_preset(self.profile.provider_type).display_name)
                    .font(MONO_11)
                    .color(k.c.text_faint), // `text-fg-subtle`
            )
    }

    /// React's `getEgressSummary` (raw English literals; the typed mirror's
    /// three variants leave the `default: ''` arm unreachable).
    fn egress_summary(&self) -> String {
        match self.profile.safety_gate.data_egress_level {
            AiDataEgressLevel::Strict => "Strict Egress".to_owned(),
            AiDataEgressLevel::SampleMasked => {
                format!("Sample Masked (≤ {} rows)", self.profile.safety_gate.max_sample_rows)
            }
            AiDataEgressLevel::Unrestricted => "Unrestricted Egress".to_owned(),
        }
    }

    // ---- the actions ----------------------------------------------------------

    /// The right column (`flex items-center gap-1 shrink-0 self-end
    /// sm:self-center` — the sm+ row centers it).
    fn actions(&self, k: &Kit) -> El {
        let id = &self.profile.id;
        let mut actions = row()
            .key(format!("model-profile-item-{id}-actions"))
            .center_y()
            .gap(4.0); // React `gap-1`

        // React's `{!isDefault && ...}`: the set-default action.
        if !self.is_default {
            let mut set_default = Button::new(t("settings.ai.setDefault"))
                .variant(ButtonVariant::GhostMuted) // React `text-fg-muted`
                .control_size(ControlSize::Large) // h-7
                .icon(&ICON_STAR)
                .help(t("settings.ai.setDefault")); // React `title`
            if let Some(on_set_default) = &self.on_set_default {
                let on_set_default = Rc::clone(on_set_default);
                set_default = set_default.on_press(move |s: &mut S| on_set_default(s));
            }
            actions = actions
                .child(set_default.build(k).key(format!("model-profile-item-{id}-set-default")));
        }

        actions = actions.child(self.icon_button(
            k,
            &ICON_COPY,
            t("settings.ai.duplicate"),
            &self.on_duplicate,
            format!("model-profile-item-{id}-duplicate"),
        ));
        actions = actions.child(self.icon_button(
            k,
            &ICON_PENCIL,
            t("settings.ai.editModel"),
            &self.on_edit,
            format!("model-profile-item-{id}-edit"),
        ));
        actions.child(self.icon_button(
            k,
            &ICON_TRASH,
            t("common.delete"),
            &self.on_delete,
            format!("model-profile-item-{id}-delete"),
        ))
    }

    /// One icon-only action (`Button variant="ghost" size="sm"` at `h-7 w-7
    /// p-0`, the icon at h-3.5): the kit's ghost `IconButton` at the sm
    /// height with React's `title` as its tooltip and accessible name.
    fn icon_button(
        &self,
        k: &Kit,
        icon: &'static rune::Icon,
        name: &str,
        act: &Option<Act<S>>,
        key: String,
    ) -> El {
        let mut button = IconButton::new(icon)
            .variant(IconButtonVariant::GhostMuted) // React `text-fg-muted`
            .control_size(ControlSize::Large) // h-7 w-7
            .label(name)
            .help(name); // React `title`
        if let Some(act) = act {
            let act = Rc::clone(act);
            button = button.on_press(move |s: &mut S| act(s));
        }
        button.build(k).key(key)
    }
}
