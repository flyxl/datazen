//! Port of `packages/frontend/src/windows/settings/AiSettingsSection.tsx`
//! (T172) — the settings window's AI section shell.
//!
//! The section composes the AI subtree: the context-directory row, the
//! configured-models block (header with the configured badge and the add
//! button, the empty state or the profile list, the model-profile dialog),
//! and the global strict-egress safeguard whose disable path runs a
//! warning confirmation. The content sections between (AiContextSettings,
//! SkillsSection, MemorySection, AiToolBridgeSection) are React siblings
//! this shell mounts — the engine section takes them as prebuilt children
//! so each lands with its own task (AiContextSettings is T015, live; T408
//! Skills, T360 Memory and T198 ToolBridge are pending; the host passes
//! what exists and the `{settingsConfig && …}` gating stays here).
//!
//! ## What maps where
//!
//! - `ContextDirSetting` becomes the context-dir half of
//!   [`AiSettingsSectionState`]: the local draft resyncs from
//!   `settings.contextDir` (the `useEffect` dep — the host calls
//!   [`AiSettingsSectionState::resync_context_dir`] per frame with the
//!   persisted value), `contextCommands.getDir()`'s answer lands through
//!   [`AiSettingsSectionState::note_default_context_dir`] (the host's
//!   load; `None` until it answers), and the save arms a 2s
//!   saved-deadline the way `ai_context_settings` does
//!   (`saved_until = tx.clock() + 2.0`, `tx.wake_after`).
//! - `pickPath` (`lib/pathPicker.ts`, T571) is a seam: the browse button
//!   is [`Self::on_browse_context_dir`]; the host opens the folder
//!   dialog and writes the answer into the draft.
//! - `settingsCommands.openContextDir` (T388) is
//!   [`Self::on_open_context_dir`], a seam callback.
//! - The mount effect (`loadProviders` + `loadConfig`) is the host's
//!   section-entry step — the store's begin/complete pairs are the
//!   ai_store's; nothing here owns them.
//! - The profile handlers: add/edit are pure state (the dialog's open
//!   flag + editing profile); duplicate/set-default/delete call the
//!   host, which drives `ai_store` (`save_profile`,
//!   `begin_set_active_profile`, `delete_profile`). The duplication's
//!   `crypto.randomUUID()` is a host seam; the profiles themselves are
//!   the api crate's full [`AiModelProfile`] (the shape
//!   `ModelProfileListItem` and the dialog already consume), so the
//!   `` `${name || model} (Copy)` `` fallback reads the real fields.
//! - The strict-egress toggle: `checked = aiStrictEgress !== false`
//!   (undefined reads checked); disabling a currently-`true` strict
//!   egress arms the warning confirm
//!   ([`AiSettingsSectionState::toggle_strict_egress`] — React's
//!   truthiness check, so `undefined → false` skips the confirm exactly
//!   as the `&&` does). The confirm's resolution is the host's
//!   [`Self::on_egress_confirm_resolved`]: `resolve_egress_confirm` then
//!   the settings write on OK (React's `if (!ok) return;`).
//! - `SectionTitle`/`ToggleRow` (settingsUi.tsx, T399) are unported; the
//!   titles render in React's SectionTitle style (13/600 muted, the
//!   `ai_context_settings` precedent, hint riding the tooltip base) and
//!   the toggle row is the kit [`Checkbox`] with label + description
//!   (an on-toggle press is React's `onChange(!checked)`).
//!
//! Known deltas (documented, not silent): kit button sizes stand in for
//! Tailwind's pixel classes (h-9 → `Large` 28pt, size="sm" → `Small`);
//! React's `secondary` maps to `Outline` and `primary` to `Submit` (the
//! T022 mapping every port uses); `uppercase tracking-wider` renders
//! without case or tracking (the zh-CN strings carry no case);
//! PathInput's embedded browse affordance is a separate trailing button;
//! the empty state's dashed border renders as a plain bordered box.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `add-ai-model-button`,
//! `ai-models-empty`, `ai-models-list`. Unlabeled parts derive from the
//! section key (`key`, default `"ai-settings-section"`): the context-dir
//! row `<key>-dir`, its input/save/browse/open buttons
//! `<key>-dir-input`/`<key>-dir-save`/`<key>-dir-browse`/`<key>-dir-open`,
//! the configured badge `<key>-configured`, the error line
//! `<key>-error`, and the egress row `<key>-egress`.

#![allow(dead_code)] // reusable port; the settings view track wires the "ai" section

use std::rc::Rc;

use rune::{col, icon, row, text, tok, El, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

use datazen_ui_api::{AiModelProfile, AiSettingsConfig};
use crate::hooks::confirm_dialog::{ConfirmDialogHost, ConfirmDialogState, ConfirmOptionsVm};
use crate::i18n::t;
use crate::icons::{ICON_FOLDER_OPEN, ICON_PLUS, ICON_SPARKLES};
use crate::ui::confirm_dialog::ConfirmKind;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;
use crate::ui::tooltip::Tooltip;
use crate::views::settings::ai::model_profile_list_item::ModelProfileListItem;

/// React's `setTimeout(() => setSaved(false), 2000)` — the context-dir
/// save feedback window, in seconds.
pub const CONTEXT_DIR_SAVED_WINDOW: f64 = 2.0;

type Act<S> = Rc<dyn Fn(&mut S)>;
type IdAct<S> = Rc<dyn Fn(&mut S, String)>;
type ProfileAct<S> = Rc<dyn Fn(&mut S, AiModelProfile)>;

// ---------------------------------------------------------------------------
// The section state (React's useState fields, owned)
// ---------------------------------------------------------------------------

/// What the strict-egress toggle decided (React's async IIFE branches).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EgressToggle {
    /// The disable path armed the warning confirm (the request id).
    NeedsConfirm(u64),
    /// Apply immediately (enabling, or disabling from a non-`true` value —
    /// React's `&& settings.aiStrictEgress` truthiness check).
    Apply(bool),
}

/// The section's own state: the context-dir draft and its saved
/// feedback, the model-profile dialog, and the relaxed-egress confirm.
#[derive(Clone, Default)]
pub struct AiSettingsSectionState {
    /// `localDir` (the draft, `PathInput`'s value).
    pub context_dir_draft: TextEdit,
    /// The `settings.contextDir` the draft last resynced from (the
    /// `useEffect` dep; the host drives the resync per frame).
    pub resynced_context_dir: String,
    /// `defaultDir` — `contextCommands.getDir()`'s answer; `None` until
    /// the host's load lands (React's empty-string start).
    pub default_context_dir: Option<String>,
    /// The save feedback deadline (`saved` while `tx.clock()` is behind).
    pub context_dir_saved_until: Option<f64>,
    /// `dialogOpen` — while true the model-profile dialog renders.
    pub dialog_open: bool,
    /// `editingProfile` — `None` adds, `Some` edits that profile.
    pub editing_profile: Option<AiModelProfile>,
    /// The relaxed-egress confirmation (`useConfirmDialog`'s instance).
    pub egress_confirm: ConfirmDialogState,
}

impl AiSettingsSectionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The `useEffect [settings.contextDir]` resync: a changed persisted
    /// value replaces the draft (a fresh `TextEdit` from the value is
    /// React's `setLocalDir` reset).
    pub fn resync_context_dir(&mut self, persisted: &str) {
        if self.resynced_context_dir != persisted {
            self.resynced_context_dir = persisted.to_owned();
            self.context_dir_draft = TextEdit::new(persisted.to_owned());
        }
    }

    /// `contextCommands.getDir()`'s answer (the mount effect's load).
    pub fn note_default_context_dir(&mut self, dir: Option<String>) {
        self.default_context_dir = dir;
    }

    /// The save press's state half: arm the 2s feedback window and hand
    /// the draft back. The host persists
    /// `{...settings, contextDir: draft}` and calls
    /// `tx.wake_after(CONTEXT_DIR_SAVED_WINDOW)`.
    pub fn begin_save_context_dir(&mut self, now: f64) -> String {
        self.context_dir_saved_until = Some(now + CONTEXT_DIR_SAVED_WINDOW);
        self.context_dir_draft.text.clone()
    }

    /// Whether the save feedback is still showing (the button's label).
    pub fn context_dir_saved(&self, now: f64) -> bool {
        self.context_dir_saved_until.is_some_and(|until| now < until)
    }

    /// `handleAddModel` — open the dialog with no editing profile.
    pub fn begin_add_model(&mut self) {
        self.editing_profile = None;
        self.dialog_open = true;
    }

    /// `handleEditModel` — open the dialog editing `profile`.
    pub fn begin_edit_model(&mut self, profile: &AiModelProfile) {
        self.editing_profile = Some(profile.clone());
        self.dialog_open = true;
    }

    /// The dialog's `onClose`.
    pub fn close_model_dialog(&mut self) {
        self.dialog_open = false;
    }

    /// The ToggleRow's `onChange` (the decision half): disabling a
    /// currently-`true` strict egress arms the warning confirm; every
    /// other transition applies directly. React's truthiness check is
    /// `settings.aiStrictEgress` — `Some(true)` only (undefined and
    /// false are falsy and skip the confirm).
    pub fn toggle_strict_egress(
        &mut self,
        enabled: bool,
        current: Option<bool>,
    ) -> EgressToggle {
        if !enabled && current == Some(true) {
            let id = self.egress_confirm.open(ConfirmOptionsVm {
                title: t("settings.ai.strictEgressDisableTitle").to_owned(),
                message: t("settings.ai.strictEgressDisableMessage").to_owned(),
                kind: ConfirmKind::Warning,
                ..ConfirmOptionsVm::default()
            });
            EgressToggle::NeedsConfirm(id)
        } else {
            EgressToggle::Apply(enabled)
        }
    }

    /// The confirm's resolution (`if (!ok) return;`): clear the pending
    /// request and report whether the relaxed-egress write goes through.
    pub fn resolve_egress_confirm(&mut self, result: bool) -> bool {
        matches!(self.egress_confirm.answer_pending(result), Some(true))
    }
}

// ---------------------------------------------------------------------------
// The section component (built per frame over the host's `S`)
// ---------------------------------------------------------------------------

/// The AI settings section (React `AiSettingsSection`).
pub struct AiSettingsSection<'a, S: 'static> {
    key: &'a str,
    state: &'a AiSettingsSectionState,
    /// The store's `settingsConfig` (the content sections' load gate).
    settings_config: Option<&'a AiSettingsConfig>,
    /// `isConfigured` (the header badge).
    is_configured: bool,
    /// `configError` (the red line under the header).
    config_error: Option<&'a str>,
    /// `settings.aiStrictEgress` (`None` = undefined → checked).
    strict_egress: Option<bool>,
    /// Caller-computed from the saved deadline; the view reads no clock.
    context_dir_saved: bool,
    /// The prebuilt content sections (each lands with its own task):
    /// AiContextSettings (T015), Skills (T408), Memory (T360) — rendered
    /// only while `settings_config` is present, and ToolBridge (T198) —
    /// rendered unconditionally.
    context_settings: Option<El>,
    skills_section: Option<El>,
    memory_section: Option<El>,
    tool_bridge_section: Option<El>,
    /// The model-profile dialog's appearance (T065's component over the
    /// host's slots); painted into the layer only while open.
    model_dialog: Option<Rc<dyn Fn(&S, bool, &mut Vec<El>)>>,
    // callbacks
    context_dir_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
    on_save_context_dir: Option<Rc<dyn Fn(&mut S, &mut Tx)>>,
    on_browse_context_dir: Option<Act<S>>,
    on_open_context_dir: Option<Act<S>>,
    on_add_model: Option<Act<S>>,
    on_edit_model: Option<ProfileAct<S>>,
    on_duplicate_model: Option<ProfileAct<S>>,
    on_set_default: Option<ProfileAct<S>>,
    on_delete_model: Option<IdAct<S>>,
    on_toggle_strict_egress: Option<Rc<dyn Fn(&mut S, bool)>>,
    /// The relaxed-egress confirm's resolution: resolve the pending
    /// request (`state.resolve_egress_confirm(result)`) and apply the
    /// write on OK.
    on_egress_confirm_resolved: Option<Rc<dyn Fn(&mut S, bool)>>,
}

impl<'a, S: 'static> AiSettingsSection<'a, S> {
    pub fn new(key: &'a str, state: &'a AiSettingsSectionState) -> Self {
        Self {
            key,
            state,
            settings_config: None,
            is_configured: false,
            config_error: None,
            strict_egress: None,
            context_dir_saved: false,
            context_settings: None,
            skills_section: None,
            memory_section: None,
            tool_bridge_section: None,
            model_dialog: None,
            context_dir_edit: None,
            on_save_context_dir: None,
            on_browse_context_dir: None,
            on_open_context_dir: None,
            on_add_model: None,
            on_edit_model: None,
            on_duplicate_model: None,
            on_set_default: None,
            on_delete_model: None,
            on_toggle_strict_egress: None,
            on_egress_confirm_resolved: None,
        }
    }

    /// The store's `settingsConfig` projection.
    pub fn settings_config(mut self, config: Option<&'a AiSettingsConfig>) -> Self {
        self.settings_config = config;
        self
    }

    /// `isConfigured` — the header badge's text.
    pub fn is_configured(mut self, configured: bool) -> Self {
        self.is_configured = configured;
        self
    }

    /// `configError` — the red line under the models header.
    pub fn config_error(mut self, error: Option<&'a str>) -> Self {
        self.config_error = error;
        self
    }

    /// `settings.aiStrictEgress` — `None` is React's undefined.
    pub fn strict_egress(mut self, value: Option<bool>) -> Self {
        self.strict_egress = value;
        self
    }

    /// Whether the context-dir save feedback window is still open.
    pub fn context_dir_saved(mut self, saved: bool) -> Self {
        self.context_dir_saved = saved;
        self
    }

    /// The AiContextSettings section (T015), prebuilt; gated on
    /// `settingsConfig` exactly as React's `{settingsConfig && …}`.
    pub fn context_settings(mut self, section: Option<El>) -> Self {
        self.context_settings = section;
        self
    }

    /// The SkillsSection (T408's seam; `None` until it lands).
    pub fn skills_section(mut self, section: Option<El>) -> Self {
        self.skills_section = section;
        self
    }

    /// The MemorySection (T360's seam; `None` until it lands).
    pub fn memory_section(mut self, section: Option<El>) -> Self {
        self.memory_section = section;
        self
    }

    /// The AiToolBridgeSection (T198's seam; `None` until it lands).
    /// React mounts it unconditionally — no `settingsConfig` gate.
    pub fn tool_bridge_section(mut self, section: Option<El>) -> Self {
        self.tool_bridge_section = section;
        self
    }

    /// The model-profile dialog's appearance (T065's component; the host
    /// builds it with its form slots). Painted into the layer only while
    /// the section's dialog is open.
    pub fn model_dialog(
        mut self,
        build: impl Fn(&S, bool, &mut Vec<El>) + 'static,
    ) -> Self {
        self.model_dialog = Some(Rc::new(build));
        self
    }

    /// The `PathInput`'s `TextEdit` in `S` (the context-dir draft).
    pub fn context_dir_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + Clone + 'static,
    ) -> Self {
        self.context_dir_edit = Some(Rc::new(f));
        self
    }

    /// The context-dir save press: persist
    /// `{...settings, contextDir: draft}`, arm the 2s feedback and
    /// `tx.wake_after` it.
    pub fn on_save_context_dir(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_save_context_dir = Some(Rc::new(f));
        self
    }

    /// The `PathInput`'s browse (`pickPath` — the folder dialog; T571's
    /// seam).
    pub fn on_browse_context_dir(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_browse_context_dir = Some(Rc::new(f));
        self
    }

    /// `settingsCommands.openContextDir()` (T388's seam).
    pub fn on_open_context_dir(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_context_dir = Some(Rc::new(f));
        self
    }

    /// The header's add-model press (`handleAddModel`).
    pub fn on_add_model(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_add_model = Some(Rc::new(f));
        self
    }

    /// A row's edit press (`handleEditModel`).
    pub fn on_edit_model(mut self, f: impl Fn(&mut S, AiModelProfile) + 'static) -> Self {
        self.on_edit_model = Some(Rc::new(f));
        self
    }

    /// A row's duplicate press (`handleDuplicateModel`: the host builds
    /// the copy — new uuid, `` `${name || model} (Copy)` ``, default
    /// false — and saves it).
    pub fn on_duplicate_model(
        mut self,
        f: impl Fn(&mut S, AiModelProfile) + 'static,
    ) -> Self {
        self.on_duplicate_model = Some(Rc::new(f));
        self
    }

    /// A row's set-default press (`handleSetDefault`: save with
    /// `isDefault: true`, then `setActiveProfile(id)`).
    pub fn on_set_default(mut self, f: impl Fn(&mut S, AiModelProfile) + 'static) -> Self {
        self.on_set_default = Some(Rc::new(f));
        self
    }

    /// A row's delete press (`handleDeleteModel`).
    pub fn on_delete_model(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_delete_model = Some(Rc::new(f));
        self
    }

    /// The strict-egress toggle's press (the decision runs in the
    /// section state; the host applies the direct branch).
    pub fn on_toggle_strict_egress(
        mut self,
        f: impl Fn(&mut S, bool) + 'static,
    ) -> Self {
        self.on_toggle_strict_egress = Some(Rc::new(f));
        self
    }

    /// The relaxed-egress confirm's resolution: clear the pending
    /// request and apply the write on OK.
    pub fn on_egress_confirm_resolved(
        mut self,
        f: impl Fn(&mut S, bool) + 'static,
    ) -> Self {
        self.on_egress_confirm_resolved = Some(Rc::new(f));
        self
    }

    /// Build the section. The tooltips, the model dialog and the
    /// relaxed-egress confirm land in `layer` (the floating overlay the
    /// caller composes).
    pub fn build(self, s: &S, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let key = self.key;
        let state = self.state;

        // The store's projections.
        let profiles: &[AiModelProfile] =
            self.settings_config.map(|c| c.profiles.as_slice()).unwrap_or(&[]);
        let active_profile_id =
            self.settings_config.map(|c| c.active_profile_id.as_str()).unwrap_or("");

        let mut section = col().key(key.to_owned()).w_full().gap(24.0);

        // React's first SectionTitle is t('context.openDir') — verbatim,
        // odd as a directory-setting title.
        section = section.child(section_title(
            &k,
            layer,
            format!("{key}-dir-title"),
            t("context.openDir"),
            "",
        ));

        // ContextDirSetting: PathInput (grow) + save + open-dir.
        let mut dir_row = row().key(format!("{key}-dir")).w_full().gap(8.0).center_y();
        let mut field = TextField::new(format!("{key}-dir-input"), &state.context_dir_draft)
            .height(28.0)
            .grow();
        if let Some(edit) = &self.context_dir_edit {
            let edit = edit.clone();
            field = field.on_edit(move |s: &mut S| edit(s));
        }
        // placeholder: defaultDir || t('context.dirSettingDesc')
        let placeholder = state
            .default_context_dir
            .clone()
            .unwrap_or_else(|| t("context.dirSettingDesc").to_owned());
        field = field.placeholder(placeholder);
        dir_row = dir_row.child(field.build(dark));

        let save_label = if self.context_dir_saved {
            t("common.success")
        } else {
            t("common.save")
        };
        let mut save_button = Button::new(save_label)
            .variant(ButtonVariant::Outline) // React secondary (T022 mapping)
            .control_size(ControlSize::Large);
        if let Some(on_save) = &self.on_save_context_dir {
            let on_save = Rc::clone(on_save);
            save_button = save_button.on_press_tx(move |s: &mut S, tx: &mut Tx| on_save(s, tx));
        }
        dir_row = dir_row.child(save_button.build(&k).key(format!("{key}-dir-save")));

        let mut open_button = IconButton::new(&ICON_FOLDER_OPEN)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Large)
            .label(t("context.openDir")); // React's title + aria-label
        if let Some(on_open) = &self.on_open_context_dir {
            let on_open = Rc::clone(on_open);
            open_button = open_button.on_press(move |s: &mut S| on_open(s));
        }
        dir_row = dir_row.child(open_button.build(&k).key(format!("{key}-dir-open")));
        section = section.child(dir_row);

        // The browse affordance (PathInput's onBrowse / pickPath): a
        // trailing secondary button under the row.
        if let Some(on_browse) = &self.on_browse_context_dir {
            let on_browse = Rc::clone(on_browse);
            section = section.child(
                Button::new(t("common.browse"))
                    .variant(ButtonVariant::Outline) // React secondary
                    .control_size(ControlSize::Large)
                    .on_press(move |s: &mut S| on_browse(s))
                    .build(&k)
                    .key(format!("{key}-dir-browse")),
            );
        }

        // The configured content sections: `{settingsConfig && <…/>}`.
        if self.settings_config.is_some() {
            if let Some(context) = self.context_settings {
                section = section.child(context);
            }
            if let Some(skills) = self.skills_section {
                section = section.child(skills);
            }
            if let Some(memory) = self.memory_section {
                section = section.child(memory);
            }
        }
        // AiToolBridgeSection mounts unconditionally.
        if let Some(bridge) = self.tool_bridge_section {
            section = section.child(bridge);
        }

        // The models header: title + hint, the configured badge, the add
        // button.
        let header_title = section_title(
            &k,
            layer,
            format!("{key}-models-title"),
            t("settings.ai.modelsTitle"),
            t("settings.ai.description"),
        );
        let badge = text(
            if self.is_configured {
                t("settings.ai.configured")
            } else {
                t("settings.ai.notConfigured")
            }
            .to_owned(),
        )
        .font(tok::type_::LABEL)
        .color(k.c.text_muted)
        .key(format!("{key}-configured"));
        let mut add_button = Button::new(t("settings.ai.addModel"))
            .variant(ButtonVariant::Submit) // React primary (T022 mapping)
            .control_size(ControlSize::Small)
            .icon(&ICON_PLUS);
        if let Some(on_add) = &self.on_add_model {
            let on_add = Rc::clone(on_add);
            add_button = add_button.on_press(move |s: &mut S| on_add(s));
        }
        section = section.child(
            row()
                .key(format!("{key}-models-header"))
                .w_full()
                .gap(12.0)
                .center_y()
                .child(header_title)
                .child(badge)
                .child(add_button.build(&k).key("add-ai-model-button")),
        );

        // The error line (React text-red-500).
        if let Some(error) = self.config_error {
            section = section.child(
                text(error.to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.danger.fg)
                    .selectable(true)
                    .key(format!("{key}-error")),
            );
        }

        // The empty state or the profile list.
        if profiles.is_empty() {
            let mut empty_add = Button::new(t("settings.ai.addModel"))
                .variant(ButtonVariant::Outline) // React secondary
                .control_size(ControlSize::Small)
                .icon(&ICON_PLUS);
            if let Some(on_add) = &self.on_add_model {
                let on_add = Rc::clone(on_add);
                empty_add = empty_add.on_press(move |s: &mut S| on_add(s));
            }
            section = section.child(
                col()
                    .key("ai-models-empty")
                    .w_full()
                    .gap(12.0)
                    .center()
                    .py(32.0)
                    .radius(12.0)
                    .border(1.0, k.c.border_base)
                    .child(icon(&ICON_SPARKLES, 24.0).color(k.c.text_muted))
                    .child(
                        text(t("settings.ai.noModels").to_owned())
                            .font(tok::type_::LABEL)
                            .color(k.c.text_muted),
                    )
                    .child(empty_add.build(&k)),
            );
        } else {
            let mut list = col().key("ai-models-list").w_full();
            for profile in profiles {
                let mut item = ModelProfileListItem::<S>::new(profile)
                    .is_default(profile.is_default == Some(true) || profile.id == active_profile_id)
                    .is_active(profile.id == active_profile_id);
                // The callback slots are `+ 'static` (Rc<dyn Fn>), so the
                // row's payload is cloned out of the borrow before each
                // capture — the press hands the host an owned profile.
                if let Some(on_set_default) = &self.on_set_default {
                    let on_set_default = Rc::clone(on_set_default);
                    let owned = profile.clone();
                    item = item.on_set_default(move |s: &mut S| {
                        on_set_default(s, owned.clone());
                    });
                }
                if let Some(on_edit) = &self.on_edit_model {
                    let on_edit = Rc::clone(on_edit);
                    let owned = profile.clone();
                    item = item.on_edit(move |s: &mut S| on_edit(s, owned.clone()));
                }
                if let Some(on_duplicate) = &self.on_duplicate_model {
                    let on_duplicate = Rc::clone(on_duplicate);
                    let owned = profile.clone();
                    item = item.on_duplicate(move |s: &mut S| {
                        on_duplicate(s, owned.clone());
                    });
                }
                if let Some(on_delete) = &self.on_delete_model {
                    let on_delete = Rc::clone(on_delete);
                    let id = profile.id.clone();
                    item = item.on_delete(move |s: &mut S| on_delete(s, id.clone()));
                }
                list = list.child(item.build(dark));
            }
            section = section.child(list);
        }

        // The global strict-egress safeguard (checked = `!== false`).
        let checked = self.strict_egress != Some(false);
        let mut egress = Checkbox::new(format!("{key}-egress"), checked)
            .label(t("settings.ai.strictEgress"))
            .description(t("settings.ai.strictEgressHint"));
        if let Some(on_toggle) = &self.on_toggle_strict_egress {
            let on_toggle = Rc::clone(on_toggle);
            egress = egress.on_toggle(move |s: &mut S| on_toggle(s, !checked));
        }
        section = section.child(egress.build(&k));

        // The model-profile dialog (the layer, only while open — React's
        // `{dialogOpen && <ModelProfileDialog …/>}`).
        if state.dialog_open {
            if let Some(build_dialog) = &self.model_dialog {
                build_dialog(s, dark, layer);
            }
        }

        // The relaxed-egress confirm (React's trailing
        // `{confirmRelaxedEgressDialog}`).
        let mut confirm_host = ConfirmDialogHost::<S>::new(&state.egress_confirm);
        if let Some(on_resolved) = &self.on_egress_confirm_resolved {
            let on_resolved = Rc::clone(on_resolved);
            confirm_host = confirm_host.on_resolved(move |s: &mut S, result: bool| {
                on_resolved(s, result);
            });
        }
        confirm_host.build(dark, layer);

        section
    }
}

/// React's SectionTitle stand-in (settingsUi.tsx is T399's): 13/600
/// muted, the hint as a "?" tooltip trigger — the `ai_context_settings`
/// precedent, verbatim. Shared by the settings subtree's sections.
pub fn section_title(
    k: &Kit,
    layer: &mut Vec<El>,
    key: String,
    title: &str,
    hint: &str,
) -> El {
    let mut heading = row()
        .key(key.clone())
        .w_full()
        .gap(4.0)
        .center_y()
        .child(text(title.to_owned()).font(tok::type_::TITLE).color(k.c.text_muted));
    if !hint.is_empty() {
        let mark = row()
            .key(format!("{key}-mark"))
            .size(16.0, 16.0)
            .center()
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .child(text("?".to_owned()).font(tok::type_::LABEL).color(k.c.text_muted));
        heading = heading.child(
            Tooltip::new(format!("{key}-hint"))
                .trigger(mark)
                .text(hint.to_owned())
                .build(k, layer),
        );
    }
    heading
}
