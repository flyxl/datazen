//! Port of `packages/frontend/src/windows/wapps/InstallWappDialog.tsx` (T022).
//!
//! React installs a workspace app ("wapp") from a local `.zip` package or an
//! unpacked directory in two steps: a native pick plus a validate-only
//! inspect (nothing is written), then a review of the inspected manifest —
//! package label, name, `v{version} · author`, optional description and one
//! badge per requested permission — before the explicit confirmation
//! performs the install. Filesystem paths never cross the view: the host
//! owns the pick, the inspect and the install.
//!
//! ## Structure (`InstallWappDialog.tsx:89-198`)
//!
//! | React | Rune |
//! |---|---|
//! | `<Dialog open title description onClose footer>` | the [`Dialog`] base, mounted from the root modal layer (`views/modals.rs::modal_layer`) — React portals the dialog to `document.body`, so it floats over the whole window whichever page hosts it, and the root layer is that host here |
//! | `step === 'review' && manifest` → the review body | [`InstallWappStep::Review`] with [`AppState::install_wapp_preview`] |
//! | otherwise the select body: the prompt + zip/folder buttons | the same two buttons, primary and secondary variants |
//! | footer `back` / `confirm`, or `cancel` | the same three kit buttons with the same disabled gates |
//!
//! ## Honest seams (dependencies this port does not include; each belongs to
//! its own later task in the migration ledger)
//!
//! - The wapp host use cases — the validate-only inspect
//!   (`wappCommands.inspectWappPackageWithDialog`), the install
//!   (`installWapp`) and the wapp store's refresh — have no
//!   `DesktopAdapter` methods yet; their wiring lands with the wapps
//!   management page port (T073), which needs the same list/enable/remove
//!   surface. The native picker itself is real here (`tx.open_file` with
//!   the host's own "Wapp package"/zip filter, `tx.open_folder` for the
//!   directory pick — the same pick the React host command performs). The
//!   inspect/install answers land as state mutations
//!   (`install_wapp_inspected` / `install_wapp_succeeded`) for that wiring;
//!   until it lands, a picked path and a confirmed install run React's
//!   error branches honestly (see `wapps_state.rs`).
//! - `PERMISSION_LABELS` — `windows/wapps/permissionLabels.ts` is T484, so
//!   the permission badges render without their tooltip texts until that
//!   port lands.
//! - The open press (`extensions.page.install`, `WappManagementPage.tsx`)
//!   belongs to the page port (T073); `open_install_wapp_dialog` is its
//!   call, and the root modal layer only mounts the dialog once it runs.
//! - The transient "已复制" feedback on the error's copy control needs the
//!   caller's clock state (`ui/copyable.rs`'s documented contract); no
//!   production caller carries one yet (the same residual the batch-export
//!   dialog's error carries). The copy press itself is real.
//! - React's `className="text-sm text-red-400"` on `CopyableError` is a
//!   styling escape hatch the Rune base does not carry as a slot: the base
//!   owns the danger ink and its own type scale.
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | panel / title / close / body / footer / scrim / layer | `install-wapp-dialog*` (from the [`Dialog`] base) |
//! | select body / prompt / pick row | `extension-install-select`, `extension-install-prompt`, `extension-install-picks` |
//! | zip / folder pick | `extension-install-browse-zip`, `extension-install-browse-folder` |
//! | review root / package label / identity | `extension-install-review`, `extension-install-package-label`, `extension-install-identity` |
//! | name / version · author / description | `extension-install-name`, `extension-install-version`, `extension-install-description` |
//! | permissions label / row / badges | `extension-install-permissions-label`, `extension-install-permissions`, `extension-install-permission-<perm>` |
//! | empty permissions | `extension-install-no-permissions` |
//! | error (either step, React's `data-testid`) | `extension-install-error` |
//! | footer back / confirm / cancel | `extension-install-back`, `extension-install-confirm`, `extension-install-cancel` |

use crate::i18n::t;
use crate::state_def::AppState;
use crate::ui::badge::Badge;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use rune::{col, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

/// React's `InstallStep`: `'select' | 'review'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum InstallWappStep {
    #[default]
    Select,
    Review,
}

/// React's `WappPackageKind`: `'zip' | 'folder'` — which native picker the
/// select step's buttons open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WappPackageKind {
    Zip,
    Folder,
}

/// The manifest fields the review step renders — the honest projection of
/// the host's `WappManifest` (`types/wapp.ts`; the full manifest mirror is
/// the host contract, not this view's). The inspect seam (T073) fills it.
#[allow(dead_code)] // projection type; fields are read by host wiring/tests
#[derive(Clone, Debug)]
pub(crate) struct WappManifestVm {
    pub(crate) name: String,
    pub(crate) version: String,
    /// React renders the author only when it is a non-empty string.
    pub(crate) author: Option<String>,
    pub(crate) description: Option<String>,
    /// The requested permission strings, one badge each.
    pub(crate) permissions: Vec<String>,
}

/// The preview the inspect answers with — the projection of the host's
/// `WappPackagePreview` (`commands/wapps.ts`): the pick token names the
/// held package, the label is what the review shows above the name. The
/// three travel together in React (three `useState`s set and cleared as
/// one), so they are one `Option` here.
#[allow(dead_code)] // projection type; fields are read by host wiring/tests
#[derive(Clone, Debug)]
pub(crate) struct WappPackagePreview {
    pub(crate) pick_token: String,
    pub(crate) package_label: String,
    pub(crate) manifest: WappManifestVm,
}

/// The install dialog over the window, branch by branch. The root modal
/// layer (`views/modals.rs::modal_layer`) mounts it while
/// [`AppState::install_wapp_open`] is set, the way React's portal hosts it
/// for the management page.
pub(crate) fn install_wapp_dialog(state: &AppState, dark: bool) -> Dialog {
    let k = kit(dark);
    // React: `step === 'review' && manifest` — a review without a preview
    // falls back to the select body, exactly as the JSX condition does.
    let review =
        state.install_wapp_step == InstallWappStep::Review && state.install_wapp_preview.is_some();

    let mut dialog = Dialog::new("install-wapp-dialog", t("extensions.install.title"))
        .open(true)
        .open_epoch(state.install_wapp_epoch)
        .description(t("extensions.install.description"))
        // React's panel: `w-full max-w-xl` (576pt), content-sized.
        .width(576.0)
        // React caps the body at `max-h-[70vh]`; 448 keeps that cap inside
        // the 640pt minimum window (the import dialog's documented figure
        // for the same React cap).
        .fit()
        .body_max_height(448.0)
        // React's footer row: `py-3` (12pt) around the md button (`h-9`).
        .footer_height(60.0);

    if review {
        dialog = dialog.child(install_wapp_review_body(state, dark));
        dialog = dialog
            .footer(
                Button::new(t("extensions.install.back"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::ExtraLarge)
                    // React: `disabled={installing}` — a back press must not
                    // drop a package mid-install.
                    .disabled(state.install_wapp_installing)
                    .on_press(|state: &mut AppState| state.back_to_install_wapp_select())
                    .build(k)
                    .key("extension-install-back"),
            )
            .footer(
                Button::new(if state.install_wapp_installing {
                    t("extensions.install.installing")
                } else {
                    t("extensions.install.confirm")
                })
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                // React: `disabled={!manifest || !pickToken || installing}`.
                .disabled(
                    state.install_wapp_preview.is_none() || state.install_wapp_installing,
                )
                .on_press(|state: &mut AppState| state.confirm_install_wapp())
                .build(k)
                .key("extension-install-confirm"),
            );
    } else {
        dialog = dialog.child(install_wapp_select_body(state, dark));
        dialog = dialog.footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::ExtraLarge)
                // React: `disabled={inspecting}` — the picker stays up.
                .disabled(state.install_wapp_inspecting)
                .on_press(|state: &mut AppState| state.close_install_wapp_dialog())
                .build(k)
                .key("extension-install-cancel"),
        );
    }

    dialog.on_close(|state: &mut AppState| state.close_install_wapp_dialog())
}

/// The select step: the prompt and the two pick buttons
/// (`InstallWappDialog.tsx:165-195`), plus the error branch a failed pick
/// leaves behind.
fn install_wapp_select_body(state: &AppState, dark: bool) -> El {
    let k = kit(dark);
    let mut body = col().key("extension-install-select").w_full().gap(12.0);
    body = body.child(
        text(t("extensions.install.pickPrompt"))
            .key("extension-install-prompt")
            // React: `text-xs text-fg-secondary`.
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted)
            .wrap(),
    );
    // React: `flex flex-wrap gap-2` around the two picks.
    let mut picks = row()
        .key("extension-install-picks")
        .w_full()
        .gap(8.0)
        .flex_wrap();
    // React's inspecting label swap: both buttons say they are busy while a
    // pick is in flight, and neither accepts another press.
    let busy = t("extensions.install.inspecting");
    picks = picks.child(
        Button::new(if state.install_wapp_inspecting {
            busy
        } else {
            t("extensions.install.browseZip")
        })
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::ExtraLarge)
        .disabled(state.install_wapp_inspecting)
        .on_press_tx(|state: &mut AppState, tx: &mut Tx| {
            state.begin_install_wapp_pick(WappPackageKind::Zip, tx)
        })
        .build(k)
        .key("extension-install-browse-zip"),
    );
    picks = picks.child(
        Button::new(if state.install_wapp_inspecting {
            busy
        } else {
            t("extensions.install.browseFolder")
        })
        .variant(ButtonVariant::Outline)
        .control_size(ControlSize::ExtraLarge)
        .disabled(state.install_wapp_inspecting)
        .on_press_tx(|state: &mut AppState, tx: &mut Tx| {
            state.begin_install_wapp_pick(WappPackageKind::Folder, tx)
        })
        .build(k)
        .key("extension-install-browse-folder"),
    );
    body = body.child(picks);
    if let Some(error) = state.install_wapp_error.clone() {
        body = body.child(
            CopyableError::new("extension-install-error", error)
                .copy_button(true)
                .build::<AppState>(k),
        );
    }
    body
}

/// The review step: the inspected package, branch by branch
/// (`InstallWappDialog.tsx:123-164`).
fn install_wapp_review_body(state: &AppState, dark: bool) -> El {
    // The dialog mounts this branch only with a preview; a review without
    // one keeps the select body (React's `&& manifest` arm).
    let Some(preview) = state.install_wapp_preview.as_ref() else {
        return install_wapp_select_body(state, dark);
    };
    let k = kit(dark);
    let mut body = col().key("extension-install-review").w_full().gap(12.0);
    body = body.child(
        text(preview.package_label.clone())
            .key("extension-install-package-label")
            // React: `text-xs text-fg-muted`.
            .font(k.ty.text_xsmall)
            .color(k.c.text_faint)
            .wrap(),
    );
    // The identity block: the name over the version · author line
    // (`mt-0.5` = 2pt between them).
    let version_author = match preview.manifest.author.as_deref() {
        // React's truthiness: an empty author string renders no suffix.
        Some(author) if !author.is_empty() => {
            format!("v{} · {}", preview.manifest.version, author)
        }
        _ => format!("v{}", preview.manifest.version),
    };
    body = body.child(
        col()
            .key("extension-install-identity")
            .w_full()
            .gap(2.0)
            .child(
                text(preview.manifest.name.clone())
                    .key("extension-install-name")
                    // React: `text-sm font-semibold text-fg`.
                    .font(k.ty.body_medium)
                    .color(k.c.text_base)
                    .wrap(),
            )
            .child(
                text(version_author)
                    .key("extension-install-version")
                    // React: `text-xs text-fg-muted`.
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint)
                    .wrap(),
            ),
    );
    if let Some(description) = preview.manifest.description.clone() {
        // React renders the line only when the manifest carries one
        // (`leading-relaxed` ≈ 1.625 on 12pt ≈ 19.5pt line height).
        body = body.child(
            text(description)
                .key("extension-install-description")
                .font(k.ty.text_xsmall.lh(19.5))
                .color(k.c.text_muted)
                .wrap(),
        );
    }
    // The permissions block: the label over the badge row
    // (`flex flex-col gap-1.5`).
    let mut permissions = col()
        .key("extension-install-permissions-block")
        .w_full()
        .gap(6.0)
        .child(
            text(t("extensions.install.permissions"))
                .key("extension-install-permissions-label")
                // React: `text-xs font-medium text-fg-secondary`.
                .font(k.ty.text_xsmall_medium)
                .color(k.c.text_muted),
        );
    let mut badges = row()
        .key("extension-install-permissions")
        .w_full()
        .gap(4.0)
        .flex_wrap();
    if preview.manifest.permissions.is_empty() {
        badges = badges.child(
            text(t("extensions.install.noPermissions"))
                .key("extension-install-no-permissions")
                // React: `text-xs text-fg-muted`.
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint)
                .wrap(),
        );
    } else {
        // React: one neutral `Badge` per permission string; the tooltip
        // texts (`PERMISSION_LABELS`) are T484's port, so the badges render
        // without them until that lands.
        for perm in preview.manifest.permissions.clone() {
            badges = badges.child(
                Badge::new(format!("extension-install-permission-{perm}"), perm)
                    .build(dark),
            );
        }
    }
    permissions = permissions.child(badges);
    body = body.child(permissions);
    if let Some(error) = state.install_wapp_error.clone() {
        // React carries the same error branch in the review body; no React
        // path sets an error without leaving the review step, but the JSX
        // renders it if one ever does.
        body = body.child(
            CopyableError::new("extension-install-error", error)
                .copy_button(true)
                .build::<AppState>(k),
        );
    }
    body
}
