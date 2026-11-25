//! The navigation guard — `resolveEditorBeforeNavigation` / `guardNavigation`.
//!
//! An open editor owns a draft. A page move must ask it to write that draft
//! first, and a draft the editor refuses to commit stops the navigation: the
//! editor stays open with its inline error instead of losing the typing. The
//! host owns the editor and its `commit()` result; this module only decides
//! whether the navigation may proceed and what the host must do.

use super::CellValueErrorReason;

/// React `ActiveCellEditor` reduced to the facts the guard reads. `commit` is
/// the host's already-computed verdict of `editor.commit()`; `None` means the
/// draft was written (or the editor was closed) without an error.
#[allow(dead_code)] // VM mirror; drop when the host owns the active editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenEditorVm {
    pub panel_id: String,
    pub commit: Option<CellValueErrorReason>,
}

/// React `EditorNavigationGuard` (`{ ok: true }` / `{ ok: false, reason }`).
#[allow(dead_code)] // decision; drop when the host owns navigation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorNavigationGuardVm {
    pub ok: bool,
    pub reason: Option<CellValueErrorReason>,
}

/// React `resolveEditorBeforeNavigation`: an editor that is not ours (or none
/// at all) cannot hold our draft, so the navigation is unconditionally allowed.
#[allow(dead_code)] // pure gate; drop when the host owns navigation.
pub fn resolve_editor_before_navigation(
    panel_id: &str,
    active: Option<&OpenEditorVm>,
) -> EditorNavigationGuardVm {
    let Some(editor) = active else {
        return EditorNavigationGuardVm {
            ok: true,
            reason: None,
        };
    };
    if editor.panel_id != panel_id {
        return EditorNavigationGuardVm {
            ok: true,
            reason: None,
        };
    }
    match editor.commit {
        Some(reason) => EditorNavigationGuardVm {
            ok: false,
            reason: Some(reason),
        },
        None => EditorNavigationGuardVm {
            ok: true,
            reason: None,
        },
    }
}

/// What the host of `guardNavigation` must do. `commit_editor` is true only
/// for an editor belonging to this panel; `navigate` is false when its draft
/// was refused. No callback is invoked here — the host executes the plan.
#[allow(dead_code)] // host intent; drop when the host owns navigation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationPlanVm {
    pub commit_editor: bool,
    pub navigate: bool,
    pub reason: Option<CellValueErrorReason>,
}

/// React `guardNavigation`: run the action and return `true` only after the
/// guard accepted the navigation; a refusal keeps the editor open.
#[allow(dead_code)] // pure decision; drop when the host owns navigation.
pub fn guard_navigation(
    panel_id: &str,
    active: Option<&OpenEditorVm>,
) -> NavigationPlanVm {
    let guard = resolve_editor_before_navigation(panel_id, active);
    let commit_editor = active.is_some_and(|editor| editor.panel_id == panel_id);
    NavigationPlanVm {
        commit_editor,
        navigate: guard.ok,
        reason: guard.reason,
    }
}
