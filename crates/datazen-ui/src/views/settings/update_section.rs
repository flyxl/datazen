//! Port of
//! `packages/frontend/src/windows/settings/UpdateSection.tsx`
//! (T464) — the update card over the three channels.
//!
//! Returning `null` (as this used to) hid the update story entirely
//! from every non-Basic SKU, which is exactly the confusion that
//! let the "variant silently replaced by Basic" bug go unnoticed:
//! the user saw no update UI and then, if they enabled startup
//! checks on a build that *did* have one, got a different SKU
//! installed after a check. Naming the build and pointing at the
//! matching installer keeps the expectation honest.
//!
//! ## What maps where
//!
//! - The two early returns (`updateChannel === 'none'` → nothing,
//!   `'manual'` → the manual card) are [`UpdateSectionState::face`]
//!   + the builder's two arms.
//! - `handleCheck` is [`Self::begin_check`] (the unsupported guard
//!   and the checking status) + the host's `on_check` slot +
//!   [`Self::note_check_result`].
//! - `handleDownload` is [`Self::begin_download`] + the host's
//!   `on_download` slot (progress through [`Self::note_progress`])
//!   + [`Self::note_download_result`], whose installed arm arms the
//!   `useConfirmDialog` restart prompt ([`Self::confirm`]); the
//!   answer lands in [`Self::note_confirm_answer`] — restart-now
//!   relaunches through the `on_confirm_restart` slot (a failure in
//!   [`Self::note_download_relaunch_error`]), restart-later takes
//!   the hint ([`Self::status`]).
//! - `handleRestart` (the Restart Now button) is
//!   [`Self::begin_restart`] + the `on_restart` slot (a failure in
//!   [`Self::note_restart_error`] — no hint line, unlike the
//!   download path).
//! - `progressLabel` is [`progress_label`] — `total` of `0` is
//!   falsy in React, so a zero total renders the indeterminate
//!   sentence.
//! - `settingsCommands.openPath(RELEASES_URL)` is the
//!   `on_open_releases` slot.
//!
//! ## Keys
//!
//! The React structure's own: `update-section`, the two
//! `section-title` faces, `update-check`, `update-download`
//! (`update-restart` after an install), `update-status`,
//! `update-error`, `update-variant`, `update-open-releases` (the
//! manual card's action).
//!
//! Known deltas: the `select-text` CSS class on the error line has
//! no Rune equivalent; the busy-forever face after a successful
//! relaunch ask is invisible (the app exits).

// No production consumer yet (the settings content's update_section
// slot mounts it once a host wires it); this crate's tests are the
// callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::hooks::confirm_dialog::{ConfirmDialogHost, ConfirmDialogState, ConfirmOptionsVm};
use crate::i18n::t;
use crate::ui::confirm_dialog::ConfirmKind;
use crate::views::settings::settings_ui::section_title;
use crate::views::settings::updater::{
    is_updater_supported, UpdateChannel, UpdateCheckResult, UpdatePhase, UpdateProgress,
};

/// GitHub Releases hosts every SKU's installer, including the ones
/// with no updater channel.
pub(crate) const RELEASES_URL: &str = "https://github.com/flyxl/datazen/releases";

/// `text-xs` — the status, error and variant lines.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);

/// `progressLabel(progress, t)`.
pub(crate) fn progress_label(progress: &UpdateProgress) -> String {
    match progress.phase {
        UpdatePhase::Checking => t("settings.updater.checking").to_owned(),
        UpdatePhase::Downloading => {
            // React `if (progress.total)`: a zero total is falsy —
            // the indeterminate sentence.
            if progress.total.map(|total| total > 0).unwrap_or(false) {
                let pct =
                    ((progress.downloaded as f64 / progress.total.unwrap_or(1) as f64) * 100.0)
                        .round()
                        .min(100.0) as u64;
                t("settings.updater.downloading").replace("{pct}", &pct.to_string())
            } else {
                t("settings.updater.downloadingIndeterminate").to_owned()
            }
        }
        UpdatePhase::Installing => t("settings.updater.installing").to_owned(),
        UpdatePhase::Done => {
            t("settings.updater.readyToRestart").replace("{version}", &progress.version)
        }
    }
}

/// Which card renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateFace {
    /// `updateChannel === 'none'` — nothing at all.
    Hidden,
    /// `updateChannel === 'manual'` — the variant sentence + the
    /// Releases button.
    Manual,
    /// The full check / download / restart card.
    Auto,
}

/// What the confirm answer asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartAsk {
    /// "Restart now" — the host relaunches.
    Relaunch,
    /// "Restart later" — the hint line.
    Stay,
}

/// `UpdateSection`'s state (the busy/status/error triple, the
/// pending restart and the confirm hook's half).
#[derive(Clone, Debug, Default)]
pub(crate) struct UpdateSectionState {
    /// The build's channel (host-injected constant).
    pub(crate) channel: Option<UpdateChannel>,
    /// The build's variant (host-injected constant).
    pub(crate) variant: String,
    /// An action in flight.
    pub(crate) busy: bool,
    /// The status sentence (already the display text).
    pub(crate) status: Option<String>,
    /// The error sentence (already the display text).
    pub(crate) error: Option<String>,
    /// The installed version awaiting a restart.
    pub(crate) pending_restart_version: Option<String>,
    /// `useConfirmDialog`'s state.
    pub(crate) confirm: ConfirmDialogState,
}

impl UpdateSectionState {
    /// The host-injected build constants decide the face.
    pub(crate) fn new(channel: UpdateChannel, variant: &str) -> Self {
        Self {
            channel: Some(channel),
            variant: variant.to_owned(),
            ..Self::default()
        }
    }

    /// Which card renders: `none` → nothing, `manual` → the manual
    /// card, otherwise the full card.
    pub(crate) fn face(&self) -> UpdateFace {
        match self.channel {
            Some(UpdateChannel::None) | None => UpdateFace::Hidden,
            Some(UpdateChannel::Manual) => UpdateFace::Manual,
            Some(UpdateChannel::Auto) => UpdateFace::Auto,
        }
    }

    /// `handleCheck`'s preamble: the unsupported guard (the error,
    /// no invoke) then the busy + checking status. Answers whether
    /// the host should run the check.
    pub(crate) fn begin_check(&mut self) -> bool {
        if !is_updater_supported(self.channel.unwrap_or(UpdateChannel::None)) {
            self.error = Some(t("settings.updater.unavailable").to_owned());
            return false;
        }
        self.busy = true;
        self.error = None;
        self.status = Some(t("settings.updater.checking").to_owned());
        true
    }

    /// `checkForUpdates`'s answer: the three statuses (`installed`
    /// has no branch in the check handler — the status survives).
    pub(crate) fn note_check_result(&mut self, result: &UpdateCheckResult) {
        match result {
            UpdateCheckResult::UpToDate => {
                self.status = Some(t("settings.updater.upToDate").to_owned());
            }
            UpdateCheckResult::Available { version } => {
                self.status =
                    Some(t("settings.updater.available").replace("{version}", version));
            }
            UpdateCheckResult::Error { message } => {
                self.error = Some(message.clone());
                self.status = None;
            }
            UpdateCheckResult::Installed { .. } => {}
        }
        self.busy = false;
    }

    /// `handleDownload`'s preamble: the same unsupported guard, the
    /// busy flag. Answers whether the host should run the download.
    pub(crate) fn begin_download(&mut self) -> bool {
        if !is_updater_supported(self.channel.unwrap_or(UpdateChannel::None)) {
            self.error = Some(t("settings.updater.unavailable").to_owned());
            return false;
        }
        self.busy = true;
        self.error = None;
        true
    }

    /// `downloadAndInstallUpdate`'s progress callback.
    pub(crate) fn note_progress(&mut self, progress: &UpdateProgress) {
        self.status = Some(progress_label(progress));
    }

    /// `downloadAndInstallUpdate`'s answer. The installed arm arms
    /// the restart confirm and returns its request id (busy stays
    /// set until the answer); every other arm clears busy.
    pub(crate) fn note_download_result(&mut self, result: &UpdateCheckResult) -> Option<u64> {
        match result {
            UpdateCheckResult::UpToDate => {
                self.status = Some(t("settings.updater.upToDate").to_owned());
                self.busy = false;
                None
            }
            UpdateCheckResult::Installed { version } => {
                self.pending_restart_version = Some(version.clone());
                self.status = Some(
                    t("settings.updater.readyToRestart").replace("{version}", version),
                );
                let message =
                    t("settings.updater.restartPrompt").replace("{version}", version);
                let id = self.confirm.open(ConfirmOptionsVm {
                    title: t("settings.updater.title").to_owned(),
                    message,
                    confirm_label: Some(t("settings.updater.restartNow").to_owned()),
                    cancel_label: Some(t("settings.updater.restartLater").to_owned()),
                    kind: ConfirmKind::Info,
                    ..ConfirmOptionsVm::default()
                });
                Some(id)
            }
            UpdateCheckResult::Error { message } => {
                self.error = Some(message.clone());
                self.status = None;
                self.busy = false;
                None
            }
            UpdateCheckResult::Available { .. } => {
                // No branch in the download handler — just the busy
                // clear.
                self.busy = false;
                None
            }
        }
    }

    /// The restart confirm's answer: "now" relaunches (busy stays
    /// set — the await never cleared it, and the app exits),
    /// "later" takes the hint.
    pub(crate) fn note_confirm_answer(&mut self, answer: bool) -> RestartAsk {
        let _ = self.confirm.answer_pending(answer);
        if answer {
            RestartAsk::Relaunch
        } else {
            let version =
                self.pending_restart_version.clone().unwrap_or_default();
            self.status =
                Some(t("settings.updater.restartLaterHint").replace("{version}", &version));
            self.busy = false;
            RestartAsk::Stay
        }
    }

    /// The download path's relaunch failure: the error, the busy
    /// clear, then the hint line (the `catch` falls through to the
    /// `setStatus` after the `if`).
    pub(crate) fn note_download_relaunch_error(&mut self, message: &str) {
        self.error = Some(message.to_owned());
        self.busy = false;
        let version = self.pending_restart_version.clone().unwrap_or_default();
        self.status =
            Some(t("settings.updater.restartLaterHint").replace("{version}", &version));
    }

    /// `handleRestart`'s preamble: busy + a clean error slate (no
    /// unsupported guard — React's own has none either).
    pub(crate) fn begin_restart(&mut self) -> bool {
        self.busy = true;
        self.error = None;
        true
    }

    /// The restart button's relaunch failure: the error and the
    /// busy clear, no hint line.
    pub(crate) fn note_restart_error(&mut self, message: &str) {
        self.error = Some(message.to_owned());
        self.busy = false;
    }

    /// `ManualUpdateSection`'s variant sentence.
    pub(crate) fn manual_variant_line(&self) -> String {
        t("settings.updater.manualVariant").replace("{variant}", &self.variant)
    }
}

/// `UpdateSection({ … })`: the card over the state, the state lens
/// and the host's action slots.
pub(crate) struct UpdateSection<'a, S: 'static> {
    state: &'a UpdateSectionState,
    dark: bool,
    section_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut UpdateSectionState>,
    on_check: Option<Rc<dyn Fn(&mut S)>>,
    on_download: Option<Rc<dyn Fn(&mut S)>>,
    on_restart: Option<Rc<dyn Fn(&mut S)>>,
    /// The confirm's "restart now" relaunch (distinct from the
    /// button: its failure takes the hint line).
    on_confirm_restart: Option<Rc<dyn Fn(&mut S)>>,
    on_open_releases: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> UpdateSection<'a, S> {
    pub(crate) fn new(
        state: &'a UpdateSectionState,
        dark: bool,
        section_edit: impl for<'b> Fn(&'b mut S) -> &'b mut UpdateSectionState + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            section_edit: Rc::new(section_edit),
            on_check: None,
            on_download: None,
            on_restart: None,
            on_confirm_restart: None,
            on_open_releases: None,
        }
    }

    /// The Check button's `handleCheck`.
    pub(crate) fn on_check(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_check = Some(Rc::new(f));
        self
    }

    /// The Download & Install button's `handleDownload` (the host
    /// reports progress + the result).
    pub(crate) fn on_download(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_download = Some(Rc::new(f));
        self
    }

    /// The Restart Now button's `handleRestart`.
    pub(crate) fn on_restart(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_restart = Some(Rc::new(f));
        self
    }

    /// The restart confirm's "restart now" relaunch.
    pub(crate) fn on_confirm_restart(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_restart = Some(Rc::new(f));
        self
    }

    /// The manual card's `openPath(RELEASES_URL)`.
    pub(crate) fn on_open_releases(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_releases = Some(Rc::new(f));
        self
    }

    /// The card (`None` face → an empty column that lays out
    /// nothing).
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let mut section = col().key("update-section").w_full().gap(12.0);
        match state.face() {
            UpdateFace::Hidden => {} // React `return null`.
            UpdateFace::Manual => {
                section = section.child(section_title(
                    k,
                    layer,
                    "update-section-title".to_owned(),
                    t("settings.updater.title"),
                    Some(t("settings.updater.manualDescription")),
                ));
                section = section.child(
                    text(state.manual_variant_line())
                        .key("update-variant")
                        .font(TEXT_XS)
                        .color(k.c.text_muted),
                );
                let mut open_releases = Button::new(t("settings.updater.openReleases"))
                    .variant(ButtonVariant::Outline) // React secondary
                    .control_size(ControlSize::Small);
                if let Some(on_open_releases) = &self.on_open_releases {
                    let on_open_releases = Rc::clone(on_open_releases);
                    open_releases = open_releases.on_press(move |s: &mut S| on_open_releases(s));
                }
                let actions = row()
                    .key("update-manual-actions")
                    .w_full()
                    .gap(12.0) // gap-3
                    .center_y()
                    .child(open_releases.build(k).key("update-open-releases"));
                section = section.child(actions);
            }
            UpdateFace::Auto => {
                section = section.child(section_title(
                    k,
                    layer,
                    "update-section-title".to_owned(),
                    t("settings.updater.title"),
                    Some(t("settings.updater.description")),
                ));
                let mut check = Button::new(t("settings.updater.check"))
                    .variant(ButtonVariant::Outline) // React secondary
                    .control_size(ControlSize::Small)
                    .disabled(state.busy);
                if let Some(on_check) = &self.on_check {
                    let on_check = Rc::clone(on_check);
                    let section_edit = Rc::clone(&self.section_edit);
                    check = check.on_press(move |s: &mut S| {
                        if section_edit(s).begin_check() {
                            on_check(s);
                        }
                    });
                }
                let primary = if state.pending_restart_version.is_none() {
                    // The Download & Install button.
                    let mut download = Button::new(t("settings.updater.downloadInstall"))
                        .control_size(ControlSize::Small) // React primary
                        .disabled(state.busy);
                    if let Some(on_download) = &self.on_download {
                        let on_download = Rc::clone(on_download);
                        let section_edit = Rc::clone(&self.section_edit);
                        download = download.on_press(move |s: &mut S| {
                            if section_edit(s).begin_download() {
                                on_download(s);
                            }
                        });
                    }
                    download.build(k).key("update-download")
                } else {
                    // The Restart Now button.
                    let mut restart = Button::new(t("settings.updater.restartNow"))
                        .control_size(ControlSize::Small) // React primary
                        .disabled(state.busy);
                    if let Some(on_restart) = &self.on_restart {
                        let on_restart = Rc::clone(on_restart);
                        let section_edit = Rc::clone(&self.section_edit);
                        restart = restart.on_press(move |s: &mut S| {
                            if section_edit(s).begin_restart() {
                                on_restart(s);
                            }
                        });
                    }
                    restart.build(k).key("update-restart")
                };
                let actions = row()
                    .key("update-actions")
                    .w_full()
                    .gap(12.0) // gap-3
                    .center_y()
                    .child(check.build(k).key("update-check"))
                    .child(primary);
                section = section.child(actions);

                if let Some(status) = &state.status {
                    section = section.child(
                        text(status.clone())
                            .key("update-status")
                            .font(TEXT_XS)
                            .color(k.c.text_muted), // text-fg-secondary
                    );
                }
                if let Some(error) = &state.error {
                    section = section.child(
                        text(error.clone())
                            .key("update-error")
                            .font(TEXT_XS)
                            .color(k.c.danger.fg), // text-red-500
                    );
                }
            }
        }
        // `{confirmRestartDialog}` — the shared confirm host.
        let section_edit = Rc::clone(&self.section_edit);
        let on_confirm_restart = self.on_confirm_restart.clone();
        let mut confirm_layer: Vec<El> = Vec::new();
        ConfirmDialogHost::<S>::new(&state.confirm)
            .on_resolved(move |s: &mut S, answer: bool| {
                let ask = section_edit(s).note_confirm_answer(answer);
                if ask == RestartAsk::Relaunch {
                    if let Some(on_confirm_restart) = &on_confirm_restart {
                        on_confirm_restart(s);
                    }
                }
            })
            .build(self.dark, &mut confirm_layer);
        for el in confirm_layer {
            section = section.child(el);
        }
        section
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_face_follows_the_channel() {
        assert_eq!(UpdateSectionState::new(UpdateChannel::None, "all").face(), UpdateFace::Hidden);
        assert_eq!(UpdateSectionState::new(UpdateChannel::Manual, "all").face(), UpdateFace::Manual);
        assert_eq!(UpdateSectionState::new(UpdateChannel::Auto, "all").face(), UpdateFace::Auto);
        // The default (no injected channel) hides — the None arm.
        assert_eq!(UpdateSectionState::default().face(), UpdateFace::Hidden);
    }

    #[test]
    fn an_unsupported_check_refuses_without_busy() {
        let mut state = UpdateSectionState::new(UpdateChannel::Manual, "all");
        assert!(!state.begin_check());
        assert_eq!(state.error.as_deref(), Some(t("settings.updater.unavailable")));
        assert!(!state.busy, "no invoke armed");
        assert_eq!(state.status, None);
        // The manual channel's download refuses the same way.
        assert!(!state.begin_download());
    }

    #[test]
    fn the_check_answers_set_the_status() {
        let mut state = UpdateSectionState::new(UpdateChannel::Auto, "all");
        assert!(state.begin_check());
        assert!(state.busy);
        assert_eq!(state.status.as_deref(), Some(t("settings.updater.checking")));
        state.note_check_result(&UpdateCheckResult::UpToDate);
        assert_eq!(state.status.as_deref(), Some(t("settings.updater.upToDate")));
        assert!(!state.busy);
        assert!(state.begin_check());
        state.note_check_result(&UpdateCheckResult::Available { version: "1.2.0".to_owned() });
        assert_eq!(
            state.status,
            Some(t("settings.updater.available").replace("{version}", "1.2.0"))
        );
        state.note_check_result(&UpdateCheckResult::Error { message: "net down".to_owned() });
        assert_eq!(state.error.as_deref(), Some("net down"));
        assert_eq!(state.status, None, "the error clears the status");
    }

    #[test]
    fn the_progress_label_walks_its_phases() {
        let progress = |phase: UpdatePhase, downloaded: u64, total: Option<u64>, version: &str| {
            UpdateProgress {
                phase,
                downloaded,
                total,
                version: version.to_owned(),
            }
        };
        assert_eq!(
            progress_label(&progress(UpdatePhase::Checking, 0, None, "")),
            t("settings.updater.checking")
        );
        assert_eq!(
            progress_label(&progress(UpdatePhase::Installing, 0, None, "")),
            t("settings.updater.installing")
        );
        // 50 of 200 bytes → 25%.
        assert_eq!(
            progress_label(&progress(UpdatePhase::Downloading, 50, Some(200), "")),
            t("settings.updater.downloading").replace("{pct}", "25")
        );
        // The clamp at 100.
        assert_eq!(
            progress_label(&progress(UpdatePhase::Downloading, 500, Some(200), "")),
            t("settings.updater.downloading").replace("{pct}", "100")
        );
        // No total (or a falsy zero) → the indeterminate sentence.
        assert_eq!(
            progress_label(&progress(UpdatePhase::Downloading, 50, None, "")),
            t("settings.updater.downloadingIndeterminate")
        );
        assert_eq!(
            progress_label(&progress(UpdatePhase::Downloading, 50, Some(0), "")),
            t("settings.updater.downloadingIndeterminate")
        );
        assert_eq!(
            progress_label(&progress(UpdatePhase::Done, 0, None, "1.2.0")),
            t("settings.updater.readyToRestart").replace("{version}", "1.2.0")
        );
    }

    #[test]
    fn the_download_progress_and_settle() {
        let mut state = UpdateSectionState::new(UpdateChannel::Auto, "all");
        assert!(state.begin_download());
        assert!(state.busy);
        state.note_progress(&UpdateProgress {
            phase: UpdatePhase::Downloading,
            downloaded: 10,
            total: Some(100),
            version: String::new(),
        });
        assert_eq!(
            state.status,
            Some(t("settings.updater.downloading").replace("{pct}", "10"))
        );
        // An up-to-date answer settles without a confirm.
        assert_eq!(state.note_download_result(&UpdateCheckResult::UpToDate), None);
        assert!(!state.busy);
        assert_eq!(state.status.as_deref(), Some(t("settings.updater.upToDate")));
    }

    #[test]
    fn an_installed_download_arms_the_restart_confirm() {
        let mut state = UpdateSectionState::new(UpdateChannel::Auto, "all");
        state.begin_download();
        let id = state.note_download_result(&UpdateCheckResult::Installed {
            version: "1.3.0".to_owned(),
        });
        let id = id.expect("the installed arm arms the confirm");
        assert_eq!(state.pending_restart_version.as_deref(), Some("1.3.0"));
        assert_eq!(
            state.status,
            Some(t("settings.updater.readyToRestart").replace("{version}", "1.3.0"))
        );
        assert!(state.busy, "busy until the confirm answers");
        assert!(state.confirm.is_open());
        // The prompt's own text.
        let options = state.confirm.pending.clone().expect("the options");
        assert_eq!(options.title, t("settings.updater.title"));
        assert_eq!(
            options.message,
            t("settings.updater.restartPrompt").replace("{version}", "1.3.0")
        );
        assert_eq!(options.confirm_label.as_deref(), Some(t("settings.updater.restartNow")));
        assert_eq!(options.cancel_label.as_deref(), Some(t("settings.updater.restartLater")));
        // A stale answer (wrong id) never resolves.
        assert_eq!(state.confirm.answer(id + 1, true), None);
    }

    #[test]
    fn restart_later_takes_the_hint() {
        let mut state = UpdateSectionState::new(UpdateChannel::Auto, "all");
        state.begin_download();
        state.note_download_result(&UpdateCheckResult::Installed { version: "1.3.0".to_owned() });
        assert_eq!(state.note_confirm_answer(false), RestartAsk::Stay);
        assert!(!state.busy);
        assert_eq!(
            state.status,
            Some(t("settings.updater.restartLaterHint").replace("{version}", "1.3.0"))
        );
        assert!(!state.confirm.is_open());
        // The pending restart survives (the button face flipped).
        assert_eq!(state.pending_restart_version.as_deref(), Some("1.3.0"));
    }

    #[test]
    fn restart_now_stays_busy_and_a_failure_takes_the_hint() {
        let mut state = UpdateSectionState::new(UpdateChannel::Auto, "all");
        state.begin_download();
        state.note_download_result(&UpdateCheckResult::Installed { version: "1.4.0".to_owned() });
        assert_eq!(state.note_confirm_answer(true), RestartAsk::Relaunch);
        assert!(state.busy, "the await holds it through the relaunch");
        assert!(state.error.is_none());
        // The relaunch failed: the catch's error + hint.
        state.note_download_relaunch_error("relaunch refused");
        assert_eq!(state.error.as_deref(), Some("relaunch refused"));
        assert!(!state.busy);
        assert_eq!(
            state.status,
            Some(t("settings.updater.restartLaterHint").replace("{version}", "1.4.0"))
        );
    }

    #[test]
    fn the_restart_button_path_has_no_hint() {
        let mut state = UpdateSectionState::new(UpdateChannel::Auto, "all");
        state.pending_restart_version = Some("1.4.0".to_owned());
        assert!(state.begin_restart());
        assert!(state.busy);
        assert!(state.error.is_none());
        state.note_restart_error("exit failed");
        assert_eq!(state.error.as_deref(), Some("exit failed"));
        assert!(!state.busy);
        // Unlike the download path: the ready line survives.
        assert_eq!(state.status, None);
    }

    #[test]
    fn the_manual_card_names_the_variant() {
        let state = UpdateSectionState::new(UpdateChannel::Manual, "akulaku");
        assert_eq!(
            state.manual_variant_line(),
            t("settings.updater.manualVariant").replace("{variant}", "akulaku")
        );
    }
}
