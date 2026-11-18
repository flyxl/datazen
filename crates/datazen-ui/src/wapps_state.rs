//! AppState method implementations.

#![allow(unused_imports)]
use crate::i18n::t;
use crate::state_def::*;
use crate::views::{InstallWappStep, WappPackageKind, WappPackagePreview};
use rune::{FileDialogOptions, FileDialogResult, Tx};

impl AppState {
    /// T022: open the install-wapp dialog over the window — React's
    /// `installOpen` in `WappManagementPage.tsx`. The open press (the
    /// "安装插件…" action, `extensions.page.install`) belongs to the wapps
    /// management page port (T073); the root modal layer
    /// (`views/modals.rs`) mounts the dialog once this runs, exactly as
    /// React's portal does.
    #[allow(dead_code)] // the wapps management page (T073) is the production caller
    pub(crate) fn open_install_wapp_dialog(&mut self) {
        self.install_wapp_open = true;
        // A new open is a new body for the dialog base, so its scroll
        // viewport starts at the top (`Dialog::open_epoch`).
        self.install_wapp_epoch = self.install_wapp_epoch.wrapping_add(1);
        self.reset_install_wapp_flow();
    }

    /// T022: close the dialog. React unmounts the component on close, so
    /// its local state resets with it; the same reset keeps a reopen at the
    /// select step with no stale pick data or error.
    pub(crate) fn close_install_wapp_dialog(&mut self) {
        self.install_wapp_open = false;
        self.reset_install_wapp_flow();
    }

    /// React's `backToSelect`: one step back, the pick data and the error
    /// gone (`InstallWappDialog.tsx:38-44`).
    pub(crate) fn back_to_install_wapp_select(&mut self) {
        self.install_wapp_step = InstallWappStep::Select;
        self.install_wapp_preview = None;
        self.install_wapp_error = None;
    }

    /// React's `handlePick(kind)` (`InstallWappDialog.tsx:47-68`): the
    /// native picker runs first, then the host's validate-only inspect.
    /// The in-flight mark (`inspecting`) spans the pick exactly as React's
    /// `await` did, and the pick itself is real — `tx.open_file` with the
    /// host's own zip filter (`commands/wapps/mod.rs`:
    /// `("Wapp package", ["zip"])`), `tx.open_folder` for the directory
    /// pick, whose filters do not apply.
    pub(crate) fn begin_install_wapp_pick(&mut self, kind: WappPackageKind, tx: &mut Tx) {
        if self.install_wapp_inspecting || self.install_wapp_installing {
            // React: `if (inspecting || installing) return;`
            return;
        }
        self.install_wapp_inspecting = true;
        self.install_wapp_error = None;
        let options = FileDialogOptions {
            title: match kind {
                WappPackageKind::Zip => t("extensions.install.browseZip").to_owned(),
                WappPackageKind::Folder => t("extensions.install.browseFolder").to_owned(),
            },
            ..Default::default()
        };
        let options = match kind {
            WappPackageKind::Zip => options.filter("Wapp package", &["zip"]),
            WappPackageKind::Folder => options,
        };
        let answer = |state: &mut AppState, result: FileDialogResult, _: &mut Tx| {
            state.answer_install_wapp_pick(result);
        };
        match kind {
            WappPackageKind::Zip => tx.open_file(options, answer),
            WappPackageKind::Folder => tx.open_folder(options, answer),
        }
    }

    /// The picker's answer (`handlePick`'s `finally` clears `inspecting`
    /// whatever came back): a cancelled pick stays on the select step with
    /// no error (React: `if (!preview) return`), while a picked path and an
    /// unavailable picker both run the inspect seam below.
    pub(crate) fn answer_install_wapp_pick(&mut self, result: FileDialogResult) {
        self.install_wapp_inspecting = false;
        match result {
            Ok(None) => {}
            Ok(Some(_path)) => {
                // The validate-only inspect is a host use case
                // (`inspect_wapp_package`) that the Rune adapter does not
                // carry yet — the wapps host wiring lands with the
                // management page port (T073). The path is not echoed back
                // into an error the way a summary must not carry absolute
                // paths; the answer is the honest error branch React renders
                // from a failed inspect (`handlePick`'s catch).
                self.install_wapp_inspect_failed(
                    "wapp package inspection needs the live host; the wapps install use cases are not wired into this Rune build yet"
                        .into(),
                );
            }
            Err(error) => {
                self.install_wapp_inspect_failed(format!(
                    "the package picker is unavailable ({error}); the install needs a package"
                ));
            }
        }
    }

    /// `handlePick`'s catch: the pick data gone, the error showing
    /// (`setPickToken(null); setPackageLabel(''); setManifest(null);
    /// setError(...)`).
    pub(crate) fn install_wapp_inspect_failed(&mut self, error: String) {
        self.install_wapp_preview = None;
        self.install_wapp_error = Some(error);
    }

    /// The inspect answer's success transition: the review step with the
    /// preview the host answered (`handlePick`'s `setPickToken`/
    /// `setPackageLabel`/`setManifest`/`setStep('review')`). No production
    /// caller exists until the wapps host wiring (T073) lands; tests pin
    /// the transition the wiring will call.
    #[allow(dead_code)] // the wapps host wiring (T073) is the production caller
    pub(crate) fn install_wapp_inspected(&mut self, preview: WappPackagePreview) {
        self.install_wapp_preview = Some(preview);
        self.install_wapp_step = InstallWappStep::Review;
    }

    /// React's `handleInstall` (`InstallWappDialog.tsx:71-87`): the
    /// confirmed install. The install use case (`install_wapp`) and the
    /// wapp store's refresh are host concerns the Rune adapter does not
    /// carry yet (the same T073 wiring), so the press cannot reach a real
    /// install and runs React's failure branch honestly: back to the
    /// select step with the error showing there (`backToSelect()` then
    /// `setError(...)` in the catch).
    pub(crate) fn confirm_install_wapp(&mut self) {
        if self.install_wapp_preview.is_none() || self.install_wapp_installing {
            // React: `if (!pickToken || !manifest || installing) return;`
            return;
        }
        self.install_wapp_installing = true;
        self.install_wapp_error = None;
        // The install seam: no adapter method exists, so the answer is the
        // failure branch, never a silent success.
        self.install_wapp_installing = false;
        self.back_to_install_wapp_select();
        self.install_wapp_error = Some(
            "wapp install needs the live host; the wapps install use case is not wired into this Rune build yet"
                .into(),
        );
    }

    /// The install answer's success transition — React's try branch:
    /// `backToSelect()`, `onClose()`, `onInstalled()`. The store refresh and
    /// the `onInstalled` callback belong to the hosting page (T073), so the
    /// seam's caller here only resets and closes the dialog. No production
    /// caller exists until the host wiring lands; tests pin the transition.
    #[allow(dead_code)] // the wapps host wiring (T073) is the production caller
    pub(crate) fn install_wapp_succeeded(&mut self) {
        self.install_wapp_installing = false;
        self.close_install_wapp_dialog();
    }

    /// The dialog's full reset — React unmounts the component on close, so
    /// every local field starts over; the open and close paths share it.
    fn reset_install_wapp_flow(&mut self) {
        self.install_wapp_step = InstallWappStep::Select;
        self.install_wapp_preview = None;
        self.install_wapp_inspecting = false;
        self.install_wapp_installing = false;
        self.install_wapp_error = None;
    }
}
