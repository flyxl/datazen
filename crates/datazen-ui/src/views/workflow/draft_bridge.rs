//! Port of `packages/frontend/src/windows/workflow/draftBridge.ts`
//! (T484) — the richer editor draft's parking lot behind the legacy
//! WorkflowPage callback.
//!
//! The legacy callback currently reconstructs `WorkflowStep` and
//! therefore only forwards the historical fields; the bridge keeps
//! the richer editor draft here until that callback is migrated,
//! so Command fields are not silently lost.
//!
//! ## What maps where
//!
//! - React's module-level `Map` is [`WorkflowDraftBridge`] — the
//!   host owns one (Rune has no module state to park it in).
//! - `rememberWorkflowDraft(draft)` is
//!   [`WorkflowDraftBridge::remember`]: only a draft with a
//!   non-empty id parks (`if (draft.id)`).
//! - `forgetWorkflowDraft(id)` is [`WorkflowDraftBridge::forget`].
//! - `mergeDraftCommandFields(workflow)` is
//!   [`WorkflowDraftBridge::merge_command_fields`] over the wire
//!   JSON (the UI crate has no `WorkflowDefinition` type of its
//!   own; the merge touches `id`, `connection` and the command
//!   steps' `command` / `input` — everything else rides through
//!   verbatim, exactly as React's spreads).
//!
//! Known deltas: the bridge stores the draft's command-relevant
//! projection (connection + per-step command/input), not the whole
//! draft object — that is everything the merge reads.

// No production consumer yet (the legacy WorkflowPage callback is
// the host's); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use crate::views::workflow::workflow_form::{WorkflowDraft, WorkflowStepKind};

/// One parked step's command-relevant projection.
#[derive(Clone, Debug, PartialEq)]
struct BridgeStep {
    /// Whether the draft's step is a command step (only command
    /// steps contribute — React's `source.type !== 'command'`
    /// guard).
    is_command: bool,
    command: Option<String>,
    input: Map<String, Value>,
}

/// One parked draft.
#[derive(Clone, Debug, PartialEq)]
struct BridgeEntry {
    connection: Option<String>,
    steps: Vec<BridgeStep>,
}

/// The module-level `Map<string, WorkflowDraft>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct WorkflowDraftBridge {
    drafts: HashMap<String, BridgeEntry>,
}

impl WorkflowDraftBridge {
    /// `rememberWorkflowDraft(draft)` — only a draft with an id
    /// parks.
    pub(crate) fn remember(&mut self, draft: &WorkflowDraft) {
        let id = draft.id.text.trim().to_owned();
        if id.is_empty() {
            return;
        }
        let steps = draft
            .steps
            .iter()
            .map(|step| BridgeStep {
                is_command: step.kind == WorkflowStepKind::Command,
                command: step.command.clone(),
                input: step.input.clone(),
            })
            .collect();
        self.drafts.insert(
            id,
            BridgeEntry { connection: draft.connection.clone(), steps },
        );
    }

    /// `forgetWorkflowDraft(id)`.
    pub(crate) fn forget(&mut self, id: &str) {
        self.drafts.remove(id);
    }

    /// Whether a draft is parked under `id`.
    pub(crate) fn has(&self, id: &str) -> bool {
        self.drafts.contains_key(id)
    }

    /// `mergeDraftCommandFields(workflow)`: the workflow's missing
    /// connection borrows the draft's, and every command step's
    /// `command` / `input` come back from the parked draft (the
    /// legacy callback reconstructed them away). Everything else
    /// rides through verbatim.
    pub(crate) fn merge_command_fields(&self, workflow: &Value) -> Value {
        let Some(id) = workflow.get("id").and_then(Value::as_str) else {
            return workflow.clone();
        };
        let Some(entry) = self.drafts.get(id) else {
            return workflow.clone();
        };
        let mut next = workflow.clone();

        // `connection: workflow.connection ?? draft.connection` —
        // an absent OR null field borrows (and a draft without one
        // leaves it as it was).
        let connection_missing = match next.get("connection") {
            None | Some(Value::Null) => true,
            Some(_) => false,
        };
        if connection_missing {
            if let Some(connection) = &entry.connection {
                next["connection"] = json!(connection);
            }
        }

        // `steps.map(...)`: the index-aligned walk; only when BOTH
        // the workflow's step and the draft's step are command
        // steps does the merge fire.
        if let Some(steps) = next.get_mut("steps").and_then(Value::as_array_mut) {
            for (index, step) in steps.iter_mut().enumerate() {
                let Some(source) = entry.steps.get(index) else {
                    continue;
                };
                let step_is_command = step.get("type").and_then(Value::as_str) == Some("command");
                if !step_is_command || !source.is_command {
                    continue;
                }
                if let Some(command) = &source.command {
                    step["command"] = json!(command);
                }
                // `input: source.input ?? {}`.
                step["input"] = Value::Object(source.input.clone());
            }
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn workflow(steps: Value) -> Value {
        json!({
            "id": "wf-1",
            "name": "Nightly import",
            "connection": null,
            "steps": steps,
        })
    }

    /// A parked draft whose second step is a command step.
    fn bridge_with_command() -> WorkflowDraftBridge {
        let mut bridge = WorkflowDraftBridge::default();
        let mut draft = WorkflowDraft::empty();
        draft.id = rune::TextEdit::new("wf-1".to_owned());
        draft.connection = Some("pg-main".to_owned());
        bridge.remember(&draft);
        bridge
    }

    #[test]
    fn an_idless_draft_never_parks() {
        let mut bridge = WorkflowDraftBridge::default();
        let mut draft = WorkflowDraft::empty();
        draft.id = rune::TextEdit::new("   ".to_owned());
        bridge.remember(&draft);
        assert!(bridge.drafts.is_empty());
        assert!(!bridge.has(""));
    }

    #[test]
    fn a_workflow_without_a_parked_draft_rides_verbatim() {
        let bridge = WorkflowDraftBridge::default();
        let wf = workflow(json!([
            { "type": "query", "id": "step1", "sql": "SELECT 1" }
        ]));
        assert_eq!(bridge.merge_command_fields(&wf), wf);
    }

    #[test]
    fn the_missing_connection_borrows_the_drafts() {
        let bridge = bridge_with_command();
        let wf = workflow(json!([
            { "type": "query", "id": "step1", "sql": "SELECT 1" }
        ]));
        let merged = bridge.merge_command_fields(&wf);
        assert_eq!(merged["connection"], json!("pg-main"));
        // A workflow that already has one keeps it.
        let mut own = wf.clone();
        own["connection"] = json!("sqlite-local");
        let merged = bridge.merge_command_fields(&own);
        assert_eq!(merged["connection"], json!("sqlite-local"));
        // And one with no connection anywhere stays absent (a
        // draft without one leaves it as it was).
        let mut bare = bridge_with_command();
        bare.drafts.get_mut("wf-1").expect("parked").connection = None;
        let mut wf_bare = workflow(json!([]));
        wf_bare.as_object_mut().expect("object").remove("connection");
        let merged = bare.merge_command_fields(&wf_bare);
        assert!(merged.get("connection").is_none());
    }

    #[test]
    fn command_fields_come_back_only_for_command_steps() {
        let mut bridge = WorkflowDraftBridge::default();
        let mut draft = WorkflowDraft::empty();
        draft.id = rune::TextEdit::new("wf-1".to_owned());
        draft.steps.clear();
        // A query step (ignored), a command step (the payload), an
        // AI step (ignored).
        for (kind, command, input) in [
            (WorkflowStepKind::Query, None, json!({})),
            (
                WorkflowStepKind::Command,
                Some("backup.export".to_owned()),
                json!({ "format": "sql" }),
            ),
            (WorkflowStepKind::Ai, None, json!({})),
        ] {
            let mut step = crate::views::workflow::workflow_form::WorkflowStepDraft::query("s");
            step.kind = kind;
            step.command = command;
            step.input = input.as_object().cloned().unwrap_or_default();
            draft.steps.push(step);
        }
        bridge.remember(&draft);

        // The reconstructed workflow: the command step lost its
        // fields to the legacy callback.
        let wf = workflow(json!([
            { "type": "query", "id": "s1", "sql": "SELECT 1" },
            { "type": "command", "id": "s2" },
            { "type": "ai", "id": "s3", "prompt": "hi" },
        ]));
        let merged = bridge.merge_command_fields(&wf);
        let steps = merged["steps"].as_array().expect("steps");
        assert_eq!(steps[0]["type"], json!("query"), "the query step rides verbatim");
        assert_eq!(steps[1]["command"], json!("backup.export"));
        assert_eq!(steps[1]["input"], json!({ "format": "sql" }));
        assert_eq!(steps[2].get("command"), None, "the AI step gains nothing");
        // A non-command workflow step never borrows.
        let wf = workflow(json!([
            { "type": "ai", "id": "s2", "prompt": "hi" },
        ]));
        let merged = bridge.merge_command_fields(&wf);
        assert!(merged["steps"][0].get("command").is_none());

        // A misaligned index (the workflow grew) merges only the
        // aligned pairs.
        let wf = workflow(json!([
            { "type": "query", "id": "s0", "sql": "SELECT 0" },
            { "type": "query", "id": "s1", "sql": "SELECT 1" },
            { "type": "command", "id": "s2" },
        ]));
        let merged = bridge.merge_command_fields(&wf);
        let steps = merged["steps"].as_array().expect("steps");
        // Index 1 pairs the draft's command step with a query step:
        // no borrow (the workflow's own step is not a command).
        assert!(steps[1].get("command").is_none());
        // Index 2 pairs the draft's AI step with the command step:
        // no borrow (the draft's step is not a command).
        assert!(steps[2].get("command").is_none());

        // Forgetting clears the parking.
        bridge.forget("wf-1");
        assert!(!bridge.has("wf-1"));
        let wf = workflow(json!([{ "type": "command", "id": "s2" }]));
        assert_eq!(bridge.merge_command_fields(&wf), wf);
    }
}
