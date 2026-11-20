//! Port of `packages/frontend/src/windows/settings/AiToolBridgeSection.tsx`
//! (T198) — the opt-in switches for model-facing bridge tools.
//!
//! A plugin/workflow is absent from the Agent tool list until its id is
//! stored in `aiPluginToolIds` / `aiWorkflowToolIds`; the host
//! re-resolves both lists on every turn, so turning a switch off removes
//! the tool at once.
//!
//! ## What maps where
//!
//! - The mount effect (`fetchWapps` + `aiCommands.workflowList()`) is
//!   the host's section-entry step: the workflow rows land through
//!   [`AiToolBridgeSectionState::note_workflow_rows`] (`rows ?? []` — a
//!   null answer is an empty list) and a failure through
//!   [`AiToolBridgeSectionState::note_workflow_error`]
//!   (`toErrorMessage`, the red line). The wapp list is the host's own
//!   store, passed as [`BridgeWappVm`] rows (id, name, enabled, the
//!   manifest's `aiTools` — the only fields this section reads of
//!   `WappSummary`).
//! - `bridgePlugins` (wapps with at least one declared AI tool) is this
//!   section's own filter, kept here.
//! - The toggles' set logic is [`toggle_tool_id`]: enable = remove-then-
//!   append (deduped, latest position), disable = remove. The host
//!   applies it to `settings.aiPluginToolIds` /
//!   `aiWorkflowToolIds` through the section's callbacks.
//! - The plugin hint: enabled wapps spell `{id} · {tool list}` with each
//!   read-only tool suffixed ` (read-only)`; a disabled wapp says
//!   请先启用该插件 (`settings.ai.toolsPluginDisabled`) and its row is
//!   disabled (the value still displays). The workflow hint is
//!   `description || id`.
//! - `SectionTitle`/`ToggleRow` (settingsUi.tsx, T399) stand in as in
//!   T172: the 13/600 muted title with the tooltip hint, kit
//!   [`Checkbox`] rows with label + description.
//!
//! Known deltas: `uppercase tracking-wide` renders without case or
//! tracking (the zh-CN strings carry no case); the card's border/spacing
//! approximates Tailwind's rounded-xl border-edge p-4.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `ai-tool-bridge-section`,
//! `ai-plugin-tool-{id}`, `ai-workflow-tool-{id}`. Unlabeled parts
//! derive from the section key: the plugins header
//! `<key>-plugins-head`, its empty line `<key>-plugins-empty`, the
//! workflows header `<key>-workflows-head`, its empty line
//! `<key>-workflows-empty`, and the error line `<key>-error`.

use std::rc::Rc;

use rune::{col, icon, row, text, tok, El};
use rune_kit::{ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_BOT, ICON_WORKFLOWS};
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;
use crate::ui::tooltip::Tooltip;
use crate::views::settings::ai_settings_section::section_title;

/// One bridge-eligible wapp row (the `WappSummary` fields this section
/// reads: the manifest's AI tools decide eligibility, `enabled` decides
/// whether the row can toggle).
#[derive(Clone)]
pub(crate) struct BridgeWappVm {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub ai_tools: Vec<BridgeAiToolVm>,
}

/// One declared AI tool (`WappAiToolSummary`'s name + readOnly — the
/// hint's currency).
#[derive(Clone)]
pub(crate) struct BridgeAiToolVm {
    pub name: String,
    pub read_only: bool,
}

/// One workflow row (`WorkflowListItem`'s id/name/description — the
/// section's whole read of it).
#[derive(Clone)]
pub(crate) struct BridgeWorkflowVm {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// The toggle's set logic: enable = remove-then-append (deduped, the id
/// moves to the end), disable = remove.
pub(crate) fn toggle_tool_id(ids: &[String], id: &str, enabled: bool) -> Vec<String> {
    let mut next: Vec<String> = ids.iter().filter(|existing| existing.as_str() != id).cloned().collect();
    if enabled {
        next.push(id.to_owned());
    }
    next
}

/// The section's own state: the workflow list and its load error.
#[derive(Clone, Default)]
pub(crate) struct AiToolBridgeSectionState {
    workflows: Vec<BridgeWorkflowVm>,
    workflow_error: Option<String>,
}

impl AiToolBridgeSectionState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The loaded workflow rows (the render's source).
    pub(crate) fn workflows(&self) -> &[BridgeWorkflowVm] {
        &self.workflows
    }

    /// The load error, if the last `workflowList` rejected.
    pub(crate) fn workflow_error(&self) -> Option<&str> {
        self.workflow_error.as_deref()
    }

    /// `workflowList`'s answer: `rows ?? []` (a null answer is an empty
    /// list, not an error), clearing any stale error.
    pub(crate) fn note_workflow_rows(&mut self, rows: Option<Vec<BridgeWorkflowVm>>) {
        self.workflows = rows.unwrap_or_default();
        self.workflow_error = None;
    }

    /// `workflowList`'s rejection (`toErrorMessage`): the red line; the
    /// rows stay whatever they were.
    pub(crate) fn note_workflow_error(&mut self, message: String) {
        self.workflow_error = Some(message);
    }
}

/// The AI tool bridge section (React `AiToolBridgeSection`).
pub(crate) struct AiToolBridgeSection<'a, S: 'static> {
    key: &'a str,
    state: &'a AiToolBridgeSectionState,
    /// The host's wapp rows; the bridge filter (`aiTools` non-empty) is
    /// applied here.
    wapps: &'a [BridgeWappVm],
    /// `settings.aiPluginToolIds ?? []`.
    plugin_ids: &'a [String],
    /// `settings.aiWorkflowToolIds ?? []`.
    workflow_ids: &'a [String],
    on_toggle_plugin: Option<Rc<dyn Fn(&mut S, String, bool)>>,
    on_toggle_workflow: Option<Rc<dyn Fn(&mut S, String, bool)>>,
}

impl<'a, S: 'static> AiToolBridgeSection<'a, S> {
    pub(crate) fn new(
        state: &'a AiToolBridgeSectionState,
        wapps: &'a [BridgeWappVm],
        plugin_ids: &'a [String],
        workflow_ids: &'a [String],
    ) -> Self {
        Self {
            key: "ai-tool-bridge-section",
            state,
            wapps,
            plugin_ids,
            workflow_ids,
            on_toggle_plugin: None,
            on_toggle_workflow: None,
        }
    }

    /// A plugin switch's press (the host applies [`toggle_tool_id`] to
    /// `aiPluginToolIds`).
    pub(crate) fn on_toggle_plugin(
        mut self,
        f: impl Fn(&mut S, String, bool) + 'static,
    ) -> Self {
        self.on_toggle_plugin = Some(Rc::new(f));
        self
    }

    /// A workflow switch's press (the host applies [`toggle_tool_id`] to
    /// `aiWorkflowToolIds`).
    pub(crate) fn on_toggle_workflow(
        mut self,
        f: impl Fn(&mut S, String, bool) + 'static,
    ) -> Self {
        self.on_toggle_workflow = Some(Rc::new(f));
        self
    }

    /// Build the section (tooltips land in `layer`).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let key = self.key;

        let mut section =
            col().key(key.to_owned()).w_full().gap(16.0).child(section_title(
                &k,
                layer,
                format!("{key}-title"),
                t("settings.ai.toolsTitle"),
                t("settings.ai.toolsHint"),
            ));

        let mut card = col().key(format!("{key}-card")).w_full().gap(12.0).px(16.0).py(16.0);

        // -- Plugins --
        card = card.child(
            row()
                .key(format!("{key}-plugins-head"))
                .w_full()
                .gap(8.0)
                .center_y()
                .child(icon(&ICON_BOT, 14.0).color(k.c.text_muted))
                .child(
                    text(t("settings.ai.toolsPlugins").to_owned())
                        .font(tok::type_::LABEL)
                        .color(k.c.text_muted),
                ),
        );

        let bridge_plugins: Vec<&BridgeWappVm> =
            self.wapps.iter().filter(|wapp| !wapp.ai_tools.is_empty()).collect();
        if bridge_plugins.is_empty() {
            card = card.child(
                text(t("settings.ai.toolsNoPlugins").to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.text_muted)
                    .key(format!("{key}-plugins-empty")),
            );
        } else {
            for wapp in bridge_plugins {
                let enabled = self.plugin_ids.iter().any(|id| id == &wapp.id);
                let hint = if wapp.enabled {
                    let tools: Vec<String> = wapp
                        .ai_tools
                        .iter()
                        .map(|tool| {
                            if tool.read_only {
                                format!("{} (read-only)", tool.name)
                            } else {
                                tool.name.clone()
                            }
                        })
                        .collect();
                    format!("{} · {}", wapp.id, tools.join(", "))
                } else {
                    t("settings.ai.toolsPluginDisabled").to_owned()
                };
                let mut row_el = Checkbox::new(format!("ai-plugin-tool-{}", wapp.id), enabled)
                    .label(wapp.name.clone())
                    .description(hint)
                    .disabled(!wapp.enabled);
                if let Some(on_toggle) = &self.on_toggle_plugin {
                    let on_toggle = Rc::clone(on_toggle);
                    let id = wapp.id.clone();
                    row_el = row_el.on_toggle(move |s: &mut S| on_toggle(s, id.clone(), !enabled));
                }
                card = card.child(row_el.build(&k));
            }
        }

        // -- Workflows --
        card = card.child(
            row()
                .key(format!("{key}-workflows-head"))
                .w_full()
                .gap(8.0)
                .center_y()
                .pt(12.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .child(icon(&ICON_WORKFLOWS, 14.0).color(k.c.text_muted))
                .child(
                    text(t("settings.ai.toolsWorkflows").to_owned())
                        .font(tok::type_::LABEL)
                        .color(k.c.text_muted),
                ),
        );

        if let Some(error) = &self.state.workflow_error {
            card = card.child(
                text(error.clone())
                    .font(tok::type_::LABEL)
                    .color(k.c.danger.fg)
                    .selectable(true)
                    .key(format!("{key}-error")),
            );
        }
        if self.state.workflows.is_empty() && self.state.workflow_error.is_none() {
            card = card.child(
                text(t("settings.ai.toolsNoWorkflows").to_owned())
                    .font(tok::type_::LABEL)
                    .color(k.c.text_muted)
                    .key(format!("{key}-workflows-empty")),
            );
        } else {
            for workflow in &self.state.workflows {
                let enabled = self.workflow_ids.iter().any(|id| id == &workflow.id);
                let hint = if workflow.description.is_empty() {
                    workflow.id.clone()
                } else {
                    workflow.description.clone()
                };
                let mut row_el = Checkbox::new(format!("ai-workflow-tool-{}", workflow.id), enabled)
                    .label(workflow.name.clone())
                    .description(hint);
                if let Some(on_toggle) = &self.on_toggle_workflow {
                    let on_toggle = Rc::clone(on_toggle);
                    let id = workflow.id.clone();
                    row_el = row_el.on_toggle(move |s: &mut S| on_toggle(s, id.clone(), !enabled));
                }
                card = card.child(row_el.build(&k));
            }
        }

        section.child(card)
    }
}
