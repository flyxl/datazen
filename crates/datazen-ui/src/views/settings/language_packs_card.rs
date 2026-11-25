//! Port of `packages/frontend/src/windows/settings/LanguagePacksCard.tsx`
//! (T321) — Settings → General → Language packs.
//!
//! The list of installed external language packs with install / remove.
//! Removing the *active* pack first switches the UI language back to
//! English (builtin — always available), then uninstalls: the
//! `updateSettings({ language: 'en' })` write drives
//! `startLocaleSync` → `setLocale('en')`, so the UI switches before the
//! dictionary is dropped.
//!
//! ## What maps where
//!
//! - `useLanguagePackStore` (segment-3's T253) is the host seam: the
//!   card's [`LanguagePacksCardState`] holds the store's projections
//!   (`packs` / `loaded` / `store_error`) delivered by the host, and
//!   the mount fetch (`useEffect` → `fetch()`) is
//!   [`LanguagePacksCardState::on_mount`] — the request the host runs
//!   (T331's `list_language_packs_request`).
//! - `requestRemoval` is [`LanguagePacksCardState::request_removal`]:
//!   the confirm dialog's subject (code, label, and whether the pack
//!   is the active language — `code === activeLanguage`).
//! - `handleRemove` is [`LanguagePacksCardState::begin_remove`] +
//!   [`RemovalPlan`] + the `note_*` adoptions: the gate
//!   (`!pendingRemoval || removing` refuses), the
//!   **English-fallback-FIRST** rule (the active pack persists
//!   `language: 'en'` before the removal — the plan's ordering is the
//!   contract), `unloadLanguagePack(code)` (segment-2's T549, the
//!   host's runtime half), and the finish (`note_remove_ok` clears
//!   the subject; `note_remove_error` surfaces the message inline).
//! - `settings?.language ?? 'en'` is host-supplied
//!   ([`LanguagePacksCardState::active_language`] reads the field the
//!   host keeps current).
//! - `InstallLanguagePackDialog` is T285's builder, composed with the
//!   card's `install_open` flag; the removal confirm is the kit
//!   `Dialog` (the `onClose`-while-removing gate included).
//! - `SectionTitle` / `SettingRow` / `SettingHint` are T399 / T381's.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim:
//!
//! | part | key |
//! |------|-----|
//! | root card | `settings-language-packs-card` |
//! | install button | `language-pack-install-open` |
//! | the pack list | `language-pack-list` |
//! | one pack row | `language-pack-row-{code}` |
//! | the active badge | `language-pack-active-{code}` |
//! | one remove button | `language-pack-remove-{code}` |
//! | the error strip | `language-pack-card-error` |
//! | the confirm button | `language-pack-remove-confirm` |

use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::commands::language_packs::{
    LanguagePackRequest, LanguagePackSummary, list_language_packs_request,
};
use crate::hooks::use_i18n::{I18nParams, UseI18n};
use crate::i18n::t;
use crate::ui::badge::{Badge, BadgeTone};
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::views::settings::install_language_pack_dialog::{
    InstallLanguagePackDialog, InstallLanguagePackDialogState,
};
use crate::views::settings::setting_hint::SettingHint;
use crate::views::settings::settings_ui::{section_title, setting_row};

/// The confirm dialog's subject (React `PendingRemoval`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingRemoval {
    /// The pack code awaiting removal.
    pub code: String,
    pub label: String,
    /// Whether the pack is the active language.
    pub is_active: bool,
}

/// What the host executes for a confirmed removal — the ordering IS
/// the contract (the active pack's English fallback persists FIRST).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemovalPlan {
    /// `!pendingRemoval || removing`: nothing happens.
    Refused,
    /// The active pack: persist `language: 'en'` first (the UI
    /// switches before the dictionary drops), then remove the pack,
    /// then unload it from the runtime.
    SwitchToEnglishThenRemove { code: String },
    /// An inactive pack: remove, then unload.
    Remove { code: String },
}

/// The card's state (React's `useState` fields plus the store's
/// delivered projections).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LanguagePacksCardState {
    /// `packs` (the store's list, host-delivered).
    pub packs: Vec<LanguagePackSummary>,
    /// `loaded` (the store's first-fetch flag).
    pub loaded: bool,
    /// `storeError` (the store's own error — shown when the card has
    /// no fresher one).
    pub store_error: Option<String>,
    /// `installOpen`.
    pub install_open: bool,
    /// `pendingRemoval`.
    pub pending_removal: Option<PendingRemoval>,
    /// `removing`.
    pub removing: bool,
    /// `error` (the removal flow's own error).
    pub error: Option<String>,
    /// `settings?.language ?? 'en'` — the host keeps it current.
    pub active_language: String,
}

impl LanguagePacksCardState {
    /// React's start: English, nothing fetched, nothing pending.
    pub fn new() -> Self {
        Self { active_language: "en".to_owned(), ..Default::default() }
    }

    /// The mount effect: the fetch request the host runs (settings can
    /// be opened after a pack was installed in another window — the
    /// list must never be a stale snapshot).
    pub fn on_mount(&self) -> LanguagePackRequest {
        list_language_packs_request()
    }

    /// The store's fetch answer.
    pub fn on_packs(&mut self, packs: Vec<LanguagePackSummary>) {
        self.packs = packs;
        self.loaded = true;
    }

    /// The store's fetch error.
    pub fn on_packs_error(&mut self, message: String) {
        self.store_error = Some(message);
        self.loaded = true;
    }

    /// `requestRemoval(code, label)`: the confirm dialog's subject,
    /// with the active-language answer decided now (the dialog's
    /// description reads it).
    pub fn request_removal(&mut self, code: &str, label: &str) {
        self.error = None;
        self.pending_removal = Some(PendingRemoval {
            code: code.to_owned(),
            label: label.to_owned(),
            is_active: code == self.active_language,
        });
    }

    /// `handleRemove`'s begin: the gate, then the plan. The `removing`
    /// flag is armed for every accepted plan; the host reports back
    /// through `note_remove_ok` / `note_remove_error`.
    pub fn begin_remove(&mut self) -> RemovalPlan {
        let Some(pending) = self.pending_removal.clone() else {
            return RemovalPlan::Refused;
        };
        if self.removing {
            return RemovalPlan::Refused;
        }
        self.removing = true;
        self.error = None;
        if pending.is_active {
            // The English fallback persists FIRST: the settingsStore
            // change drives `startLocaleSync` → `setLocale('en')`, so
            // the UI switches before the dictionary is dropped.
            RemovalPlan::SwitchToEnglishThenRemove { code: pending.code }
        } else {
            RemovalPlan::Remove { code: pending.code }
        }
    }

    /// The removal succeeded: the subject clears (the `removing` flag
    /// ends on both paths — React's `finally`).
    pub fn note_remove_ok(&mut self) {
        self.pending_removal = None;
        self.removing = false;
    }

    /// The removal failed: the message surfaces inline (the dialog
    /// stays open with the subject — a retry is a fresh confirm).
    pub fn note_remove_error(&mut self, message: String) {
        self.error = Some(message);
        self.removing = false;
    }

    /// The dialog's cancel / onClose (gated by `removing` in React:
    /// `onClose={() => (removing ? undefined : setPendingRemoval(null))}`).
    pub fn cancel_removal(&mut self) {
        if !self.removing {
            self.pending_removal = None;
        }
    }

    /// The card's error display: the flow's own error first, the
    /// store's second (`error ?? storeError`).
    pub fn error_display(&self) -> Option<&str> {
        self.error.as_deref().or(self.store_error.as_deref())
    }

    /// The install dialog's open flag.
    pub fn open_install(&mut self) {
        self.install_open = true;
    }

    /// `onClose` of the install dialog.
    pub fn close_install(&mut self) {
        self.install_open = false;
    }
}

/// `LanguagePacksCard()`: the card, built over the state (plus the
/// install dialog's own state) and the host actions.
pub struct LanguagePacksCard<'a, S: 'static> {
    /// The card state.
    pub state: &'a LanguagePacksCardState,
    /// The install dialog's own state (T285).
    pub install: &'a InstallLanguagePackDialogState,
    /// The install dialog's actions, forwarded.
    pub on_install_close: Option<Rc<dyn Fn(&mut S)>>,
    pub on_install_pick: Option<Rc<dyn Fn(&mut S)>>,
    pub on_install_back: Option<Rc<dyn Fn(&mut S)>>,
    pub on_install_confirm: Option<Rc<dyn Fn(&mut S)>>,
    /// The install-open button.
    pub on_open_install: Option<Rc<dyn Fn(&mut S)>>,
    /// One pack's remove button (`requestRemoval`).
    pub on_request_removal: Option<Rc<dyn Fn(&mut S, String)>>,
    /// The confirm dialog's cancel.
    pub on_cancel_removal: Option<Rc<dyn Fn(&mut S)>>,
    /// The confirm dialog's confirm (`handleRemove`).
    pub on_confirm_removal: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> LanguagePacksCard<'a, S> {
    /// React's component over the two states: every action unwired.
    pub fn new(
        state: &'a LanguagePacksCardState,
        install: &'a InstallLanguagePackDialogState,
    ) -> Self {
        Self {
            state,
            install,
            on_install_close: None,
            on_install_pick: None,
            on_install_back: None,
            on_install_confirm: None,
            on_open_install: None,
            on_request_removal: None,
            on_cancel_removal: None,
            on_confirm_removal: None,
        }
    }

    /// The install dialog's close.
    pub fn on_install_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_install_close = Some(Rc::new(f));
        self
    }

    /// The install dialog's pick step.
    pub fn on_install_pick(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_install_pick = Some(Rc::new(f));
        self
    }

    /// The install dialog's review-step back.
    pub fn on_install_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_install_back = Some(Rc::new(f));
        self
    }

    /// The install dialog's confirm.
    pub fn on_install_confirm(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_install_confirm = Some(Rc::new(f));
        self
    }
    /// `setInstallOpen(true)`.
    pub fn on_open_install(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_install = Some(Rc::new(f));
        self
    }

    /// One pack's remove button.
    pub fn on_request_removal(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_request_removal = Some(Rc::new(f));
        self
    }

    /// The confirm dialog's cancel (`setPendingRemoval(null)`).
    pub fn on_cancel_removal(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel_removal = Some(Rc::new(f));
        self
    }

    /// The confirm dialog's confirm (`handleRemove`).
    pub fn on_confirm_removal(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_removal = Some(Rc::new(f));
        self
    }

    /// The card body; the two dialogs land in `layer` (the kit's
    /// overlay convention).
    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let i18n = UseI18n::new();
        let state = self.state;

        let mut body = col().key("settings-language-packs-card").w_full().gap(16.0);

        body = body.child(section_title(
            k,
            layer,
            "settings-language-packs-title".to_owned(),
            t("settings.languagePacks.title"),
            Some(t("settings.languagePacks.subtitle")),
        ));

        // The install row: the empty hint only once the first fetch
        // answered with nothing.
        let empty_hint = (state.packs.is_empty() && state.loaded)
            .then(|| t("settings.languagePacks.emptyHint").to_owned());
        let mut install = Button::new(t("settings.languagePacks.install.button").to_owned())
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Regular);
        if let Some(f) = &self.on_open_install {
            let f = Rc::clone(f);
            install = install.on_press(move |s: &mut S| f(s));
        }
        body = body.child(setting_row(
            k,
            layer,
            "settings-language-packs-installed".to_owned(),
            t("settings.languagePacks.installed"),
            empty_hint.as_deref(),
            install.build(&k).key("language-pack-install-open"),
        ));

        // The pack list: `divide-y` rows inside one rounded border.
        if !state.packs.is_empty() {
            let mut list = col()
                .key("language-pack-list")
                .w_full()
                .radius(k.radius.md)
                .border(1.0, k.c.border_base);
            let mut first = true;
            for pack in &state.packs {
                let mut row_el = row()
                    .key(format!("language-pack-row-{}", pack.code))
                    .w_full()
                    .center_y()
                    .px(12.0)
                    .py(8.0)
                    .gap(12.0);
                if !first {
                    row_el = row_el.border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base);
                }
                first = false;

                let mut meta = col().key("language-pack-meta").min_w(0.0).gap(2.0);
                let mut head = row().flex_wrap().gap(8.0).center_y();
                head = head.child(
                    text(pack.label.clone())
                        .font(TextStyle::new(14.0, 400.0))
                        .color(k.c.text_base),
                );
                head = head.child(Badge::new(
                    format!("language-pack-code-{}", pack.code),
                    pack.code.clone(),
                ).build(dark));
                head = head.child(Badge::new(
                    format!("language-pack-version-{}", pack.code),
                    format!("v{}", pack.version),
                ).build(dark));
                if pack.code == state.active_language {
                    head = head.child(
                        Badge::new(
                            format!("language-pack-active-{}", pack.code),
                            t("settings.languagePacks.inUse").to_owned(),
                        )
                        .tone(BadgeTone::Accent)
                        .build(dark),
                    );
                }
                meta = meta.child(head);
                // `{count} messages` · the author, when declared.
                let count_line = i18n.t_with_params(
                    "settings.languagePacks.messageCount",
                    &[("count", &pack.message_count.to_string())],
                );
                let line = match &pack.author {
                    Some(author) => format!("{} · {}", count_line, author),
                    None => count_line,
                };
                meta = meta.child(
                    text(line)
                        .font(TextStyle::new(12.0, 400.0))
                        .color(k.c.text_muted),
                );

                let mut remove = Button::new(t("settings.languagePacks.remove").to_owned())
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small);
                if let Some(f) = &self.on_request_removal {
                    let f = Rc::clone(f);
                    let code = pack.code.clone();
                    remove = remove.on_press(move |s: &mut S| f(s, code.clone()));
                }
                row_el = row_el
                    .child(meta.grow(1.0).min_w(0.0))
                    .child(remove.build(&k).key(format!("language-pack-remove-{}", pack.code)));
                list = list.child(row_el);
            }
            body = body.child(list);
        }

        // The error strip: the flow's error, else the store's.
        if let Some(message) = state.error_display() {
            body = body.child(
                CopyableError::new("language-pack-card-error", message.to_owned())
                    .build::<S>(&k),
            );
        }

        // The install dialog (T285) and the removal confirm.
        let mut install_dialog =
            InstallLanguagePackDialog::<S>::new(self.install, state.install_open);
        if let Some(f) = &self.on_install_close {
            let f = Rc::clone(f);
            install_dialog = install_dialog.on_close(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_install_pick {
            let f = Rc::clone(f);
            install_dialog = install_dialog.on_pick(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_install_back {
            let f = Rc::clone(f);
            install_dialog = install_dialog.on_back(move |s: &mut S| f(s));
        }
        if let Some(f) = &self.on_install_confirm {
            let f = Rc::clone(f);
            install_dialog = install_dialog.on_install(move |s: &mut S| f(s));
        }
        install_dialog.build(dark, layer);

        if let Some(pending) = state.pending_removal.clone() {
            let description = if pending.is_active {
                i18n.t_with_params(
                    "settings.languagePacks.removeActiveDescription",
                    &[("label", pending.label.as_str())],
                )
            } else {
                i18n.t_with_params(
                    "settings.languagePacks.removeDescription",
                    &[("label", pending.label.as_str())],
                )
            };
            let mut dialog = Dialog::new(
                "language-pack-remove-dialog",
                t("settings.languagePacks.removeTitle").to_owned(),
            )
            .open(true)
            .description(description)
            .child(SettingHint::new(
                "language-pack-remove-hint",
                t("settings.languagePacks.removeTitle"),
                t("settings.languagePacks.removeHint"),
            ).build(k, layer));
            // The footer: cancel + the danger confirm, both disabled
            // while removing; the confirm's label swaps to "removing".
            let mut footer = row().gap(8.0);
            let mut cancel = Button::new(t("common.cancel").to_owned())
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Regular)
                .disabled(state.removing);
            if let Some(f) = &self.on_cancel_removal {
                let f = Rc::clone(f);
                cancel = cancel.on_press(move |s: &mut S| f(s));
            }
            footer = footer.child(cancel.build(&k));
            let mut confirm = Button::new(if state.removing {
                t("settings.languagePacks.removing").to_owned()
            } else {
                t("settings.languagePacks.remove").to_owned()
            })
            .variant(ButtonVariant::Danger)
            .control_size(ControlSize::Regular)
            .disabled(state.removing);
            if let Some(f) = &self.on_confirm_removal {
                let f = Rc::clone(f);
                confirm = confirm.on_press(move |s: &mut S| f(s));
            }
            footer = footer.child(confirm.build(&k).key("language-pack-remove-confirm"));
            dialog = dialog.footer(footer);
            // `onClose` is a no-op while removing (React's ternary).
            if !state.removing {
                if let Some(f) = &self.on_cancel_removal {
                    let f = Rc::clone(f);
                    dialog = dialog.on_close(move |s: &mut S| f(s));
                }
            }
            dialog.build(&k, layer);
        }

        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack(code: &str, label: &str, author: Option<&str>) -> LanguagePackSummary {
        LanguagePackSummary {
            code: code.to_owned(),
            label: label.to_owned(),
            version: "1.2.0".to_owned(),
            author: author.map(str::to_owned),
            description: None,
            message_count: 3800,
            file_name: code.to_owned(),
        }
    }

    fn state() -> LanguagePacksCardState {
        LanguagePacksCardState::new()
    }

    #[test]
    fn the_initial_state_is_the_react_initials() {
        let state = state();
        assert_eq!(state.active_language, "en", "settings?.language ?? 'en'");
        assert!(state.packs.is_empty());
        assert!(!state.loaded);
        assert!(state.store_error.is_none());
        assert!(!state.install_open);
        assert!(state.pending_removal.is_none());
        assert!(!state.removing);
        assert!(state.error.is_none());
    }

    #[test]
    fn the_mount_fetch_is_the_list_request() {
        let request = state().on_mount();
        assert_eq!(request.command, "list_language_packs");
        assert_eq!(request.value, serde_json::json!({}));
    }

    #[test]
    fn the_fetch_answer_marks_loaded() {
        let mut state = state();
        state.on_packs(vec![pack("zh-CN", "简体中文", None)]);
        assert!(state.loaded);
        assert_eq!(state.packs.len(), 1);
        let mut state = LanguagePacksCardState::new();
        state.on_packs_error("boom".to_owned());
        assert!(state.loaded, "a failed fetch still answers the flag");
        assert_eq!(state.store_error.as_deref(), Some("boom"));
    }

    #[test]
    fn a_removal_request_decides_active_now() {
        let mut state = state();
        state.active_language = "zh-CN".to_owned();
        state.request_removal("zh-CN", "简体中文");
        assert_eq!(
            state.pending_removal,
            Some(PendingRemoval {
                code: "zh-CN".to_owned(),
                label: "简体中文".to_owned(),
                is_active: true,
            })
        );
        state.request_removal("ja", "日本語");
        assert_eq!(state.pending_removal.as_ref().map(|p| p.is_active), Some(false));
    }

    #[test]
    fn the_removal_plan_persists_english_first_for_the_active_pack() {
        let mut state = state();
        assert_eq!(state.begin_remove(), RemovalPlan::Refused, "nothing pending");
        state.active_language = "zh-CN".to_owned();
        state.request_removal("zh-CN", "简体中文");
        // A second confirm while removing refuses (React's gate).
        state.removing = true;
        assert_eq!(state.begin_remove(), RemovalPlan::Refused);
        state.removing = false;

        assert_eq!(
            state.begin_remove(),
            RemovalPlan::SwitchToEnglishThenRemove { code: "zh-CN".to_owned() },
            "the active pack switches the UI to English BEFORE the dictionary drops"
        );
        assert!(state.removing);
        assert!(state.error.is_none(), "the begin clears a stale error");

        // The inactive pack: no language write.
        state.note_remove_ok();
        assert!(state.pending_removal.is_none());
        assert!(!state.removing);
        state.request_removal("ja", "日本語");
        assert_eq!(state.begin_remove(), RemovalPlan::Remove { code: "ja".to_owned() });
    }

    #[test]
    fn the_removal_finish_adopts_both_outcomes() {
        let mut state = state();
        state.request_removal("ja", "日本語");
        let _ = state.begin_remove();
        state.note_remove_error("boom".to_owned());
        assert_eq!(state.error.as_deref(), Some("boom"));
        assert!(!state.removing, "the finally ends the flag");
        assert!(state.pending_removal.is_some(), "the subject stays for a retry");
        state.note_remove_ok();
        assert!(state.pending_removal.is_none());
    }

    #[test]
    fn the_cancel_is_gated_by_removing() {
        let mut state = state();
        state.request_removal("ja", "日本語");
        state.cancel_removal();
        assert!(state.pending_removal.is_none());
        state.request_removal("ja", "日本語");
        state.removing = true;
        state.cancel_removal();
        assert!(state.pending_removal.is_some(), "no dismiss while removing");
    }

    #[test]
    fn the_error_display_prefers_the_flow_error() {
        let mut state = state();
        assert!(state.error_display().is_none());
        state.on_packs_error("store".to_owned());
        assert_eq!(state.error_display(), Some("store"));
        state.error = Some("flow".to_owned());
        assert_eq!(state.error_display(), Some("flow"), "error ?? storeError");
    }

    #[test]
    fn the_install_dialog_flags() {
        let mut state = state();
        state.open_install();
        assert!(state.install_open);
        state.close_install();
        assert!(!state.install_open);
    }

    #[test]
    fn the_i18n_keys_are_the_react_literals() {
        for key in [
            "settings.languagePacks.title",
            "settings.languagePacks.subtitle",
            "settings.languagePacks.installed",
            "settings.languagePacks.emptyHint",
            "settings.languagePacks.install.button",
            "settings.languagePacks.inUse",
            "settings.languagePacks.messageCount",
            "settings.languagePacks.remove",
            "settings.languagePacks.removeTitle",
            "settings.languagePacks.removeHint",
            "settings.languagePacks.removeActiveDescription",
            "settings.languagePacks.removeDescription",
            "settings.languagePacks.removing",
        ] {
            assert!(crate::i18n::has(key), "{key} must exist in the zh-CN packs");
        }
        // The `{count}` / `{label}` tokens interpolate.
        let i18n = UseI18n::new();
        let count = i18n.t_with_params(
            "settings.languagePacks.messageCount",
            &[("count", "3800")],
        );
        assert!(count.contains("3800"), "the count token rides: {count}");
        let described = i18n.t_with_params(
            "settings.languagePacks.removeDescription",
            &[("label", "简体中文")],
        );
        assert!(described.contains("简体中文"), "the label token rides: {described}");
    }
}
