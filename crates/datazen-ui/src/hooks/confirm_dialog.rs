//! Port of `packages/frontend/src/hooks/useConfirmDialog.tsx`.
//!
//! The imperative confirm hook: `confirm(options)` opens the modal and
//! hands back a promise; OK resolves true, every dismissal path resolves
//! false; a second `confirm` while one is open supersedes it (React's
//! `setState` replaces the pending request and the first resolver leaks
//! unresolved — mirrored here, see [`ConfirmDialogState::open`]).
//!
//! ## What maps where
//!
//! - The React hook's `useState<{options, resolve} | null>` becomes
//!   [`ConfirmDialogState`] in the owning state `S` — the hook IS its
//!   state; the dialog element is [`ConfirmDialogHost::build`] painting
//!   `ui/confirm_dialog.rs`'s base.
//! - React's `Promise<boolean>` cannot cross the immediate-mode boundary:
//!   [`ConfirmDialogState::open`] returns a **request id**, and the host
//!   keeps its resolver keyed by that id. [`ConfirmDialogState::answer`]
//!   takes `(id, result)` and yields `Some(result)` only when the id
//!   matches the pending request — that id match is React's resolve
//!   closure identity, and the only way a stale answer lands is the way a
//!   leaked React resolver could never land. The host resolves, records
//!   the copied feedback for the preview and re-frames.
//! - The dialog's open flag is `pending.is_some()`; `onCancel` covers all
//!   four of React's dismissal paths (footer cancel, header close, Esc,
//!   scrim — `ui/dialog.rs` funnels them through one `on_close`).
//! - `bindConfirmDialog(useConfirmDialog)` (the driver-sdk injection that
//!   lets driver UI gate destructive actions without importing host code)
//!   is host wiring: the host exposes the `open`/`answer` pair at its
//!   seam. Nothing here reaches driver-sdk.
//! - `components/ui/ConfirmDialog.tsx` is a pure re-export shell of
//!   `@datazen/ui`'s `ConfirmDialog` — the `X` classification — so the
//!   presentation lives in `ui/confirm_dialog.rs` (this segment's T038
//!   registry addition: the lucide `triangle-alert` glyph).
//!
//! ## Keys
//!
//! The dialog rides `ui/confirm_dialog.rs`'s keys (default key
//! `"confirm-dialog"`; the data-testids `confirm-dialog-ok` /
//! `-cancel` / `-copy-sql` are the base's).

use std::rc::Rc;

use rune::El;

use crate::ui::confirm_dialog::{ConfirmDialog, ConfirmKind};

/// React `ConfirmOptions`, the fields the dialog renders.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)] // reusable port; wired by the host track that owns confirmations
pub(crate) struct ConfirmOptionsVm {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) confirm_label: Option<String>,
    pub(crate) cancel_label: Option<String>,
    pub(crate) kind: ConfirmKind,
    /// Optional badge/tag displayed next to the title.
    pub(crate) badge: Option<String>,
    /// Optional SQL code preview section.
    pub(crate) code_preview: Option<String>,
    /// Optional longer description displayed below the message.
    pub(crate) description: Option<String>,
}

/// React's `useState<{options, resolve} | null>`: the pending request. The
/// resolve closure's identity is the `id`; nothing else resolves it.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // reusable port; wired by the host track that owns confirmations
pub(crate) struct ConfirmDialogState {
    /// The request id `open` handed out last; 0 = none pending.
    pub(crate) pending_id: u64,
    pub(crate) pending: Option<ConfirmOptionsVm>,
    /// The copied-flag projection for the preview's copy button (the host
    /// records it after `CopyButton` copies; React's `useCopyFeedback`).
    pub(crate) copied: bool,
}

#[allow(dead_code)] // reusable port; wired by the host track that owns confirmations
impl ConfirmDialogState {
    /// React `confirm(options)`: store the request; the returned id is the
    /// host's resolver key. A second call while one is open replaces the
    /// request (React's `setState` does the same — the superseded promise
    /// never resolves).
    pub(crate) fn open(&mut self, options: ConfirmOptionsVm) -> u64 {
        self.pending_id = self.pending_id.saturating_add(1).max(1);
        self.pending = Some(options);
        self.copied = false;
        self.pending_id
    }

    /// React's `handleClose(result)`: resolve `id`'s promise and close.
    /// Returns `Some(result)` when `id` is the pending request (the host
    /// resolves its promise with it); `None` leaves everything untouched —
    /// a stale answer from a superseded request, exactly as React's
    /// replaced resolver can never fire.
    pub(crate) fn answer(&mut self, id: u64, result: bool) -> Option<bool> {
        if self.pending.is_none() || id != self.pending_id {
            return None;
        }
        self.pending = None;
        Some(result)
    }

    /// Whether the dialog paints (`state ? <ConfirmDialog …/> : null`).
    pub(crate) fn is_open(&self) -> bool {
        self.pending.is_some()
    }

    /// Convenience: answer the pending request whatever its id (the
    /// dialog's own buttons know they answer the live request).
    pub(crate) fn answer_pending(&mut self, result: bool) -> Option<bool> {
        self.answer(self.pending_id, result)
    }
}

#[allow(dead_code)] // reusable port; wired by the host track that owns confirmations
type ResolvedFn<S> = Rc<dyn Fn(&mut S, bool) + 'static>;

/// The host-side half of the hook: paints the pending request through the
/// `ui/confirm_dialog.rs` base and forwards the two answers.
#[allow(dead_code)] // reusable port; wired by the host track that owns confirmations
pub(crate) struct ConfirmDialogHost<'a, S: 'static> {
    state: &'a ConfirmDialogState,
    on_resolved: Option<ResolvedFn<S>>,
}

#[allow(dead_code)] // reusable port; wired by the host track that owns confirmations
impl<'a, S: 'static> ConfirmDialogHost<'a, S> {
    /// Paint the pending request (nothing while none is pending).
    pub(crate) fn new(state: &'a ConfirmDialogState) -> Self {
        Self { state, on_resolved: None }
    }

    /// The host's resolution hook: called with the request's result once
    /// the pending request is answered (the promise continuation).
    pub(crate) fn on_resolved(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_resolved = Some(Rc::new(f));
        self
    }

    /// Paint the dialog into `layer` while a request is pending.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        let Some(options) = &self.state.pending else { return };

        let on_resolved = self.on_resolved.clone();
        let forward = move |s: &mut S, result: bool| {
            if let Some(on_resolved) = &on_resolved {
                on_resolved(s, result);
            }
        };

        let mut dialog = ConfirmDialog::<S>::new("confirm-dialog", &options.title, &options.message)
            .kind(options.kind)
            .copied(self.state.copied);
        if let Some(badge) = &options.badge {
            dialog = dialog.badge(badge);
        }
        if let Some(preview) = &options.code_preview {
            dialog = dialog.code_preview(preview);
        }
        if let Some(description) = &options.description {
            dialog = dialog.description(description);
        }
        if let Some(label) = &options.confirm_label {
            dialog = dialog.confirm_label(label);
        }
        if let Some(label) = &options.cancel_label {
            dialog = dialog.cancel_label(label);
        }
        let confirm_forward = forward.clone();
        dialog = dialog.on_confirm(move |s: &mut S| confirm_forward(s, true));
        let cancel_forward = forward;
        dialog = dialog.on_cancel(move |s: &mut S| cancel_forward(s, false));
        dialog.build(dark, layer);
    }
}
