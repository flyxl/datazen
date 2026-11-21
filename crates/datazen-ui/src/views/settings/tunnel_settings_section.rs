//! Port of
//! `packages/frontend/src/windows/settings/TunnelSettingsSection.tsx`
//! (T453) — Settings → Tunnels: the management surface for saved
//! tunnels (G2).
//!
//! The list renders exclusively from the summaries — the secret-free
//! `get_tunnel_summaries` projection. `getTunnels` (which returns
//! decrypted credentials) is never called from a render path; the
//! only full-entity fetch is `getTunnel(id)` when the edit dialog
//! opens (G9), and the copy action's own read.
//!
//! ## What maps where
//!
//! - The mount's `load()` + the store faces (`summaries` /
//!   `loaded` / `loading` / `error`) are
//!   [`TunnelSettingsSectionState::note_store`]: the host drives the
//!   tunnel store and reports its faces.
//! - The usage batch effect (one concurrent `allSettled` over the
//!   summaries — a serial per-row fetch would be an N+1 stall on a
//!   settings page, and the collection is small enough that
//!   `allSettled` keeps a single failing probe from hiding the
//!   counts of every other tunnel) is
//!   [`Self::usage_batch_due`] + [`Self::note_usage_batch`]: the
//!   host runs the batch when due and reports the fulfilled map; an
//!   empty summary list clears the map and the pending face without
//!   a read.
//! - `handleCopy` is [`Self::begin_copy`] +
//!   [`Self::note_copy_missing`] / [`Self::note_copy_ok`] /
//!   [`Self::note_copy_error`]: the host reads the full entity (the
//!   copy's own G9 exception), creates the clone (the minted id +
//!   `copyTunnelName` with the locale suffix), and reloads on a
//!   missing entity.
//! - `performDelete` is the host's slot over T441's absorbed
//!   `delete_tunnel_and_unbind` (unbind first, delete second — the
//!   backend delete does not clean up references, so this ordering
//!   is what keeps a delete from leaving dangling `tunnelId`s
//!   behind); the non-unbind-failed outcomes refresh the
//!   navigator's connection view, and every outcome reloads.
//! - The three dialogs are the T447 / T441 / **T459 (not landed)**
//!   ports: the edit and delete states are the section state's own
//!   `edit_target` / `delete_target` / `test_target` (`None` =
//!   closed; `{ id: null }` = the create form), the test dialog is
//!   the delegated `test_dialog` slot (the structural marker until
//!   T459 lands).
//! - `usageText` / `usageTitle` are [`Self::usage_text`] /
//!   [`Self::usage_title`] (the `{count}` placeholder formatted;
//!   the title is the joined names, `None` when there are none).
//!
//! ## Keys
//!
//! The React `data-testid`s stay verbatim (`settings-tunnels-section`,
//! `tunnel-create`, `tunnel-load-error`, `tunnel-action-error`,
//! `tunnel-loading`, `tunnel-list-empty`, `tunnel-list`, `tunnel-row`,
//! `tunnel-row-name`, `tunnel-row-kind`, `tunnel-row-usage`,
//! `tunnel-edit-{id}`, `tunnel-copy-{id}`, `tunnel-test-{id}`,
//! `tunnel-delete-{id}`).
//!
//! Known deltas: the repeated `tunnel-row` testid becomes the
//! runtime-suffixed duplicate keys (the established mirror); the
//! delete button's red face rides the danger tone.

// No production consumer yet (the settings content shell T390
// mounts it through its tunnels slot); this crate's tests are the
// callers meanwhile.
#![allow(dead_code)]

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::commands::tunnel::{SavedTunnelSummary, TunnelUsage};
use crate::i18n::t;
use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::views::settings::tunnel_delete_dialog::{TunnelDeleteDialog, TunnelDeleteDialogState};
use crate::views::settings::tunnel_edit_dialog::{TunnelEditDialog, TunnelEditDialogState};
use crate::views::settings::settings_ui::section_title;
use crate::views::settings::tunnel_draft::copy_tunnel_name;

/// `text-xs` — the loading and usage lines.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-[11px]` — the row's meta line.
const TEXT_11: TextStyle = TextStyle::new(11.0, 400.0);
/// `text-sm` — the row's name.
const TEXT_SM: TextStyle = TextStyle::new(14.0, 400.0);
/// `text-[11px]` mono — the kind chip.
const TEXT_11_MONO: TextStyle = TextStyle::new(11.0, 500.0);

/// The section's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TunnelSettingsSectionState {
    /// The store's secret-free summaries.
    pub(crate) summaries: Vec<SavedTunnelSummary>,
    /// The store's loaded face (a finished load, successful or not).
    pub(crate) loaded: bool,
    /// The store's loading face.
    pub(crate) loading: bool,
    /// The store's error sentence.
    pub(crate) store_error: Option<String>,
    /// The usage batch's answers, by tunnel id.
    pub(crate) usage_by_id: HashMap<String, TunnelUsage>,
    /// The usage batch in flight.
    pub(crate) usage_pending: bool,
    /// The action error sentence (the copy's own).
    pub(crate) action_error: Option<String>,
    /// The busy row (the copy in flight; `None` = idle).
    pub(crate) busy_id: Option<String>,
    /// The edit dialog's target (`None` = closed).
    pub(crate) edit_target: Option<Option<String>>,
    /// The delete dialog's target.
    pub(crate) delete_target: Option<SavedTunnelSummary>,
    /// The test dialog's target (T459's slot).
    pub(crate) test_target: Option<SavedTunnelSummary>,
}

impl TunnelSettingsSectionState {
    /// The store's faces (the mount's `load()` reports through it).
    pub(crate) fn note_store(
        &mut self,
        summaries: Vec<SavedTunnelSummary>,
        loaded: bool,
        loading: bool,
        store_error: Option<String>,
    ) {
        self.summaries = summaries;
        self.loaded = loaded;
        self.loading = loading;
        self.store_error = store_error;
    }

    /// The usage batch's arm: an empty summary list clears the map
    /// and the pending face without a read; otherwise the batch
    /// arms. Answers the ids to fetch.
    pub(crate) fn usage_batch_due(&mut self) -> Option<Vec<String>> {
        if self.summaries.is_empty() {
            self.usage_by_id.clear();
            self.usage_pending = false;
            return None;
        }
        self.usage_pending = true;
        Some(self.summaries.iter().map(|s| s.id.clone()).collect())
    }

    /// The batch's answer (the fulfilled map; a rejected probe is
    /// simply absent — `allSettled` keeps one failure from hiding
    /// the others).
    pub(crate) fn note_usage_batch(&mut self, usage_by_id: HashMap<String, TunnelUsage>) {
        self.usage_by_id = usage_by_id;
        self.usage_pending = false;
    }

    /// `handleCopy`'s head: busy arms, the action error clears.
    pub(crate) fn begin_copy(&mut self, id: &str) {
        self.busy_id = Some(id.to_owned());
        self.action_error = None;
    }

    /// The copy's missing entity: the copyFailed error + the reload.
    pub(crate) fn note_copy_missing(&mut self) {
        self.action_error = Some(t("settings.tunnels.copyFailed").to_owned());
        self.busy_id = None;
    }

    /// The copy's success (the reload lands through `note_store`).
    pub(crate) fn note_copy_ok(&mut self) {
        self.busy_id = None;
    }

    /// The copy's failure.
    pub(crate) fn note_copy_error(&mut self, message: &str) {
        self.action_error = Some(if message.is_empty() {
            t("settings.tunnels.copyFailed").to_owned()
        } else {
            message.to_owned()
        });
        self.busy_id = None;
    }

    /// The copy's build: `copyTunnelName(entity.name, t(copySuffix))`
    /// + the minted id.
    pub(crate) fn copy_name(name: &str) -> String {
        copy_tunnel_name(name, t("settings.tunnels.copySuffix"))
    }

    /// `usageText` — the row's usage sentence.
    pub(crate) fn usage_text(&self, id: &str) -> String {
        match self.usage_by_id.get(id) {
            None => t(if self.usage_pending {
                "settings.tunnels.usagePending"
            } else {
                "settings.tunnels.usageNone"
            })
            .to_owned(),
            Some(usage) if usage.connection_names.is_empty() => {
                t("settings.tunnels.usageNone").to_owned()
            }
            Some(usage) => t("settings.tunnels.usageCount")
                .replace("{count}", &usage.connection_names.len().to_string()),
        }
    }

    /// `usageTitle` — the joined names (`None` when there are none).
    pub(crate) fn usage_title(&self, id: &str) -> Option<String> {
        let names = self.usage_by_id.get(id)?.connection_names.as_slice();
        if names.is_empty() {
            None
        } else {
            Some(names.join(", "))
        }
    }

    /// `showEmpty` — a finished load with no tunnels.
    pub(crate) fn show_empty(&self) -> bool {
        self.loaded && self.summaries.is_empty()
    }
}

/// `TunnelSettingsSection()`: the section over the state, the
/// dialog states' lenses and the host's action slots.
pub(crate) struct TunnelSettingsSection<'a, S: 'static> {
    state: &'a TunnelSettingsSectionState,
    edit_state: &'a TunnelEditDialogState,
    delete_state: &'a TunnelDeleteDialogState,
    dark: bool,
    section_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelSettingsSectionState>,
    edit_dialog_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelEditDialogState>,
    delete_dialog_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelDeleteDialogState>,
    /// `load(true)` — the store's reload (the mount, the retry, the
    /// post-action refresh).
    on_reload: Option<Rc<dyn Fn(&mut S)>>,
    /// `handleCopy`'s host half: read the entity, create the clone,
    /// answer through the note methods.
    on_copy: Option<Rc<dyn Fn(&mut S, &SavedTunnelSummary)>>,
    /// `performDelete` — T441's unbind-then-delete over the
    /// connection rail.
    on_delete: Option<Rc<dyn Fn(&mut S, &str)>>,
    /// The delegated TunnelTestDialog (T459's slot).
    test_dialog: Option<El>,
}

impl<'a, S: 'static> TunnelSettingsSection<'a, S> {
    pub(crate) fn new(
        state: &'a TunnelSettingsSectionState,
        edit_state: &'a TunnelEditDialogState,
        delete_state: &'a TunnelDeleteDialogState,
        dark: bool,
        section_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TunnelSettingsSectionState + 'static,
        edit_dialog_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TunnelEditDialogState + 'static,
        delete_dialog_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TunnelDeleteDialogState
            + 'static,
    ) -> Self {
        Self {
            state,
            edit_state,
            delete_state,
            dark,
            section_edit: Rc::new(section_edit),
            edit_dialog_edit: Rc::new(edit_dialog_edit),
            delete_dialog_edit: Rc::new(delete_dialog_edit),
            on_reload: None,
            on_copy: None,
            on_delete: None,
            test_dialog: None,
        }
    }

    /// `load(true)` — the reload slot.
    pub(crate) fn on_reload(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_reload = Some(Rc::new(f));
        self
    }

    /// `handleCopy`'s host half.
    pub(crate) fn on_copy(mut self, f: impl Fn(&mut S, &SavedTunnelSummary) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    /// `performDelete` — the delete dialog's confirm slot.
    pub(crate) fn on_delete(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_delete = Some(Rc::new(f));
        self
    }

    /// The delegated TunnelTestDialog (T459's slot).
    pub(crate) fn test_dialog(mut self, dialog: Option<El>) -> Self {
        self.test_dialog = dialog;
        self
    }

    /// The section.
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let mut section = col()
            .key("settings-tunnels-section")
            .w_full()
            .gap(16.0); // space-y-4

        // The header: the title + the create button.
        let mut head = row().key("tunnels-head").w_full().gap(16.0).justify_between();
        head = head.child(section_title(
            k,
            layer,
            "tunnels-title".to_owned(),
            t("settings.tunnels.title"),
            Some(t("settings.tunnels.description")),
        ));
        let edit_dialog_edit = Rc::clone(&self.edit_dialog_edit);
        let section_edit = Rc::clone(&self.section_edit);
        let mut create = Button::new(t("settings.tunnels.create"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Small)
            .on_press(move |s: &mut S| {
                // `setEditTarget({ id: null })` — the create form.
                section_edit(s).test_target = None;
                let edit = edit_dialog_edit(s);
                edit.open_changed(true, None);
            });
        head = head.child(create.build(k).key("tunnel-create"));
        section = section.child(head);

        // The store error + the retry.
        if let Some(store_error) = &state.store_error {
            let mut row_el = row().key("tunnel-load-error").w_full().gap(12.0).center_y();
            if let Some(banner) = Banner::new("tunnel-load-error-banner", store_error.clone())
                .variant(BannerVariant::Plain)
                .as_element(BannerElement::P)
                .build(k)
            {
                row_el = row_el.child(banner);
            }
            let mut retry = Button::new(t("common.retry"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            if let Some(on_reload) = &self.on_reload {
                let on_reload = Rc::clone(on_reload);
                retry = retry.on_press(move |s: &mut S| on_reload(s));
            }
            row_el = row_el.child(retry.build(k).key("tunnel-retry"));
            section = section.child(row_el);
        }

        // The action error.
        if let Some(action_error) = &state.action_error {
            if let Some(banner) = Banner::new("tunnel-action-error", action_error.clone())
                .variant(BannerVariant::Plain)
                .as_element(BannerElement::P)
                .build(k)
            {
                section = section.child(banner);
            }
        }

        // The loading line (only before the first load lands).
        if state.loading && !state.loaded {
            section = section.child(
                text(t("common.loading").to_owned())
                    .key("tunnel-loading")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        }

        // The empty state.
        if state.show_empty() {
            let mut empty = col()
                .key("tunnel-list-empty")
                .w_full()
                .gap(8.0)
                .padding(16.0)
                .radius(6.0)
                .border(1.0, k.c.border_muted);
            empty = empty.child(
                text(t("settings.tunnels.empty").to_owned())
                    .font(TEXT_SM)
                    .color(k.c.text_base),
            );
            empty = empty.child(
                text(t("settings.tunnels.emptyHint").to_owned())
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
            section = section.child(empty);
        }

        // The rows.
        if !state.summaries.is_empty() {
            let mut list = col().key("tunnel-list").w_full().gap(8.0);
            for summary in &state.summaries {
                let mut row_el = row()
                    .key(format!("tunnel-row-{}", summary.id))
                    .w_full()
                    .gap(12.0)
                    .center_y()
                    .padding(8.0)
                    .radius(6.0)
                    .border(1.0, k.c.border_base)
                    .fill(k.c.surface_weak);

                let mut lead = col().key(format!("tunnel-row-{}-lead", summary.id)).gap(2.0).grow(1.0).min_w(0.0);
                lead = lead.child(
                    text(summary.name.clone())
                        .key(format!("tunnel-row-{}-name", summary.id))
                        .font(TEXT_SM)
                        .color(k.c.text_base),
                );
                let mut meta = row()
                    .key(format!("tunnel-row-{}-meta", summary.id))
                    .gap(8.0)
                    .center_y();
                meta = meta.child(
                    row()
                        .key(format!("tunnel-row-{}-kind", summary.id))
                        .padding(2.0)
                        .radius(4.0)
                        .fill(k.alpha(k.c.text_accent, 0.2))
                        .child(
                            text(t(summary_kind_label_key(summary)).to_owned())
                                .font(TEXT_11_MONO)
                                .color(k.c.text_accent),
                        ),
                );
                meta = meta.child(
                    text(state.usage_text(&summary.id))
                        .key(format!("tunnel-row-{}-usage", summary.id))
                        .font(TEXT_11)
                        .color(k.c.text_muted),
                );
                lead = lead.child(meta);
                row_el = row_el.child(lead);

                // The action pair: edit / copy / test / delete.
                let mut actions = row().gap(4.0).center_y().shrink();
                let edit_dialog_edit = Rc::clone(&self.edit_dialog_edit);
                let section_edit = Rc::clone(&self.section_edit);
                let id = summary.id.clone();
                let mut edit = Button::new(t("settings.tunnels.edit"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| {
                        // `setEditTarget({ id: summary.id })`.
                        section_edit(s).test_target = None;
                        let edit = edit_dialog_edit(s);
                        edit.open_changed(true, Some(&id));
                    });
                actions = actions
                    .child(edit.build(k).key(format!("tunnel-edit-{}", summary.id)));

                let busy = state.busy_id.as_deref() == Some(summary.id.as_str());
                let section_edit = Rc::clone(&self.section_edit);
                let on_copy = self.on_copy.clone();
                let summary_for_copy = summary.clone();
                let mut copy = Button::new(t("settings.tunnels.copy"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .disabled(busy)
                    .on_press(move |s: &mut S| {
                        section_edit(s).begin_copy(&summary_for_copy.id);
                        if let Some(on_copy) = &on_copy {
                            on_copy(s, &summary_for_copy);
                        }
                    });
                actions = actions
                    .child(copy.build(k).key(format!("tunnel-copy-{}", summary.id)));

                let section_edit = Rc::clone(&self.section_edit);
                let summary_for_test = summary.clone();
                let mut test = Button::new(t("settings.tunnels.test"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| {
                        // `setTestTarget(summary)` — T459's slot opens.
                        section_edit(s).test_target = Some(summary_for_test.clone());
                    });
                actions = actions
                    .child(test.build(k).key(format!("tunnel-test-{}", summary.id)));

                let section_edit = Rc::clone(&self.section_edit);
                let delete_dialog_edit = Rc::clone(&self.delete_dialog_edit);
                let summary_for_delete = summary.clone();
                let mut delete = Button::new(t("settings.tunnels.delete"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| {
                        // `setDeleteTarget(summary)`.
                        let delete = delete_dialog_edit(s);
                        let should_read = delete.open_changed(true, Some(&summary_for_delete));
                        if should_read {
                            // The usage read the dialog arms (the host
                            // runs it; the audit observes the arm).
                            delete.note_usage(TunnelUsage {
                                connection_ids: Vec::new(),
                                connection_names: Vec::new(),
                            });
                        }
                    });
                actions = actions
                    .child(delete.build(k).key(format!("tunnel-delete-{}", summary.id)));

                row_el = row_el.child(actions);
                list = list.child(row_el);
            }
            section = section.child(list);
        }

        // The delegated TunnelTestDialog (T459's slot).
        if let Some(dialog) = &self.test_dialog {
            section = section.child(dialog.clone());
        } else if state.test_target.is_some() {
            section = section.child(col().key("tunnel-test-dialog-boundary"));
        }

        section
    }
}

/// The kind chip's label key (`settings.tunnels.kind.{kind}`).
fn summary_kind_label_key(summary: &SavedTunnelSummary) -> &'static str {
    match summary.kind {
        datazen_ui_api::SavedTunnelKind::Ssh => "settings.tunnels.kind.ssh",
        datazen_ui_api::SavedTunnelKind::HttpProxy => "settings.tunnels.kind.httpProxy",
        datazen_ui_api::SavedTunnelKind::WebSocket => "settings.tunnels.kind.websocket",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ui_api::SavedTunnelKind;

    fn summary(id: &str, name: &str) -> SavedTunnelSummary {
        SavedTunnelSummary {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: SavedTunnelKind::Ssh,
        }
    }

    fn usage(names: &[&str]) -> TunnelUsage {
        TunnelUsage {
            connection_ids: names.iter().map(|n| format!("c-{n}")).collect(),
            connection_names: names.iter().map(|n| n.to_string()).collect(),
        }
    }

    #[test]
    fn the_usage_batch_clears_on_an_empty_list() {
        let mut state = TunnelSettingsSectionState::default();
        state.usage_by_id.insert("t-1".into(), usage(&["A"]));
        state.usage_pending = true;
        assert!(state.usage_batch_due().is_none(), "no read");
        assert!(state.usage_by_id.is_empty(), "the map clears");
        assert!(!state.usage_pending);
        // A populated list arms the batch with every id.
        state.note_store(vec![summary("t-1", "A"), summary("t-2", "B")], true, false, None);
        assert_eq!(
            state.usage_batch_due(),
            Some(vec!["t-1".to_owned(), "t-2".to_owned()])
        );
        assert!(state.usage_pending);
        // The batch answers; a rejected probe is simply absent.
        let mut batch = HashMap::new();
        batch.insert("t-1".to_owned(), usage(&["Alpha", "Beta"]));
        state.note_usage_batch(batch);
        assert!(!state.usage_pending);
        assert!(state.usage_by_id.contains_key("t-1"));
        assert!(!state.usage_by_id.contains_key("t-2"), "the failed probe is absent");
    }

    #[test]
    fn the_usage_text_moves_through_its_three_faces() {
        let mut state = TunnelSettingsSectionState::default();
        state.note_store(vec![summary("t-1", "A")], true, false, None);
        // No answer yet: pending or none by the pending flag.
        state.usage_pending = true;
        assert_eq!(state.usage_text("t-1"), t("settings.tunnels.usagePending"));
        state.usage_pending = false;
        assert_eq!(state.usage_text("t-1"), t("settings.tunnels.usageNone"));
        // An empty answer is the none sentence.
        state.usage_by_id.insert("t-1".into(), usage(&[]));
        assert_eq!(state.usage_text("t-1"), t("settings.tunnels.usageNone"));
        assert_eq!(state.usage_title("t-1"), None);
        // A populated answer formats the count.
        state.usage_by_id.insert("t-1".into(), usage(&["Alpha", "Beta"]));
        assert_eq!(
            state.usage_text("t-1"),
            t("settings.tunnels.usageCount").replace("{count}", "2")
        );
        assert_eq!(state.usage_title("t-1"), Some("Alpha, Beta".to_owned()));
    }

    #[test]
    fn the_copy_flow_and_its_missing_entity() {
        let mut state = TunnelSettingsSectionState::default();
        state.begin_copy("t-1");
        assert_eq!(state.busy_id.as_deref(), Some("t-1"));
        assert!(state.action_error.is_none());
        // A missing entity: the copyFailed error + busy clears.
        state.note_copy_missing();
        assert_eq!(state.action_error.as_deref(), Some(t("settings.tunnels.copyFailed")));
        assert_eq!(state.busy_id, None);
        // A failure with a message keeps it; an empty one falls back.
        state.begin_copy("t-1");
        state.note_copy_error("ipc exploded");
        assert_eq!(state.action_error.as_deref(), Some("ipc exploded"));
        state.begin_copy("t-1");
        state.note_copy_error("");
        assert_eq!(state.action_error.as_deref(), Some(t("settings.tunnels.copyFailed")));
        // A success clears busy.
        state.begin_copy("t-1");
        state.note_copy_ok();
        assert_eq!(state.busy_id, None);
        // The name carries the locale suffix.
        assert_eq!(
            TunnelSettingsSectionState::copy_name("Bastion"),
            format!("Bastion{}", t("settings.tunnels.copySuffix"))
        );
    }

    #[test]
    fn the_faces_answer_their_predicates() {
        let mut state = TunnelSettingsSectionState::default();
        assert!(!state.show_empty(), "not loaded yet");
        state.note_store(Vec::new(), true, false, None);
        assert!(state.show_empty(), "loaded with no tunnels");
        state.note_store(vec![summary("t-1", "A")], true, false, None);
        assert!(!state.show_empty());
        // The loading line only before the first load.
        state.note_store(Vec::new(), false, true, None);
        assert!(!state.show_empty());
        // The store error rides its own face.
        state.note_store(Vec::new(), true, false, Some("cannot read".to_owned()));
        assert_eq!(state.store_error.as_deref(), Some("cannot read"));
    }
}
