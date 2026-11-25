//! Port of `packages/frontend/src/windows/settings/MemorySection.tsx`
//! (T360, split) — Settings → AI → 记忆.
//!
//! The AI memory administration surface: the settings card (the opt-in
//! enable with its first-use privacy acknowledgement card, auto-learn
//! with its token-cost line, the cloud policy select), the read-only
//! organization list, and the personal entries list — the search box,
//! the connection/kind/status filters, the candidates bar (confirm-all,
//! show-candidates when the page has none but the stats do,
//! merge-selected), the rows with their stale/conflict badges and
//! candidate confirm/reject, the inline edit form, the select-all bar
//! with delete-selected, the export pair, and the privacy footer.
//!
//! ## What maps where
//!
//! - `useAiStore.saveSettingsConfig` is the host seam: the persist is
//!   optimistic with the revert-on-failure through
//!   [`MemorySectionState::note_persist_result`].
//! - The debounced load effect (`setTimeout(200)` + `refreshToken`)
//!   is [`MemorySectionState::load_deadline`] + `load_plan` (the host
//!   wakes on the deadline, runs the plan, answers through
//!   `begin_load`/`apply_loaded`/`deliver_stats`/`deliver_org`); the
//!   search face's client-side connection/kind filters (T12's IPC
//!   predates them) ride [`MemorySectionState::apply_loaded`].
//! - `deletedIds` (the ref) is the state's `deleted_ids` tombstone set
//!   — a deleted row never flashes back on a racing refresh.
//! - `toggleEnabled`'s first-use gate, `confirmEnable`, and
//!   `setCloudPolicy` are the state's plan methods; the export notice
//!   arms at [`MemorySectionState::begin_export`] (the file download
//!   itself is the host's — React's `downloadText` is browser DOM).
//! - `SectionTitle` / `ToggleRow` are the `settingsUi.tsx` port; the
//!   selects are the kit [`Select`]; the checkboxes are the kit
//!   [`Checkbox`]; the icon buttons ride the icons registry. React
//!   `Button` variants map per T022 (`secondary` → `Outline`,
//!   `primary` → `Submit`, `ghost` → `Ghost`).
//! - `normalizeMemorySettings` (lib/aiMemorySettings): the typed
//!   struct makes it the identity; the host normalizes JSON-level
//!   partials before delivery (the state module's header note).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `ai-memory-settings`,
//! `ai-memory-enabled`, `ai-memory-auto-learn`,
//! `ai-memory-auto-learn-cost`, `ai-memory-cloud-policy`,
//! `ai-memory-first-use` (confirm `-confirm`, cancel `-cancel`),
//! `ai-memory-disabled-note`, `ai-org-memory-section`,
//! `ai-org-memory-note`, `ai-org-memory-empty`, `ai-org-memory-list`,
//! `ai-org-memory-row-{id}`, `ai-memory-export-json`,
//! `ai-memory-export-markdown`, `ai-memory-export-notice`,
//! `ai-memory-search`, `ai-memory-filter-connection`,
//! `ai-memory-filter-kind`, `ai-memory-filter-status`,
//! `ai-memory-refresh`, `ai-memory-candidates-title`,
//! `ai-memory-candidate-count`, `ai-memory-confirm-all`,
//! `ai-memory-show-candidates`, `ai-memory-merge-selected`,
//! `ai-memory-notice`, `ai-memory-empty`, `ai-memory-list`,
//! `ai-memory-select-all`, `ai-memory-delete-selected`,
//! `ai-memory-row-{id}`, `ai-memory-select-{id}`,
//! `ai-memory-edit-form-{id}`, `ai-memory-edit-subject`,
//! `ai-memory-edit-content`, `ai-memory-edit-save`,
//! `ai-memory-edit-cancel`, `ai-memory-stale-{id}`,
//! `ai-memory-conflict-{id}`, `ai-memory-review-stale-{id}`,
//! `ai-memory-confirm-{id}`, `ai-memory-reject-{id}`,
//! `ai-memory-edit-{id}`, `ai-memory-delete-{id}`.
//!
//! Known deltas: the edit form's content `<textarea>` renders as the
//! kit's single-line [`TextField`] (the value keeps the multi-line
//! form); the search box and the edit fields are `TextField`s over
//! the state's `TextEdit`s (the host wires the lenses); the `Select`
//! listboxes' open state is the host's (the `open_select` slot).

// No production consumer yet (the settings view wiring track mounts
// the section); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use datazen_ui_api::{AiMemoryEntry, AiMemorySettings};
use rune::{col, icon, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, State};

use super::memory_section_state::{
    MemoryExportFormat, MemorySectionState, MemorySelect, MEMORY_CLOUD_POLICIES,
    MEMORY_EXTRACTION_TOKEN_ESTIMATE, MEMORY_KINDS, MEMORY_STATUSES,
};
use crate::hooks::use_i18n::UseI18n;
use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_DOWNLOAD, ICON_PENCIL, ICON_REFRESH, ICON_TRASH, ICON_X};
use crate::ui::badge::{Badge, BadgeTone};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::{Checkbox, Switch};
use crate::views::settings::settings_ui::{section_title, toggle_row, ToggleRow as ToggleRowSpec};

/// React `text-xs` / `text-[11px]` / `text-[9px]` / `text-[10px]`.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
const TEXT_11: TextStyle = TextStyle::new(11.0, 400.0);
const TEXT_9: TextStyle = TextStyle::new(9.0, 400.0);
const TEXT_10: TextStyle = TextStyle::new(10.0, 400.0);
const TEXT_XS_MEDIUM: TextStyle = TextStyle::new(12.0, 500.0);

/// `MemorySection({ config })`: the section over the state, the host's
/// connection list, and the action slots.
pub struct MemorySection<'a, S: 'static> {
    state: &'a MemorySectionState,
    dark: bool,
    /// `now` — the host clock (the toggles' notices read it).
    now: f64,
    /// The connection store's list (`id`, `name`) — the filter's known
    /// connections.
    connections: &'a [(String, String)],
    // the text inputs' lenses
    search_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    edit_subject_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    edit_content_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    // host actions
    on_toggle_enabled: Option<Rc<dyn Fn(&mut S, bool)>>,
    on_confirm_enable: Option<Rc<dyn Fn(&mut S)>>,
    on_cancel_enable: Option<Rc<dyn Fn(&mut S)>>,
    on_set_auto_learn: Option<Rc<dyn Fn(&mut S, bool)>>,
    on_set_cloud_policy: Option<Rc<dyn Fn(&mut S, &'static str)>>,
    on_search_commit: Option<Rc<dyn Fn(&mut S)>>,
    on_set_connection_filter: Option<Rc<dyn Fn(&mut S, String)>>,
    on_set_kind_filter: Option<Rc<dyn Fn(&mut S, String)>>,
    on_set_status_filter: Option<Rc<dyn Fn(&mut S, String)>>,
    on_refresh: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_selected: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_all: Option<Rc<dyn Fn(&mut S)>>,
    on_update_row: Option<Rc<dyn Fn(&mut S, String)>>,
    on_confirm_all: Option<Rc<dyn Fn(&mut S)>>,
    on_delete_one: Option<Rc<dyn Fn(&mut S, String)>>,
    on_delete_selected: Option<Rc<dyn Fn(&mut S)>>,
    on_merge_selected: Option<Rc<dyn Fn(&mut S)>>,
    on_confirm_entry: Option<Rc<dyn Fn(&mut S, String)>>,
    on_reject_entry: Option<Rc<dyn Fn(&mut S, String)>>,
    on_mark_reviewed: Option<Rc<dyn Fn(&mut S, String)>>,
    on_begin_edit: Option<Rc<dyn Fn(&mut S, String)>>,
    on_cancel_edit: Option<Rc<dyn Fn(&mut S)>>,
    on_export: Option<Rc<dyn Fn(&mut S, MemoryExportFormat)>>,
    /// A select's trigger press: open it (closing any other). The
    /// default no-op satisfies the kit's guard; hosts that want the
    /// listboxes wire the slot to the section state's
    /// `toggle_select`.
    on_select_toggle: Option<Rc<dyn Fn(&mut S, MemorySelect)>>,
    /// Close the open listbox (outside press / Escape).
    on_select_close: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> MemorySection<'a, S> {
    /// The section over the state and the three text lenses.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state: &'a MemorySectionState,
        dark: bool,
        now: f64,
        connections: &'a [(String, String)],
        search_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        edit_subject_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        edit_content_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            now,
            connections,
            search_edit: Rc::new(search_edit),
            edit_subject_edit: Rc::new(edit_subject_edit),
            edit_content_edit: Rc::new(edit_content_edit),
            on_toggle_enabled: None,
            on_confirm_enable: None,
            on_cancel_enable: None,
            on_set_auto_learn: None,
            on_set_cloud_policy: None,
            on_search_commit: None,
            on_set_connection_filter: None,
            on_set_kind_filter: None,
            on_set_status_filter: None,
            on_refresh: None,
            on_toggle_selected: None,
            on_toggle_all: None,
            on_update_row: None,
            on_confirm_all: None,
            on_delete_one: None,
            on_delete_selected: None,
            on_merge_selected: None,
            on_confirm_entry: None,
            on_reject_entry: None,
            on_mark_reviewed: None,
            on_begin_edit: None,
            on_cancel_edit: None,
            on_export: None,
            on_select_toggle: None,
            on_select_close: None,
        }
    }

    /// `toggleEnabled(next)` — the host runs the first-use gate then
    /// persists when it answers true.
    pub fn on_toggle_enabled(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_toggle_enabled = Some(Rc::new(f));
        self
    }
    /// `confirmEnable`.
    pub fn on_confirm_enable(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_enable = Some(Rc::new(f));
        self
    }
    /// The first-use card's cancel.
    pub fn on_cancel_enable(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel_enable = Some(Rc::new(f));
        self
    }
    /// The auto-learn toggle's write.
    pub fn on_set_auto_learn(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_set_auto_learn = Some(Rc::new(f));
        self
    }
    /// `setCloudPolicy`.
    pub fn on_set_cloud_policy(
        mut self,
        f: impl Fn(&mut S, &'static str) + 'static,
    ) -> Self {
        self.on_set_cloud_policy = Some(Rc::new(f));
        self
    }
    /// The search box's debounced commit (the host notes the filter
    /// change).
    pub fn on_search_commit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_search_commit = Some(Rc::new(f));
        self
    }
    pub fn on_set_connection_filter(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_set_connection_filter = Some(Rc::new(f));
        self
    }
    pub fn on_set_kind_filter(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_set_kind_filter = Some(Rc::new(f));
        self
    }
    pub fn on_set_status_filter(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_set_status_filter = Some(Rc::new(f));
        self
    }
    /// `refresh`.
    pub fn on_refresh(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_refresh = Some(Rc::new(f));
        self
    }
    pub fn on_toggle_selected(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_selected = Some(Rc::new(f));
        self
    }
    /// `toggleAll`.
    pub fn on_toggle_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_all = Some(Rc::new(f));
        self
    }
    /// The edit form's save (`updateRow`).
    pub fn on_update_row(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_update_row = Some(Rc::new(f));
        self
    }
    /// `confirmAll`.
    pub fn on_confirm_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_all = Some(Rc::new(f));
        self
    }
    pub fn on_delete_one(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_delete_one = Some(Rc::new(f));
        self
    }
    /// `deleteSelected`.
    pub fn on_delete_selected(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_delete_selected = Some(Rc::new(f));
        self
    }
    /// `mergeSelected`.
    pub fn on_merge_selected(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_merge_selected = Some(Rc::new(f));
        self
    }
    /// One candidate's check button.
    pub fn on_confirm_entry(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_confirm_entry = Some(Rc::new(f));
        self
    }
    /// One candidate's X button.
    pub fn on_reject_entry(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_reject_entry = Some(Rc::new(f));
        self
    }
    /// A stale row's mark-reviewed.
    pub fn on_mark_reviewed(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_mark_reviewed = Some(Rc::new(f));
        self
    }
    /// The pencil press.
    pub fn on_begin_edit(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_begin_edit = Some(Rc::new(f));
        self
    }
    /// The edit form's cancel.
    pub fn on_cancel_edit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel_edit = Some(Rc::new(f));
        self
    }
    /// The export pair.
    pub fn on_export(
        mut self,
        f: impl Fn(&mut S, MemoryExportFormat) + 'static,
    ) -> Self {
        self.on_export = Some(Rc::new(f));
        self
    }
    /// A select's trigger press (the one-open-listbox rule).
    pub fn on_select_toggle(
        mut self,
        f: impl Fn(&mut S, MemorySelect) + 'static,
    ) -> Self {
        self.on_select_toggle = Some(Rc::new(f));
        self
    }
    /// Close the open listbox.
    pub fn on_select_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_close = Some(Rc::new(f));
        self
    }

    /// The section body.
    pub fn build(self, layer: &mut Vec<El>) -> El {
        let dark = self.dark;
        let k = kit(dark);

        let mut body = col().key("ai-memory-settings").w_full().gap(20.0);

        // The heading + the privacy summary.
        let mut head = col().key("ai-memory-heading").gap(4.0);
        head = head.child(section_title(
            &k,
            layer,
            "ai-memory-title".to_owned(),
            t("settings.ai.memory.title"),
            Some(t("settings.ai.memory.description")),
        ));
        head = head.child(
            text(t("settings.ai.memory.privacySummary")).font(TEXT_XS).color(k.c.text_muted),
        );
        body = body.child(head);

        // The settings card.
        body = body.child(self.settings_card(&k, layer));

        // The org list.
        body = body.child(self.org_block(&k));

        // The entries list.
        body = body.child(self.list_block(&k, layer, dark));

        // The privacy footer.
        body = body.child(
            col()
                .key("ai-memory-privacy")
                .padding(12.0)
                .gap(4.0)
                .radius(k.radius.md)
                .border(1.0, k.c.border_muted)
                .fill(k.c.layer_01.alpha(0.4))
                .child(text(t("settings.ai.memory.privacyTitle")).font(TextStyle { weight: 500.0, ..TEXT_11 }).color(k.c.text_muted))
                .child(text(t("settings.ai.memory.privacyBody")).font(TEXT_11).color(k.c.text_muted)),
        );

        body
    }

    /// The settings card: the two toggles, the cost line, the policy
    /// select, the first-use card, the disabled note.
    fn settings_card(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;

        let mut card = col()
            .key("ai-memory-settings-card")
            .w_full()
            .padding(12.0)
            .gap(12.0)
            .radius(k.radius.xl)
            .border(1.0, k.c.border_base.alpha(0.8))
            .fill(k.c.surface_base);

        let no_write: Rc<dyn Fn(&mut S, bool)> = Rc::new(|_s: &mut S, _v: bool| {});
        // The enable toggle (the first-use gate rides the host slot).
        card = card.child(toggle_row::<S>(
            k,
            layer,
            ToggleRowSpec {
                key: "ai-memory-enabled".to_owned(),
                label: t("settings.ai.memory.enable"),
                hint: Some(t("settings.ai.memory.enableHint")),
                checked: state.memory.enabled,
                disabled: false,
            },
            self.on_toggle_enabled.clone().unwrap_or_else(|| Rc::clone(&no_write)),
        ));

        // Auto-learn.
        card = card.child(toggle_row::<S>(
            k,
            layer,
            ToggleRowSpec {
                key: "ai-memory-auto-learn".to_owned(),
                label: t("settings.ai.memory.autoLearn"),
                hint: Some(t("settings.ai.memory.autoLearnHint")),
                checked: state.memory.auto_learn,
                disabled: false,
            },
            self.on_set_auto_learn.clone().unwrap_or_else(|| no_write),
        ));

        // The cost line, only while auto-learn is on.
        if state.memory.auto_learn {
            let i18n = UseI18n::new();
            card = card.child(
                text(i18n.t_with_params(
                    "settings.ai.memory.autoLearnCost",
                    &[("tokens", &MEMORY_EXTRACTION_TOKEN_ESTIMATE.to_string())],
                ))
                .key("ai-memory-auto-learn-cost")
                .font(TEXT_11)
                .color(k.c.text_muted),
            );
        }

        // The cloud policy row.
        let policy_labels: Vec<String> = MEMORY_CLOUD_POLICIES
            .iter()
            .map(|policy| t(cloud_policy_key(*policy)).to_owned())
            .collect();
        let policy_values: Vec<&str> = MEMORY_CLOUD_POLICIES.iter().map(|p| p.as_str()).collect();
        let current_label = t(cloud_policy_key(state.memory.cloud_policy)).to_owned();
        let mut select = Select::new("ai-memory-cloud-policy", &policy_labels)
            .values(&policy_values)
            .current(Some(&current_label))
            .label(t("settings.ai.memory.cloudPolicy"))
            .width(144.0)
            .open(state.open_select == Some(MemorySelect::CloudPolicy))
            .disabled(!state.memory.enabled);
        select = wire_select_toggle(select, MemorySelect::CloudPolicy, &self.on_select_toggle, &self.on_select_close);
        if let Some(f) = &self.on_set_cloud_policy {
            let f = Rc::clone(f);
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                // The slot hands the host the POLICY VALUE (the wire
                // spelling), matching the closed list first.
                if let Some(policy) = MEMORY_CLOUD_POLICIES.iter().find(|p| p.as_str() == value) {
                    f(s, policy.as_str());
                }
            });
        }
        let policy_row = row().key("ai-memory-policy-row").w_full().gap(16.0).center_y().justify_between()
            .child(
                col().key("ai-memory-policy-meta").gap(2.0)
                    .child(text(t("settings.ai.memory.cloudPolicy")).font(TEXT_XS).color(k.c.text_muted))
                    .child(text(t("settings.ai.memory.cloudPolicyHint")).font(TEXT_11).color(k.c.text_muted)),
            )
            .child(select.build(k, layer));
        card = card.child(policy_row);

        // The first-use card.
        if state.pending_enable {
            let mut first_use = col()
                .key("ai-memory-first-use")
                .padding(12.0)
                .gap(8.0)
                .radius(k.radius.md)
                .border(1.0, k.c.border_focus.alpha(0.3))
                .fill(k.alpha(k.c.border_focus, 0.05));
            first_use = first_use.child(
                text(t("settings.ai.memory.firstUseTitle")).font(TextStyle { weight: 500.0, ..TEXT_XS }).color(k.c.text_base),
            );
            first_use = first_use.child(
                text(t("settings.ai.memory.firstUseBody")).font(TEXT_XS).color(k.c.text_muted),
            );
            let mut confirm = Button::new(t("settings.ai.memory.firstUseConfirm"))
                .variant(ButtonVariant::Submit) // React primary
                .control_size(ControlSize::Small);
            if let Some(f) = &self.on_confirm_enable {
                let f = Rc::clone(f);
                confirm = confirm.on_press(move |s: &mut S| f(s));
            }
            let mut cancel = Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            if let Some(f) = &self.on_cancel_enable {
                let f = Rc::clone(f);
                cancel = cancel.on_press(move |s: &mut S| f(s));
            }
            first_use = first_use.child(
                row().key("ai-memory-first-use-buttons").gap(8.0)
                    .child(confirm.build(k).key("ai-memory-first-use-confirm"))
                    .child(cancel.build(k).key("ai-memory-first-use-cancel")),
            );
            card = card.child(first_use);
        }

        if !state.memory.enabled {
            card = card.child(
                text(t("settings.ai.memory.disabledNote"))
                    .key("ai-memory-disabled-note")
                    .font(TEXT_11)
                    .color(k.c.text_muted),
            );
        }

        card
    }

    /// The read-only organization list.
    fn org_block(&self, k: &Kit) -> El {
        let state = self.state;

        let mut block = col().key("ai-org-memory-section").w_full().gap(8.0);
        block = block.child(section_title(
            k,
            &mut Vec::new(),
            "ai-org-memory-title".to_owned(),
            t("settings.ai.memory.orgTitle"),
            None,
        ));
        block = block.child(
            text(t("settings.ai.memory.orgReadOnly"))
                .key("ai-org-memory-note")
                .font(TEXT_11)
                .color(k.c.text_muted),
        );
        if state.organization_entries.is_empty() {
            block = block.child(
                text(t("settings.ai.memory.orgEmpty"))
                    .key("ai-org-memory-empty")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        } else {
            let mut list = col().key("ai-org-memory-list").w_full().gap(6.0);
            for entry in &state.organization_entries {
                list = list.child(org_row(k, entry));
            }
            block = block.child(list);
        }
        block
    }

    /// The entries list: exports, filters, the candidates bar, the
    /// rows.
    fn list_block(&self, k: &Kit, layer: &mut Vec<El>, dark: bool) -> El {
        let state = self.state;

        let mut block = col().key("ai-memory-list-block").w_full().gap(8.0);

        // The heading + the export pair.
        let mut head = row().key("ai-memory-list-head").w_full().gap(8.0).center_y().justify_between();
        head = head.child(section_title(
            k,
            &mut Vec::new(),
            "ai-memory-list-title".to_owned(),
            t("settings.ai.memory.listTitle"),
            None,
        ));
        let mut export_json = Button::new("JSON")
            .icon(&ICON_DOWNLOAD)
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Small);
        if let Some(f) = &self.on_export {
            let f = Rc::clone(f);
            export_json = export_json.on_press(move |s: &mut S| f(s, MemoryExportFormat::Json));
        }
        let mut export_md = Button::new("Markdown")
            .icon(&ICON_DOWNLOAD)
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Small);
        if let Some(f) = &self.on_export {
            let f = Rc::clone(f);
            export_md = export_md.on_press(move |s: &mut S| f(s, MemoryExportFormat::Markdown));
        }
        head = head.child(
            row().key("ai-memory-exports").gap(4.0)
                .child(export_json.build(k).key("ai-memory-export-json"))
                .child(export_md.build(k).key("ai-memory-export-markdown")),
        );
        block = block.child(head);

        block = block.child(
            text(t("settings.ai.memory.exportWarning"))
                .key("ai-memory-export-notice")
                .font(TEXT_11)
                .color(k.c.text_muted),
        );

        // The filter row: search + the three selects + refresh.
        let search_lens = Rc::clone(&self.search_edit);
        let search_field = TextField::new("ai-memory-search", &state.search)
            .placeholder(t("settings.ai.memory.searchPlaceholder"))
            .grow()
            .min_width(180.0)
            .on_edit::<S>(move |s: &mut S| search_lens(s));
        let mut filters = row().key("ai-memory-filters").w_full().gap(8.0).center_y()
            .child(search_field.build(dark));

        // Connection select: the all option + the union of known
        // connections and the entries' unknown ids.
        let options = state.connection_options(self.connections);
        let mut conn_labels = vec![t("settings.ai.memory.allConnections").to_owned()];
        let mut conn_values = vec![String::new()];
        for (id, name) in &options {
            conn_labels.push(name.clone());
            conn_values.push(id.clone());
        }
        let conn_current = options
            .iter()
            .find(|(id, _)| *id == state.connection_filter)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| t("settings.ai.memory.allConnections").to_owned());
        let mut conn_select = Select::new("ai-memory-filter-connection", &conn_labels)
            .values(&conn_values)
            .current(Some(&conn_current))
            .label(t("settings.ai.memory.allConnections"))
            .width(144.0)
            .open(state.open_select == Some(MemorySelect::Connection));
        conn_select = wire_select_toggle(conn_select, MemorySelect::Connection, &self.on_select_toggle, &self.on_select_close);
        if let Some(f) = &self.on_set_connection_filter {
            let f = Rc::clone(f);
            conn_select = conn_select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, value.to_owned());
            });
        }
        filters = filters.child(conn_select.build(k, layer));

        // Kind select.
        let mut kind_labels = vec![t("settings.ai.memory.allKinds").to_owned()];
        let mut kind_values = vec![String::new()];
        for kind in MEMORY_KINDS {
            kind_labels.push(t(kind_key(kind)).to_owned());
            kind_values.push(kind.as_str().to_owned());
        }
        let kind_current = if state.kind_filter.is_empty() {
            t("settings.ai.memory.allKinds").to_owned()
        } else {
            MEMORY_KINDS
                .iter()
                .find(|k| k.as_str() == state.kind_filter)
                .map(|k| t(kind_key(*k)).to_owned())
                .unwrap_or_default()
        };
        let mut kind_select = Select::new("ai-memory-filter-kind", &kind_labels)
            .values(&kind_values)
            .current(Some(&kind_current))
            .label(t("settings.ai.memory.allKinds"))
            .width(128.0)
            .open(state.open_select == Some(MemorySelect::Kind));
        kind_select = wire_select_toggle(kind_select, MemorySelect::Kind, &self.on_select_toggle, &self.on_select_close);
        if let Some(f) = &self.on_set_kind_filter {
            let f = Rc::clone(f);
            kind_select = kind_select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, value.to_owned());
            });
        }
        filters = filters.child(kind_select.build(k, layer));

        // Status select.
        let mut status_labels = vec![t("settings.ai.memory.allStatuses").to_owned()];
        let mut status_values = vec![String::new()];
        for status in MEMORY_STATUSES {
            status_labels.push(t(status_key(status)).to_owned());
            status_values.push(status.as_str().to_owned());
        }
        let status_current = if state.status_filter.is_empty() {
            t("settings.ai.memory.allStatuses").to_owned()
        } else {
            MEMORY_STATUSES
                .iter()
                .find(|s| s.as_str() == state.status_filter)
                .map(|s| t(status_key(*s)).to_owned())
                .unwrap_or_default()
        };
        let mut status_select = Select::new("ai-memory-filter-status", &status_labels)
            .values(&status_values)
            .current(Some(&status_current))
            .label(t("settings.ai.memory.allStatuses"))
            .width(128.0)
            .open(state.open_select == Some(MemorySelect::Status));
        status_select = wire_select_toggle(status_select, MemorySelect::Status, &self.on_select_toggle, &self.on_select_close);
        if let Some(f) = &self.on_set_status_filter {
            let f = Rc::clone(f);
            status_select = status_select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                f(s, value.to_owned());
            });
        }
        filters = filters.child(status_select.build(k, layer));

        let mut refresh = Button::new("")
            .icon(&ICON_REFRESH)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .help(t("settings.ai.memory.refresh"));
        if let Some(f) = &self.on_refresh {
            let f = Rc::clone(f);
            refresh = refresh.on_press(move |s: &mut S| f(s));
        }
        filters = filters.child(refresh.build(k).key("ai-memory-refresh"));
        block = block.child(filters);

        // The candidates bar.
        block = block.child(self.candidates_bar(k));

        // The error / notice / loading lines.
        if let Some(message) = &state.error {
            block = block.child(
                text(message.clone()).font(TEXT_XS).color(k.c.danger.fg),
            );
        }
        if let Some(notice) = &state.notice {
            block = block.child(
                text(t(notice))
                    .key("ai-memory-notice")
                    .font(TEXT_XS)
                    .color(k.c.text_accent),
            );
        }
        if state.loading {
            block = block.child(text(t("common.loading")).font(TEXT_XS).color(k.c.text_muted));
        }

        // The empty state / the list.
        if state.entries.is_empty() && !state.loading {
            block = block.child(
                text(t("settings.ai.memory.empty"))
                    .key("ai-memory-empty")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        } else if !state.entries.is_empty() {
            let mut list = col().key("ai-memory-list").w_full().gap(6.0);

            // The select-all bar.
            let mut select_all = Checkbox::new("ai-memory-select-all", state.all_selected())
                .label(t("settings.ai.memory.selectAll"));
            if let Some(f) = &self.on_toggle_all {
                let f = Rc::clone(f);
                select_all = select_all.on_toggle(move |s: &mut S| f(s));
            }
            let mut delete_selected = Button::new(delete_selected_label(state))
                .icon(&ICON_TRASH)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            if state.selected.is_empty() {
                delete_selected = delete_selected.state(State::Disabled).disabled(true);
            } else if let Some(f) = &self.on_delete_selected {
                let f = Rc::clone(f);
                delete_selected = delete_selected.on_press(move |s: &mut S| f(s));
            }
            list = list.child(
                row().key("ai-memory-select-all-row").gap(8.0).center_y().px(4.0)
                    .child(select_all.build(k))
                    .child(delete_selected.build(k).key("ai-memory-delete-selected")),
            );

            for entry in &state.entries {
                list = list.child(self.entry_row(k, layer, dark, entry));
            }
            block = block.child(list);
        }

        block
    }

    /// The candidates bar: the count badge, confirm-all,
    /// show-candidates, merge-selected.
    fn candidates_bar(&self, k: &Kit) -> El {
        let state = self.state;
        let candidate_entries = state.candidate_entries();
        let selected_candidates = state.selected_candidates();
        let i18n = UseI18n::new();

        let mut bar = row()
            .key("ai-memory-candidates")
            .w_full()
            .padding(8.0)
            .gap(8.0)
            .center_y()
            .flex_wrap()
            .radius(k.radius.md)
            .border(1.0, k.c.border_muted.alpha(0.7))
            .fill(k.c.surface_base);

        bar = bar.child(
            row().key("ai-memory-candidates-title").gap(4.0).center_y()
                .child(
                    text(t("settings.ai.memory.candidatesTitle"))
                        .font(TextStyle { weight: 500.0, ..TEXT_XS })
                        .color(k.c.text_muted),
                )
                .child(
                    col()
                        .key("ai-memory-candidate-count")
                        .px(6.0)
                        .radius(999.0)
                        .fill(k.alpha(k.c.text_accent, 0.15))
                        .child(
                            text(state.candidate_count().to_string())
                                .font(TEXT_10)
                                .color(k.c.text_accent),
                        ),
                ),
        );

        let mut confirm_all = Button::new(i18n.t_with_params(
            "settings.ai.memory.confirmAll",
            &[("count", &candidate_entries.len().to_string())],
        ))
        .variant(ButtonVariant::Submit) // React primary
        .control_size(ControlSize::Small);
        if candidate_entries.is_empty() {
            confirm_all = confirm_all.state(State::Disabled).disabled(true);
        } else if let Some(f) = &self.on_confirm_all {
            let f = Rc::clone(f);
            confirm_all = confirm_all.on_press(move |s: &mut S| f(s));
        }
        bar = bar.child(confirm_all.build(k).key("ai-memory-confirm-all"));

        if candidate_entries.is_empty() && state.candidate_count() > 0 {
            let mut show = Button::new(t("settings.ai.memory.showCandidates"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            if let Some(f) = &self.on_set_status_filter {
                let f = Rc::clone(f);
                show = show.on_press(move |s: &mut S| f(s, "candidate".to_owned()));
            }
            bar = bar.child(show.build(k).key("ai-memory-show-candidates"));
        }

        if candidate_entries.len() >= 2 {
            let mut merge = Button::new(i18n.t_with_params(
                "settings.ai.memory.mergeSelected",
                &[("count", &selected_candidates.len().to_string())],
            ))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Small);
            if selected_candidates.len() < 2 {
                merge = merge.state(State::Disabled).disabled(true);
            } else if let Some(f) = &self.on_merge_selected {
                let f = Rc::clone(f);
                merge = merge.on_press(move |s: &mut S| f(s));
            }
            bar = bar.child(merge.build(k).key("ai-memory-merge-selected"));
        }

        bar
    }

    /// One entry row: the checkbox, the meta line / the edit form, the
    /// actions.
    fn entry_row(
        &self,
        k: &Kit,
        _layer: &mut Vec<El>,
        dark: bool,
        entry: &AiMemoryEntry,
    ) -> El {
        let state = self.state;

        let mut card = row()
            .key(format!("ai-memory-row-{}", entry.id))
            .w_full()
            .padding(8.0)
            .gap(8.0)
            .items_start()
            .radius(k.radius.md)
            .border(1.0, k.c.border_muted.alpha(0.7))
            .fill(k.c.surface_base);

        let mut checkbox = Checkbox::new(format!("ai-memory-select-{}", entry.id), state.selected.contains(&entry.id))
            .label(entry.subject.clone());
        if let Some(f) = &self.on_toggle_selected {
            let f = Rc::clone(f);
            let id = entry.id.clone();
            checkbox = checkbox.on_toggle(move |s: &mut S| f(s, id.clone()));
        }
        card = card.child(checkbox.build(k));

        // The body: the edit form while editing THIS row, else the
        // read view.
        let editing_this_row = state.editing.as_ref().filter(|edit| edit.id == entry.id);
        if let Some(edit) = editing_this_row {
            let subject_lens = Rc::clone(&self.edit_subject_edit);
            let subject_field = TextField::new("ai-memory-edit-subject", &edit.subject)
                .w_full()
                .on_edit::<S>(move |s: &mut S| subject_lens(s));
            let content_lens = Rc::clone(&self.edit_content_edit);
            let content_field = TextField::new("ai-memory-edit-content", &edit.content)
                .w_full()
                .on_edit::<S>(move |s: &mut S| content_lens(s));
            let mut save = Button::new(t("common.save"))
                .variant(ButtonVariant::Submit) // React primary
                .control_size(ControlSize::Small);
            if let Some(f) = &self.on_update_row {
                let f = Rc::clone(f);
                let id = entry.id.clone();
                save = save.on_press(move |s: &mut S| f(s, id.clone()));
            }
            let mut cancel = Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small);
            if let Some(f) = &self.on_cancel_edit {
                let f = Rc::clone(f);
                cancel = cancel.on_press(move |s: &mut S| f(s));
            }
            card = card.child(
                col().key(format!("ai-memory-edit-form-{}", entry.id)).grow(1.0).min_w(0.0).gap(4.0)
                    .child(subject_field.build(dark))
                    .child(content_field.build(dark))
                    .child(
                        row().key("ai-memory-edit-buttons").gap(4.0)
                            .child(save.build(k).key("ai-memory-edit-save"))
                            .child(cancel.build(k).key("ai-memory-edit-cancel")),
                    ),
            );
        } else {
            // The read view: subject + status/kind badges + stale/
            // conflict badges + the meta line, then the content.
            let mut meta = row().key(format!("ai-memory-meta-{}", entry.id)).gap(6.0).center_y().flex_wrap()
                .child(text(entry.subject.clone()).font(TextStyle { weight: 500.0, ..TEXT_XS }).color(k.c.text_base))
                .child(Badge::new(format!("ai-memory-status-{}", entry.id), entry.status.as_str().to_owned()).build(dark_of(k)))
                .child(Badge::new(format!("ai-memory-kind-{}", entry.id), entry.kind.as_str().to_owned()).build(dark_of(k)));
            if entry.status == datazen_ui_api::AiMemoryStatus::Stale {
                meta = meta.child(
                    Badge::new(
                        format!("ai-memory-stale-{}", entry.id),
                        t("settings.ai.memory.staleReview").to_owned(),
                    )
                    .tone(BadgeTone::Warning)
                    .build(dark_of(k)),
                );
            }
            if entry.conflict.unwrap_or(false) {
                meta = meta.child(
                    Badge::new(
                        format!("ai-memory-conflict-{}", entry.id),
                        t("settings.ai.memory.conflict").to_owned(),
                    )
                    .tone(BadgeTone::Danger)
                    .build(dark_of(k)),
                );
            }
            meta = meta.child(
                text(format!(
                    "{} · {:.2} · {}",
                    entry.scope_type.as_str(),
                    entry.confidence,
                    entry.source.as_str(),
                ))
                .font(TEXT_9)
                .color(k.c.text_muted),
            );
            card = card.child(
                col().key(format!("ai-memory-body-{}", entry.id)).grow(1.0).min_w(0.0).gap(4.0)
                    .child(meta)
                    .child(text(entry.content.clone()).font(TEXT_11).color(k.c.text_muted)),
            );
        }

        // The actions.
        let mut actions = row().key(format!("ai-memory-actions-{}", entry.id)).gap(4.0).center_y();
        if entry.status == datazen_ui_api::AiMemoryStatus::Stale {
            let mut review = Button::new(t("settings.ai.memory.markReviewed"))
                .variant(ButtonVariant::Outline) // React secondary
                .control_size(ControlSize::Small);
            if let Some(f) = &self.on_mark_reviewed {
                let f = Rc::clone(f);
                let id = entry.id.clone();
                review = review.on_press(move |s: &mut S| f(s, id.clone()));
            }
            actions = actions.child(review.build(k).key(format!("ai-memory-review-stale-{}", entry.id)));
        }
        if entry.status == datazen_ui_api::AiMemoryStatus::Candidate {
            let mut confirm = Button::new("")
                .icon(&ICON_CHECK)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .help(t("settings.ai.memory.confirm"));
            if let Some(f) = &self.on_confirm_entry {
                let f = Rc::clone(f);
                let id = entry.id.clone();
                confirm = confirm.on_press(move |s: &mut S| f(s, id.clone()));
            }
            let mut reject = Button::new("")
                .icon(&ICON_X)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .help(t("settings.ai.memory.reject"));
            if let Some(f) = &self.on_reject_entry {
                let f = Rc::clone(f);
                let id = entry.id.clone();
                reject = reject.on_press(move |s: &mut S| f(s, id.clone()));
            }
            actions = actions
                .child(confirm.build(k).key(format!("ai-memory-confirm-{}", entry.id)))
                .child(reject.build(k).key(format!("ai-memory-reject-{}", entry.id)));
        }
        let mut edit = Button::new("")
            .icon(&ICON_PENCIL)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .help(t("settings.ai.memory.edit"));
        if let Some(f) = &self.on_begin_edit {
            let f = Rc::clone(f);
            let id = entry.id.clone();
            edit = edit.on_press(move |s: &mut S| f(s, id.clone()));
        }
        let mut delete = Button::new("")
            .icon(&ICON_TRASH)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .help(t("settings.ai.memory.delete"));
        if let Some(f) = &self.on_delete_one {
            let f = Rc::clone(f);
            let id = entry.id.clone();
            delete = delete.on_press(move |s: &mut S| f(s, id.clone()));
        }
        actions = actions
            .child(edit.build(k).key(format!("ai-memory-edit-{}", entry.id)))
            .child(delete.build(k).key(format!("ai-memory-delete-{}", entry.id)));
        card = card.child(actions);

        card
    }
}

/// One org row (the read-only shape).
fn org_row(k: &Kit, entry: &AiMemoryEntry) -> El {
    let dark = k.dark;
    col()
        .key(format!("ai-org-memory-row-{}", entry.id))
        .w_full()
        .padding(8.0)
        .gap(4.0)
        .radius(k.radius.md)
        .border(1.0, k.c.border_muted.alpha(0.7))
        .fill(k.c.surface_base)
        .child(
            row().key(format!("ai-org-memory-meta-{}", entry.id)).gap(6.0).center_y().flex_wrap()
                .child(text(entry.subject.clone()).font(TextStyle { weight: 500.0, ..TEXT_XS }).color(k.c.text_base))
                .child(Badge::new(format!("ai-org-badge-{}", entry.id), t("settings.ai.memory.orgBadge").to_owned()).build(dark))
                .child(Badge::new(format!("ai-org-kind-{}", entry.id), entry.kind.as_str().to_owned()).build(dark)),
        )
        .child(text(entry.content.clone()).font(TEXT_11).color(k.c.text_muted))
}

/// One `bg-surface-alt` mini badge — the Badge atom, Neutral tone.
/// `cloud_policy_key`: the label key of one policy.
fn cloud_policy_key(policy: datazen_ui_api::AiMemoryCloudPolicy) -> &'static str {
    use datazen_ui_api::AiMemoryCloudPolicy as P;
    match policy {
        P::NormalOnly => "settings.ai.memory.cloudNormalOnly",
        P::AllowInternal => "settings.ai.memory.cloudAllowInternal",
        P::DisableCloud => "settings.ai.memory.cloudDisable",
    }
}

/// `kind_key`.
fn kind_key(kind: datazen_ui_api::AiMemoryKind) -> &'static str {
    use datazen_ui_api::AiMemoryKind as K;
    match kind {
        K::Preference => "settings.ai.memory.kind.preference",
        K::Glossary => "settings.ai.memory.kind.glossary",
        K::Relationship => "settings.ai.memory.kind.relationship",
        K::Pattern => "settings.ai.memory.kind.pattern",
        K::Correction => "settings.ai.memory.kind.correction",
        K::Environment => "settings.ai.memory.kind.environment",
        K::Negative => "settings.ai.memory.kind.negative",
        K::SchemaNote => "settings.ai.memory.kind.schema_note",
    }
}

/// `status_key`.
fn status_key(status: datazen_ui_api::AiMemoryStatus) -> &'static str {
    use datazen_ui_api::AiMemoryStatus as T;
    match status {
        T::Candidate => "settings.ai.memory.status.candidate",
        T::Active => "settings.ai.memory.status.active",
        T::Rejected => "settings.ai.memory.status.rejected",
        T::Archived => "settings.ai.memory.status.archived",
        T::Stale => "settings.ai.memory.status.stale",
    }
}

/// The delete-selected label (`{ count }` param).
fn delete_selected_label(state: &MemorySectionState) -> String {
    let i18n = UseI18n::new();
    i18n.t_with_params("settings.ai.memory.deleteSelected", &[("count", &state.selected.len().to_string())])
}

/// The kit's dark flag (the badge atom takes it, not the kit).
fn dark_of(k: &Kit) -> bool {
    k.dark
}

/// Always installs a select's toggle/close pair (the kit's guard
/// panics on an enabled select with no on_toggle at all): the host's
/// slots run when wired, a no-op rides when not.
fn wire_select_toggle<S: 'static>(
    select: Select,
    which: MemorySelect,
    toggle_slot: &Option<Rc<dyn Fn(&mut S, MemorySelect)>>,
    close_slot: &Option<Rc<dyn Fn(&mut S)>>,
) -> Select {
    let toggle = toggle_slot.clone();
    let mut select = select.on_toggle(move |s: &mut S| {
        if let Some(f) = &toggle {
            f(s, which);
        }
    });
    let close = close_slot.clone();
    select = select.on_close(move |s: &mut S| {
        if let Some(f) = &close {
            f(s);
        }
    });
    select
}
