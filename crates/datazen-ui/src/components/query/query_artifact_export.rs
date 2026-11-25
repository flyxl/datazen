//! T057: `components/query/QueryArtifactExport.tsx` ported to Rune.
//!
//! React renders one small export button per published artifact id, plus a
//! "partial result" note when the result was truncated, and a saving / saved /
//! error status line. It renders nothing while the result is still pending or
//! when there is no artifact id at all.
//!
//! The export itself is async and host-owned: React awaits
//! `artifactCommands.exportArtifact(artifactId)` (`export_artifact` IPC,
//! `commands/artifact.ts`), and a cancelled native confirmation/save dialog
//! is a normal exit — busy just clears, with no saved mark and no error. Only
//! a `saved` receipt sets the saved mark; a rejection shows the failure's
//! message, or `query.artifactExport.failed` when the failure carries none.
//! Rune is immediate mode, so the in-flight guard (`inFlight` ref) and the
//! busy / saved / error flags live in the app's state `S` as an
//! [`ArtifactExportState`], and the async call is the host's `on_export`
//! callback: the press handler first runs the host's `begin_export` (which
//! calls [`ArtifactExportState::begin_export`]) and only fires `on_export`
//! when the guard lets it through. The host applies the outcome later with
//! [`ArtifactExportState::finish_export`] /
//! [`ArtifactExportState::fail_export`] on the same state slot.
//!
//! This component has no production consumer yet; it will be wired by the
//! query-view track (React mounts it in
//! `windows/connection/query/QueryTransactionModals.tsx:425`, keyed by
//! execution id). Reusable API items carry per-item `#[allow(dead_code)]`
//! (not a module-wide blanket) until that caller lands.
//!
//! ## Keys (prefix defaults to `query-artifact-export`)
//!
//! React gives every button the same `data-testid`
//! (`query-artifact-export-button`) and tells them apart with
//! `data-artifact-index`; Rune keys must be unique, so the index moves into
//! the key itself.
//!
//! | part | key |
//! |------|-----|
//! | root | `<prefix>` (absent when pending or with no artifact ids) |
//! | export button | `<prefix>-button-<index>` |
//! | button icon | `<prefix>-button-<index>-icon` |
//! | button label | `<prefix>-button-<index>-label` |
//! | partial note | `<prefix>-partial` |
//! | saving status | `<prefix>-saving` |
//! | saved status | `<prefix>-saved` |
//! | error status | `<prefix>-error` |
//!
//! ## Deliberate deviations
//!
//! * The buttons are hand-built rows (`role="button"`, `focusable`,
//!   `disabled`, `on_press`), like the select-all / clear-all rows in
//!   `components/connection/batch_export_parts.rs`, because React's buttons
//!   are tiny text buttons (`px-1.5 py-0.5 text-xs`, hover `bg-surface-raised`)
//!   rather than 24pt kit controls. The hover fill is the kit `hover` token;
//!   the hover-time brightening of the label from `text-fg-secondary` to
//!   `text-fg` has no Rune equivalent on a hand-built row, so the label stays
//!   `text_muted` (the fill still signals hover).
//! * While busy every button is `disabled` (React `disabled={busy}`) and its
//!   press handler is left off entirely, per the `El::disabled` contract;
//!   the `begin_export` guard is the second layer, so a press that somehow
//!   arrives while busy is still a no-op.
//! * The root's bottom border is `border_sides([0, 0, 1, 0], border_base)`
//!   (React `border-b border-edge`); `bg-surface-alt` is `layer_01`, the
//!   same surface the explain-plan tree uses for its alt fill.

use rune::{el, icon, row, text, El, LiveRegion};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::ICON_DOWNLOAD;
use crate::ui::kit::kit;

/// React `ResultCompleteness` (`types/jobs.ts:184`): `'pending' | 'complete'
/// | 'truncated'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by the query-view track
pub enum ResultCompleteness {
    Pending,
    #[default]
    Complete,
    Truncated,
}

#[allow(dead_code)] // reusable component; wired by the query-view track
impl ResultCompleteness {
    /// The wire spelling, for the host that decodes the stream event.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Complete => "complete",
            Self::Truncated => "truncated",
        }
    }

    /// Parses the wire spelling; anything else is not a completeness React
    /// could have produced, so it is `None` rather than a silent default.
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "complete" => Some(Self::Complete),
            "truncated" => Some(Self::Truncated),
            _ => None,
        }
    }
}

/// React `QueryResultArtifacts` (`types/driverCommands.ts:45`): the
/// published execution's artifact ids and how complete its result is.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // reusable component; wired by the query-view track
pub struct QueryResultArtifacts {
    pub execution_id: String,
    pub artifact_ids: Vec<String>,
    pub result_completeness: ResultCompleteness,
}

/// React's `[...new Set(artifacts.artifactIds)]`: duplicates collapse,
/// first-seen order kept, so a repeated id gets exactly one button.
#[allow(dead_code)] // reusable component; wired by the query-view track
pub fn dedup_artifact_ids(artifacts: &QueryResultArtifacts) -> Vec<String> {
    let mut ids: Vec<String> = Vec::with_capacity(artifacts.artifact_ids.len());
    for id in &artifacts.artifact_ids {
        if !ids.iter().any(|seen| seen == id) {
            ids.push(id.clone());
        }
    }
    ids
}

/// React's early return: nothing while the result is pending, and nothing
/// when dedup leaves no artifact id.
#[allow(dead_code)] // reusable component; wired by the query-view track
pub fn should_render(artifacts: &QueryResultArtifacts) -> bool {
    artifacts.result_completeness != ResultCompleteness::Pending
        && !dedup_artifact_ids(artifacts).is_empty()
}

/// One button's label. A lone artifact gets `query.artifactExport.action`;
/// several get `query.artifactExport.numberedAction` with `{index}` replaced
/// by the 1-based position, exactly as React's `t(…, { index: index + 1 })`.
#[allow(dead_code)] // reusable component; wired by the query-view track
pub fn button_label(artifact_count: usize, index: usize) -> String {
    if artifact_count == 1 {
        t("query.artifactExport.action").to_owned()
    } else {
        t("query.artifactExport.numberedAction").replacen("{index}", &(index + 1).to_string(), 1)
    }
}

/// React `ArtifactExportResponse` — the single source is
/// `commands::artifact::ArtifactExportResponse` (T130); this alias keeps
/// the component's original name.
pub use crate::commands::artifact::ArtifactExportResponse as ArtifactExportReceipt;

/// The message a rejection shows: the failure's own message when it has one,
/// else `query.artifactExport.failed` — React's `failure instanceof Error ?
/// failure.message : t('query.artifactExport.failed')`.
#[allow(dead_code)] // reusable component; wired by the query-view track
pub fn export_error_message(failure_message: Option<&str>) -> String {
    match failure_message {
        Some(message) => message.to_owned(),
        None => t("query.artifactExport.failed").to_owned(),
    }
}

/// React's `busy` / `saved` / `error` state plus the `inFlight` ref, kept in
/// the app's state `S` because Rune has no component-local `useState`.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // reusable component; wired by the query-view track
pub struct ArtifactExportState {
    /// An export is running; all buttons are disabled while set.
    pub busy: bool,
    /// The last export finished with a `saved` receipt.
    pub saved: bool,
    /// The last export's rejection message, if it failed.
    pub error: Option<String>,
}

#[allow(dead_code)] // reusable component; wired by the query-view track
impl ArtifactExportState {
    /// React's `exportResult` entry: the `inFlight` guard. Returns `false`
    /// (and changes nothing) when an export is already running; otherwise
    /// marks busy and clears the previous outcome, exactly as React's
    /// `setBusy(true); setSaved(false); setError(null)` before the await.
    pub fn begin_export(&mut self) -> bool {
        if self.busy {
            return false;
        }
        self.busy = true;
        self.saved = false;
        self.error = None;
        true
    }

    /// React's `finally` + receipt handling: busy always clears. Only a
    /// `saved` receipt sets the saved mark; a cancellation leaves the
    /// previous outcome cleared and shows nothing, as React does.
    pub fn finish_export(&mut self, receipt: &ArtifactExportReceipt) {
        self.busy = false;
        if matches!(receipt, ArtifactExportReceipt::Saved { .. }) {
            self.saved = true;
        }
    }

    /// React's `catch`: busy clears and the resolved message shows.
    pub fn fail_export(&mut self, message: impl Into<String>) {
        self.busy = false;
        self.error = Some(message.into());
    }
}

/// Reads the export state out of the app's `S`.
#[allow(dead_code)] // reusable component; wired by the query-view track
pub type ReadExportStateFn<S> = fn(&S) -> &ArtifactExportState;

/// Runs the `inFlight` guard on the app's `S`: the host's one-line call into
/// [`ArtifactExportState::begin_export`]. Returns whether the export may start.
#[allow(dead_code)] // reusable component; wired by the query-view track
pub type BeginExportFn<S> = fn(&mut S) -> bool;

/// Starts the host-owned async export of one artifact id (the
/// `export_artifact` IPC). Only called after [`BeginExportFn`] allowed it.
#[allow(dead_code)] // reusable component; wired by the query-view track
pub type ExportArtifactFn<S> = fn(&mut S, String);

/// The export bar. `read_state` and `begin_export` locate the state slot in
/// the app's `S`; `on_export` is the host's async export starter.
#[allow(dead_code)] // reusable component; wired by the query-view track
pub struct QueryArtifactExport<S: 'static> {
    key: String,
    artifacts: QueryResultArtifacts,
    read_state: ReadExportStateFn<S>,
    begin_export: BeginExportFn<S>,
    on_export: Option<ExportArtifactFn<S>>,
}

#[allow(dead_code)] // reusable component; wired by the query-view track
impl<S: 'static> QueryArtifactExport<S> {
    pub fn new(
        artifacts: QueryResultArtifacts,
        read_state: ReadExportStateFn<S>,
        begin_export: BeginExportFn<S>,
    ) -> Self {
        Self {
            key: "query-artifact-export".to_string(),
            artifacts,
            read_state,
            begin_export,
            on_export: None,
        }
    }

    /// Override the key prefix (two bars can then share one state slot only
    /// if their callers keep the prefixes distinct).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// The host's async export starter; without it the buttons render but
    /// do nothing, like a React bar whose command never resolves.
    pub fn on_export(mut self, on_export: ExportArtifactFn<S>) -> Self {
        self.on_export = Some(on_export);
        self
    }

    /// Builds the bar, or `el()` (nothing) while pending or with no ids.
    pub fn build(self, s: &S, dark: bool) -> El {
        if !should_render(&self.artifacts) {
            return el();
        }
        let k = kit(dark);
        let state = (self.read_state)(s);
        let ids = dedup_artifact_ids(&self.artifacts);
        let count = ids.len();

        // React: `flex shrink-0 flex-wrap items-center gap-2 border-b
        // border-edge bg-surface-alt px-3 py-1.5 text-xs`.
        let mut bar = row()
            .key(self.key.clone())
            .w_full()
            .flex_wrap()
            .items_center()
            .gap(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(k.c.layer_01)
            .px(12.0)
            .py(6.0);

        for (index, artifact_id) in ids.iter().enumerate() {
            bar = bar.child(self.button(k, state, artifact_id, count, index));
        }

        if self.artifacts.result_completeness == ResultCompleteness::Truncated {
            bar = bar.child(
                text(t("query.artifactExport.partial"))
                    .key(format!("{}-partial", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg),
            );
        }
        if state.busy {
            bar = bar.child(
                text(t("query.artifactExport.saving"))
                    .key(format!("{}-saving", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .role("status")
                    .live_region(LiveRegion::Polite),
            );
        }
        if state.saved {
            bar = bar.child(
                text(t("query.artifactExport.saved"))
                    .key(format!("{}-saved", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .role("status")
                    .live_region(LiveRegion::Polite),
            );
        }
        if let Some(error) = &state.error {
            bar = bar.child(
                text(error.clone())
                    .key(format!("{}-error", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .role("alert")
                    .live_region(LiveRegion::Assertive),
            );
        }
        bar
    }

    /// One tiny text button: a 12pt Download glyph plus the label
    /// (`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-fg-secondary
    /// hover:bg-surface-raised hover:text-fg disabled:opacity-50`).
    fn button(
        &self,
        k: &Kit,
        state: &ArtifactExportState,
        artifact_id: &str,
        count: usize,
        index: usize,
    ) -> El {
        let base = format!("{}-button-{index}", self.key);
        let mut button = row()
            .key(base.clone())
            .role("button")
            .focusable(!state.busy)
            .disabled(state.busy)
            .items_center()
            .gap(4.0)
            .px(6.0)
            .py(2.0)
            .radius(k.radius.sm)
            .hover(k.c.hover)
            .child(
                icon(&ICON_DOWNLOAD, 12.0)
                    .key(format!("{base}-icon"))
                    .color(k.c.text_muted)
                    .a11y_hidden(),
            )
            .child(
                text(button_label(count, index))
                    .key(format!("{base}-label"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        if state.busy {
            // React `disabled:opacity-50`; the handler stays off while busy,
            // per the `El::disabled` contract.
            button = button.opacity(0.5);
        } else if let Some(on_export) = self.on_export {
            let id = artifact_id.to_owned();
            let begin = self.begin_export;
            button = button.on_press(move |s: &mut S| {
                if begin(s) {
                    on_export(s, id.clone());
                }
            });
        }
        button
    }
}
