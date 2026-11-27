//! AppState method implementations.

#![allow(unused_imports)]
use crate::i18n::t;
use crate::state_def::*;
use crate::types::Section;
use crate::views::{
    InstallWappStep, WappFilter, WappPackageKind, WappPackagePreview, WappPageSummaryVm,
    WappRemoveRequest, WappSummaryVm,
};
use datazen_ui_api::{WappListDelivery, WappListError};
use rune::{FileDialogOptions, FileDialogResult, Tx};

impl AppState {
    /// T022: open the install-wapp dialog over the window — React's
    /// `installOpen` in `WappManagementPage.tsx`. The open press (the
    /// "安装插件…" action, `extensions.page.install`) is the wapps
    /// management page's install button (`views/wapps/
    /// wapp_management_page.rs`); the root modal layer (`views/modals.rs`)
    /// mounts the dialog once this runs, exactly as React's portal does.
    pub fn open_install_wapp_dialog(&mut self) {
        self.install_wapp_open = true;
        // A new open is a new body for the dialog base, so its scroll
        // viewport starts at the top (`Dialog::open_epoch`).
        self.install_wapp_epoch = self.install_wapp_epoch.wrapping_add(1);
        self.reset_install_wapp_flow();
    }

    /// T022: close the dialog. React unmounts the component on close, so
    /// its local state resets with it; the same reset keeps a reopen at the
    /// select step with no stale pick data or error.
    pub fn close_install_wapp_dialog(&mut self) {
        self.install_wapp_open = false;
        self.reset_install_wapp_flow();
    }

    /// React's `backToSelect`: one step back, the pick data and the error
    /// gone (`InstallWappDialog.tsx:38-44`).
    pub fn back_to_install_wapp_select(&mut self) {
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
    pub fn begin_install_wapp_pick(&mut self, kind: WappPackageKind, tx: &mut Tx) {
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
    pub fn answer_install_wapp_pick(&mut self, result: FileDialogResult) {
        self.install_wapp_inspecting = false;
        match result {
            Ok(None) => {}
            Ok(Some(_path)) => {
                // The validate-only inspect is a host use case
                // (`inspect_wapp_package`) that the Rune adapter does not
                // carry yet — the wapps host wiring is still ahead of the
                // management page port. The path is not echoed back
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
    pub fn install_wapp_inspect_failed(&mut self, error: String) {
        self.install_wapp_preview = None;
        self.install_wapp_error = Some(error);
    }

    /// The inspect answer's success transition: the review step with the
    /// preview the host answered (`handlePick`'s `setPickToken`/
    /// `setPackageLabel`/`setManifest`/`setStep('review')`). No production
    /// caller exists until the wapps host wiring lands; tests pin
    /// the transition the wiring will call.
    #[allow(dead_code)] // the wapps host wiring is the production caller
    pub fn install_wapp_inspected(&mut self, preview: WappPackagePreview) {
        self.install_wapp_preview = Some(preview);
        self.install_wapp_step = InstallWappStep::Review;
    }

    /// React's `handleInstall` (`InstallWappDialog.tsx:71-87`): the
    /// confirmed install. The install use case (`install_wapp`) and the
    /// wapp store's refresh are host concerns the Rune adapter does not
    /// carry yet (the same wiring seam), so the press cannot reach a real
    /// install and runs React's failure branch honestly: back to the
    /// select step with the error showing there (`backToSelect()` then
    /// `setError(...)` in the catch).
    pub fn confirm_install_wapp(&mut self) {
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
    /// `backToSelect()`, `onClose()`, `onInstalled()`. The dialog itself
    /// refreshes the store after a successful install
    /// (`InstallWappDialog.tsx:77`: `await useWappStore.getState().fetch()`),
    /// so the transition runs the same refresh here (the store's fetch seam
    /// below); the `onInstalled` callback is the page's own seam, and no
    /// production caller reaches this transition until the host wiring
    /// lands — tests pin it.
    #[allow(dead_code)] // the wapps host wiring is the production caller
    pub fn install_wapp_succeeded(&mut self) {
        self.install_wapp_installing = false;
        self.close_install_wapp_dialog();
        self.fetch_wapps();
    }

    // ------------------------------------------------------------------
    // The wapps management page (T072): the wapp store's surface and the
    // page's own actions — React `WappManagementPage.tsx` over
    // `stores/wappStore.ts`.

    /// Read the installed list independently of any active database or picker.
    /// Never overwrite the last valid list with an error.
    pub fn fetch_wapps(&mut self) {
        if !self.desktop.wapp_listing_is_read_only() {
            self.wapps_loaded = true;
            self.wapps_error = Some(
                "wapp list needs the live host; the wapps list use case is not wired into this Rune build yet"
                    .into(),
            );
            return;
        }
        if self.wapps_pending.is_some() {
            return;
        }
        self.wapps_generation = self.wapps_generation.wrapping_add(1);
        let generation = self.wapps_generation;
        if self.desktop.begin_wapp_list(generation) {
            self.wapps_pending = Some(generation);
        } else {
            self.wapps_loaded = true;
            self.wapps_list_problem = Some(WappListError::Unavailable);
        }
    }

    pub(crate) fn deliver_wapp_list(&mut self, delivery: WappListDelivery) -> bool {
        if self.wapps_pending != Some(delivery.generation) {
            return false;
        }
        self.wapps_pending = None;
        self.wapps_loaded = true;
        match delivery.result {
            Ok(wapps) => self.wapps_loaded(
                wapps.into_iter().map(|wapp| WappSummaryVm {
                    id: wapp.id,
                    name: wapp.name,
                    version: wapp.version,
                    api_version: wapp.api_version,
                    author: wapp.author,
                    description: wapp.description,
                    icon: wapp.icon,
                    enabled: wapp.enabled,
                    permissions: wapp.permissions.into_iter()
                        .map(|permission| permission.as_str().to_owned()).collect(),
                    pages: wapp.pages.into_iter().map(|page| WappPageSummaryVm {
                        id: page.id,
                        title: page.title,
                        icon: page.icon,
                    }).collect(),
                    themes: wapp.themes.into_iter().map(|theme| theme.id).collect(),
                }).collect(),
            ),
            Err(problem) => self.wapps_list_problem = Some(problem),
        }
        true
    }

    /// Apply a fresh, verified list.
    pub fn wapps_loaded(&mut self, wapps: Vec<WappSummaryVm>) {
        self.wapps = wapps;
        self.wapps_loaded = true;
        self.wapps_snapshot_known = true;
        self.wapps_error = None;
        self.wapps_list_problem = None;
    }

    /// React's `handleToggle` (`WappManagementPage.tsx:175-186`): the
    /// page-side half of the switch press — clear the action error, run the
    /// store's `setEnabled`, and land the store's error in the page's catch
    /// slot. The success branch's `closeByWapp` (disabling removes the
    /// wapp's workspace tabs) is the workspace-tabs seam: this build's
    /// workspace carries table/query tabs only, so no tab exists to close.
    pub fn toggle_wapp(&mut self, wapp_id: &str) {
        self.wapp_action_error = None;
        let was_enabled = self
            .wapps
            .iter()
            .any(|wapp| wapp.id == wapp_id && wapp.enabled);
        if let Err(message) = self.set_wapp_enabled(wapp_id, !was_enabled) {
            self.wapp_action_error = Some(message);
        }
    }

    /// React's store `setEnabled` (`wappStore.ts:32-47`): the optimistic flip
    /// and the store's error slot cleared, then the host command. The enable
    /// use case (`set_wapp_enabled`) has no adapter method, so the flow runs
    /// the catch branch exactly — the refetch (`fetch_wapps`'s own failure
    /// branch lands first), then the command's error over the store's slot,
    /// then the `throw` the caller reads as its action error. The optimistic
    /// flip survives the failed refetch, as React's does (a failed fetch
    /// writes only the error).
    pub fn set_wapp_enabled(&mut self, wapp_id: &str, enabled: bool) -> Result<(), String> {
        if self.desktop.wapp_listing_is_read_only() {
            return Err(t("extensions.page.nativeReadOnly").to_owned());
        }
        for wapp in &mut self.wapps {
            if wapp.id == wapp_id {
                wapp.enabled = enabled;
            }
        }
        self.wapps_error = None;
        self.fetch_wapps();
        let error =
            "wapp enable/disable needs the live host; the wapps enable use case is not wired into this Rune build yet"
                .to_owned();
        self.wapps_error = Some(error.clone());
        Err(error)
    }

    /// React's `handleRemove` press half (`WappManagementPage.tsx:188-203`):
    /// the confirm ask the `useConfirmDialog` gate holds — the uninstall
    /// title and the message with the wapp's name resolved at ask time (the
    /// options object the promise carries). A new ask is a fresh mount
    /// (`wapp_remove_epoch`), so the dialog's body starts at the top.
    pub fn ask_remove_wapp(&mut self, wapp_id: &str) {
        if self.desktop.wapp_listing_is_read_only() {
            return;
        }
        let Some(wapp) = self.wapps.iter().find(|wapp| wapp.id == wapp_id) else {
            // The card only renders wapps the list carries; a vanished row
            // leaves nothing to ask about.
            return;
        };
        let message = t("extensions.page.uninstallMessage").replace("{name}", &wapp.name);
        self.wapp_remove_request = Some(WappRemoveRequest {
            wapp_id: wapp.id.clone(),
            message,
        });
        self.wapp_remove_epoch = self.wapp_remove_epoch.wrapping_add(1);
    }

    /// The confirm dialog's answer (`handleClose(true|false)` resolving the
    /// ask): cancel only clears the ask; confirm clears the page's action
    /// error and runs the store's `remove` (whose seam error lands in the
    /// action slot through the same catch). The dialog unmounts either way.
    pub fn answer_remove_wapp(&mut self, ok: bool) {
        let Some(request) = self.wapp_remove_request.take() else {
            return;
        };
        if !ok {
            // React: `if (!ok) return;` — the wapp stays.
            return;
        }
        self.wapp_action_error = None;
        if let Err(message) = self.remove_wapp(&request.wapp_id) {
            self.wapp_action_error = Some(message);
        }
    }

    /// React's store `remove` (`wappStore.ts:49-57`): the uninstall command.
    /// The remove use case (`remove_wapp`) has no adapter method, so the
    /// flow is the catch branch — the store's error slot, and the `throw`
    /// the page's catch turns into its action error. React's success path
    /// (`closeByWapp` after the remove) is the same workspace-tabs seam
    /// `toggle_wapp` documents.
    pub fn remove_wapp(&mut self, _wapp_id: &str) -> Result<(), String> {
        if self.desktop.wapp_listing_is_read_only() {
            return Err(t("extensions.page.nativeReadOnly").to_owned());
        }
        let error =
            "wapp uninstall needs the live host; the wapps uninstall use case is not wired into this Rune build yet"
                .to_owned();
        self.wapps_error = Some(error.clone());
        Err(error)
    }

    /// The remove answer's success transition — the wiring's call once the
    /// adapter uninstalls: the row leaves the list, the store's slot clears,
    /// and the same refresh React's success path runs (`await get().fetch()`).
    /// No production caller exists yet; tests pin the transition.
    #[allow(dead_code)] // the wapps host wiring is the production caller
    pub fn wapp_removed(&mut self, wapp_id: &str) {
        self.wapps.retain(|wapp| wapp.id != wapp_id);
        self.wapps_error = None;
        self.fetch_wapps();
    }

    /// React's `handleOpen` (`WappManagementPage.tsx:205-207`) over
    /// `openWappPage` (`windows/workspace/workspacePages.ts:76-83`): resolve
    /// the wapp (enabled, with pages — the first page the tab would carry),
    /// then the host switch the caller makes of the answer
    /// (`onOpenInWorkspace()` → the workspace mode). The workspace tab the
    /// resolve opens (`workspaceTabKey(wappId, pageId)` keyed, with the
    /// page's title/icon/version) is the workspace-tabs seam — this build's
    /// workspace carries table/query tabs only, so the switch lands without
    /// it and the resolve's gate still refuses a disabled or page-less wapp.
    pub fn open_wapp_page(&mut self, wapp_id: &str) -> bool {
        if self.desktop.wapp_listing_is_read_only() {
            return false;
        }
        let Some(wapp) = self.wapps.iter().find(|wapp| wapp.id == wapp_id) else {
            return false;
        };
        if !wapp.enabled || wapp.pages.is_empty() {
            return false;
        }
        // The first page exists by the gate above; the tab it would open is
        // the seam's payload (the page's id and title).
        let _ = wapp.pages.first();
        self.select(Section::Workspace);
        true
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
