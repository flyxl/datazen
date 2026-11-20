//! Port of `packages/frontend/src/windows/settings/
//! InstallLanguagePackDialog.tsx` (T285) — install an external language
//! pack from a local `.zip` package.
//!
//! The two-step flow (the `InstallWappDialog` mirror): native pick →
//! inspect (validate only, NOTHING written — `pickToken` names the
//! host-held pick) → review the label/code/version, the author line
//! and the per-domain message counts → the explicit confirmation
//! performs the actual install. Filesystem paths never cross IPC.
//!
//! ## What maps where
//!
//! - `languagePackCommands` (T331's family) are seams:
//!   [`InstallLanguagePackDialogState::begin_pick`] arms the inspect,
//!   the host runs the dialog+inspect and answers through
//!   `note_pick`/`note_pick_error`; `begin_install`/`note_install_ok`/
//!   `note_install_error` ride the same shape (the install's own
//!   refetch-and-close / back-to-select-with-error halves).
//! - A cancelled picker (`picked == None`) stays on the select step,
//!   silent (React's bare `return`).
//! - The install success: refetch the store (the broadcast covers the
//!   other windows; the refetch covers this one), back to select,
//!   close. The install failure: back to select with the error inline
//!   (the dialog stays open).
//! - `backToSelect` resets everything (the token, the label, the
//!   preview, the error) — the review step never leaks into a fresh
//!   pick.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `language-pack-review`,
//! `language-pack-review-domains`, `language-pack-domain-{domain}`,
//! `language-pack-review-file`, `language-pack-install-pick`,
//! `language-pack-install-error`, `language-pack-install-back`,
//! `language-pack-install-confirm`.

use std::rc::Rc;

use rune::{col, row, text, tok, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

/// React `InstallStep`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstallStep {
    Select,
    Review,
}

/// React `LanguagePackManifest` (the pack's declared identity).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguagePackManifest {
    pub code: String,
    pub label: String,
    pub version: String,
    pub author: Option<String>,
}

/// React `LanguagePackDomainStat`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguagePackDomainStat {
    pub domain: String,
    pub message_count: u32,
}

/// React `LanguagePackPreview` (the validate-only inspect's answer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LanguagePackPreview {
    pub pick_token: String,
    pub package_label: String,
    pub manifest: LanguagePackManifest,
    pub message_count: u32,
    pub domains: Vec<LanguagePackDomainStat>,
}

/// The dialog's state (React's useState fields, owned).
#[derive(Clone, Default)]
pub(crate) struct InstallLanguagePackDialogState {
    pub step: InstallStep,
    pub pick_token: Option<String>,
    pub package_label: String,
    pub preview: Option<LanguagePackPreview>,
    pub inspecting: bool,
    pub installing: bool,
    pub error: Option<String>,
}

impl Default for InstallStep {
    fn default() -> Self {
        Self::Select
    }
}

impl InstallLanguagePackDialogState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `backToSelect`: reset everything — the token, the label, the
    /// preview and the error.
    pub(crate) fn back_to_select(&mut self) {
        self.step = InstallStep::Select;
        self.pick_token = None;
        self.package_label.clear();
        self.preview = None;
        self.error = None;
    }

    /// `handlePick`'s start: the inspect arms (the guard: never while
    /// inspecting or installing).
    pub(crate) fn begin_pick(&mut self) -> bool {
        if self.inspecting || self.installing {
            return false;
        }
        self.inspecting = true;
        self.error = None;
        true
    }

    /// The inspect's answer: a cancelled picker stays on select,
    /// silent; a preview advances to review.
    pub(crate) fn note_pick(&mut self, picked: Option<LanguagePackPreview>) {
        self.inspecting = false;
        let Some(picked) = picked else { return };
        self.pick_token = Some(picked.pick_token.clone());
        self.package_label = picked.package_label.clone();
        self.preview = Some(picked);
        self.step = InstallStep::Review;
    }

    /// The inspect's rejection: the halves reset, the error inline.
    pub(crate) fn note_pick_error(&mut self, message: String) {
        self.inspecting = false;
        self.pick_token = None;
        self.package_label.clear();
        self.preview = None;
        self.error = Some(message);
    }

    /// `handleInstall`'s start (the guard: a token, a preview, not
    /// already installing).
    pub(crate) fn begin_install(&mut self) -> bool {
        if self.pick_token.is_none() || self.preview.is_none() || self.installing {
            return false;
        }
        self.installing = true;
        self.error = None;
        true
    }

    /// The install succeeded: back to select, the caller closes the
    /// dialog (the store refetch is the host's).
    pub(crate) fn note_install_ok(&mut self) {
        self.installing = false;
        self.back_to_select();
    }

    /// The install failed: back to select with the error inline (the
    /// dialog stays open).
    pub(crate) fn note_install_error(&mut self, message: String) {
        self.installing = false;
        self.back_to_select();
        self.error = Some(message);
    }
}

/// The install dialog (React `InstallLanguagePackDialog`).
pub(crate) struct InstallLanguagePackDialog<'a, S: 'static> {
    state: &'a InstallLanguagePackDialogState,
    open: bool,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
    on_pick: Option<Rc<dyn Fn(&mut S)>>,
    on_back: Option<Rc<dyn Fn(&mut S)>>,
    on_install: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> InstallLanguagePackDialog<'a, S> {
    pub(crate) fn new(state: &'a InstallLanguagePackDialogState, open: bool) -> Self {
        Self { state, open, on_close: None, on_pick: None, on_back: None, on_install: None }
    }

    /// The dialog's close (`onClose`).
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// The pick press (step 1's inspect).
    pub(crate) fn on_pick(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_pick = Some(Rc::new(f));
        self
    }

    /// The back press (review → select).
    pub(crate) fn on_back(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_back = Some(Rc::new(f));
        self
    }

    /// The confirm press (the actual install).
    pub(crate) fn on_install(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_install = Some(Rc::new(f));
        self
    }

    /// Build the dialog (nothing while closed).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        if !self.open {
            return;
        }
        let k = kit(dark);
        let state = self.state;

        let mut dialog = Dialog::new("install-language-pack-dialog", t("settings.languagePacks.install.title"))
            .open(true)
            .description(t("settings.languagePacks.install.description"));

        // The body: the review step's manifest panel or the select
        // step's pick button + error.
        if state.step == InstallStep::Review && state.preview.is_some() {
            let preview = state.preview.as_ref().expect("checked");
            let mut review = col().key("language-pack-review").w_full().gap(12.0);
            review = review.child(
                row().key("language-pack-review-head").gap(8.0).center_y().flex_wrap()
                    .child(text(preview.manifest.label.clone()).font(tok::type_::LABEL).color(k.c.text_muted))
                    .child(
                        text(preview.manifest.code.clone())
                            .font(tok::type_::LABEL)
                            .color(k.c.text_muted)
                            .key("language-pack-review-code"),
                    )
                    .child(
                        text(format!("v{}", preview.manifest.version))
                            .font(tok::type_::LABEL)
                            .color(k.c.text_muted)
                            .key("language-pack-review-version"),
                    )
                    .child(
                        text(
                            t("settings.languagePacks.install.messages")
                                .replace("{count}", &preview.message_count.to_string()),
                        )
                        .font(tok::type_::LABEL)
                        .color(k.c.text_muted),
                    ),
            );
            if let Some(author) = &preview.manifest.author {
                review = review.child(
                    text(t("settings.languagePacks.install.author").replace("{author}", author))
                        .font(tok::type_::LABEL)
                        .color(k.c.text_muted),
                );
            }
            let mut domains = col().key("language-pack-review-domains").gap(6.0);
            domains = domains.child(
                text(t("settings.languagePacks.install.domains").to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.text_muted),
            );
            let mut badges = row().key("language-pack-review-domain-list").gap(6.0).flex_wrap();
            for stat in &preview.domains {
                badges = badges.child(
                    text(format!("{} · {}", stat.domain, stat.message_count))
                        .font(tok::type_::LABEL)
                        .color(k.c.text_muted)
                        .key(format!("language-pack-domain-{}", stat.domain)),
                );
            }
            domains = domains.child(badges);
            review = review.child(domains);
            review = review.child(
                text(state.package_label.clone())
                    .font(tok::type_::LABEL)
                    .color(k.c.text_muted)
                    .key("language-pack-review-file"),
            );
            dialog = dialog.child(review);
        } else {
            let mut pick = Button::new(if state.inspecting {
                t("settings.languagePacks.install.inspecting")
            } else {
                t("settings.languagePacks.install.pick")
            })
            .variant(ButtonVariant::Submit)
            .disabled(state.inspecting || state.installing);
            if let Some(on_pick) = &self.on_pick {
                let on_pick = Rc::clone(on_pick);
                pick = pick.on_press(move |s: &mut S| on_pick(s));
            }
            dialog = dialog.child(col().w_full().gap(8.0).child(pick.build(&k).key("language-pack-install-pick")));
            if let Some(error) = &state.error {
                dialog = dialog.child(
                    CopyableError::new("language-pack-install-error", error)
                        .copy_button(true)
                        .build::<S>(&k),
                );
            }
        }

        // The footer: review offers back + confirm; select offers cancel.
        if state.step == InstallStep::Review {
            let mut back = Button::new(t("settings.languagePacks.install.back"))
                .variant(ButtonVariant::Ghost)
                .disabled(state.installing);
            if let Some(on_back) = &self.on_back {
                let on_back = Rc::clone(on_back);
                back = back.on_press(move |s: &mut S| on_back(s));
            }
            dialog = dialog.footer(back.build(&k).key("language-pack-install-back"));
            let can_install = state.preview.is_some()
                && state.pick_token.is_some()
                && !state.installing;
            let mut confirm = Button::new(if state.installing {
                t("settings.languagePacks.install.installing")
            } else {
                t("settings.languagePacks.install.confirm")
            })
            .variant(ButtonVariant::Submit)
            .disabled(!can_install);
            if let Some(on_install) = &self.on_install {
                let on_install = Rc::clone(on_install);
                confirm = confirm.on_press(move |s: &mut S| on_install(s));
            }
            dialog = dialog.footer(confirm.build(&k).key("language-pack-install-confirm"));
        } else {
            let mut cancel = Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .disabled(state.inspecting);
            if let Some(on_close) = &self.on_close {
                let on_close = Rc::clone(on_close);
                cancel = cancel.on_press(move |s: &mut S| on_close(s));
            }
            dialog = dialog.footer(cancel.build(&k).key("install-language-pack-cancel"));
        }

        if let Some(on_close) = &self.on_close {
            let on_close = Rc::clone(on_close);
            dialog = dialog.on_close(move |s: &mut S| on_close(s));
        }
        dialog.build(&k, layer);
    }
}
