#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowMenuItem {
    pub kind: &'static str,
    pub id: &'static str,
    pub label: String,
    pub action: fn(),
}

impl WorkflowMenuItem {
    pub fn invoke(&self) {
        (self.action)();
    }
}

fn item(id: &'static str, label: Option<&str>, action: Option<fn()>) -> Option<WorkflowMenuItem> {
    let action = action?;
    Some(WorkflowMenuItem {
        kind: "item",
        id,
        label: label.unwrap_or("").to_owned(),
        action,
    })
}

fn push<const N: usize>(defs: [Option<WorkflowMenuItem>; N]) -> Vec<WorkflowMenuItem> {
    defs.into_iter().flatten().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowListContextMenuLabels {
    pub open: String,
    pub delete: String,
    pub copy_name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkflowListContextMenuHandlers {
    pub on_open: Option<fn()>,
    pub on_delete: Option<fn()>,
    pub on_copy_name: Option<fn()>,
}

pub struct BuildWorkflowListContextMenuArgs {
    pub labels: WorkflowListContextMenuLabels,
    pub handlers: WorkflowListContextMenuHandlers,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkflowHistoryContextMenuLabels {
    pub open_detail: String,
    pub delete: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkflowHistoryContextMenuHandlers {
    pub on_open_detail: Option<fn()>,
    pub on_delete: Option<fn()>,
}

pub struct BuildWorkflowHistoryContextMenuArgs {
    pub labels: WorkflowHistoryContextMenuLabels,
    pub handlers: WorkflowHistoryContextMenuHandlers,
}

pub const WORKFLOW_LIST_MENU_GAP: [&str; 3] = [
    "NativeMenuItemDef from ./nativeContextMenu",
    "native context-menu bridge registration",
    "right-click event wiring on the workflow sidebar entry",
];

pub const WORKFLOW_LIST_MENU_SURFACE: [&str; 8] = [
    "buildWorkflowListContextMenuItems",
    "buildWorkflowHistoryContextMenuItems",
    "WorkflowListContextMenuLabels",
    "WorkflowListContextMenuHandlers",
    "WorkflowHistoryContextMenuLabels",
    "WorkflowHistoryContextMenuHandlers",
    "WorkflowMenuItem",
    "WORKFLOW_LIST_MENU_GAP",
];

pub fn build_workflow_list_context_menu_items(
    args: BuildWorkflowListContextMenuArgs,
) -> Vec<WorkflowMenuItem> {
    let BuildWorkflowListContextMenuArgs { labels, handlers } = args;
    push([
        item("open", Some(&labels.open), handlers.on_open),
        item("delete", Some(&labels.delete), handlers.on_delete),
        item("copy-name", Some(&labels.copy_name), handlers.on_copy_name),
    ])
}

pub fn build_workflow_history_context_menu_items(
    args: BuildWorkflowHistoryContextMenuArgs,
) -> Vec<WorkflowMenuItem> {
    let BuildWorkflowHistoryContextMenuArgs { labels, handlers } = args;
    let delete_label = labels.delete.as_deref().unwrap_or("");
    push([
        item(
            "open-detail",
            Some(&labels.open_detail),
            handlers.on_open_detail,
        ),
        item("delete", Some(delete_label), handlers.on_delete),
    ])
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    thread_local! {
        static CALLS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    fn record(name: &'static str) {
        CALLS.with(|calls| calls.borrow_mut().push(name));
    }

    fn reset_calls() {
        CALLS.with(|calls| calls.borrow_mut().clear());
    }

    fn calls() -> Vec<&'static str> {
        CALLS.with(|calls| calls.borrow().clone())
    }

    fn on_open() {
        record("open");
    }
    fn on_delete() {
        record("delete");
    }
    fn on_copy() {
        record("copy-name");
    }
    fn on_detail() {
        record("open-detail");
    }

    fn list_labels() -> WorkflowListContextMenuLabels {
        WorkflowListContextMenuLabels {
            open: "Open".to_owned(),
            delete: "Delete".to_owned(),
            copy_name: "Copy name".to_owned(),
        }
    }

    #[test]
    fn list_menu_builds_all_three_items_in_order() {
        let items = build_workflow_list_context_menu_items(BuildWorkflowListContextMenuArgs {
            labels: list_labels(),
            handlers: WorkflowListContextMenuHandlers {
                on_open: Some(on_open),
                on_delete: Some(on_delete),
                on_copy_name: Some(on_copy),
            },
        });
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].id, "open");
        assert_eq!(items[0].label, "Open");
        assert_eq!(items[1].id, "delete");
        assert_eq!(items[1].label, "Delete");
        assert_eq!(items[2].id, "copy-name");
        assert_eq!(items[2].label, "Copy name");
        assert!(items.iter().all(|item| item.kind == "item"));
    }

    #[test]
    fn list_menu_omits_items_without_a_handler() {
        let none = build_workflow_list_context_menu_items(BuildWorkflowListContextMenuArgs {
            labels: list_labels(),
            handlers: WorkflowListContextMenuHandlers::default(),
        });
        assert!(none.is_empty());

        let partial = build_workflow_list_context_menu_items(BuildWorkflowListContextMenuArgs {
            labels: list_labels(),
            handlers: WorkflowListContextMenuHandlers {
                on_open: Some(on_open),
                on_copy_name: Some(on_copy),
                ..WorkflowListContextMenuHandlers::default()
            },
        });
        assert_eq!(partial.len(), 2);
        assert_eq!(partial[0].id, "open");
        assert_eq!(partial[1].id, "copy-name");
    }

    #[test]
    fn list_menu_items_invoke_their_own_handler() {
        reset_calls();
        let items = build_workflow_list_context_menu_items(BuildWorkflowListContextMenuArgs {
            labels: list_labels(),
            handlers: WorkflowListContextMenuHandlers {
                on_open: Some(on_open),
                on_delete: Some(on_delete),
                on_copy_name: Some(on_copy),
            },
        });
        items[2].invoke();
        items[0].invoke();
        items[1].invoke();
        assert_eq!(calls(), ["copy-name", "open", "delete"]);
    }

    #[test]
    fn history_menu_includes_delete_only_when_a_handler_is_supplied() {
        let items =
            build_workflow_history_context_menu_items(BuildWorkflowHistoryContextMenuArgs {
                labels: WorkflowHistoryContextMenuLabels {
                    open_detail: "Details".to_owned(),
                    delete: Some("Remove".to_owned()),
                },
                handlers: WorkflowHistoryContextMenuHandlers {
                    on_open_detail: Some(on_detail),
                    on_delete: Some(on_delete),
                },
            });
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "open-detail");
        assert_eq!(items[0].label, "Details");
        assert_eq!(items[1].id, "delete");
        assert_eq!(items[1].label, "Remove");

        let detail_only =
            build_workflow_history_context_menu_items(BuildWorkflowHistoryContextMenuArgs {
                labels: WorkflowHistoryContextMenuLabels {
                    open_detail: "Details".to_owned(),
                    delete: Some("Remove".to_owned()),
                },
                handlers: WorkflowHistoryContextMenuHandlers {
                    on_open_detail: Some(on_detail),
                    on_delete: None,
                },
            });
        assert_eq!(detail_only.len(), 1);
        assert_eq!(detail_only[0].id, "open-detail");

        let empty =
            build_workflow_history_context_menu_items(BuildWorkflowHistoryContextMenuArgs {
                labels: WorkflowHistoryContextMenuLabels::default(),
                handlers: WorkflowHistoryContextMenuHandlers::default(),
            });
        assert!(empty.is_empty());
    }

    #[test]
    fn history_delete_without_a_label_uses_an_empty_string() {
        let items =
            build_workflow_history_context_menu_items(BuildWorkflowHistoryContextMenuArgs {
                labels: WorkflowHistoryContextMenuLabels::default(),
                handlers: WorkflowHistoryContextMenuHandlers {
                    on_delete: Some(on_delete),
                    ..WorkflowHistoryContextMenuHandlers::default()
                },
            });
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "delete");
        assert_eq!(items[0].label, "");
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(WORKFLOW_LIST_MENU_GAP.len(), 3);
        assert_eq!(WORKFLOW_LIST_MENU_SURFACE.len(), 8);
        assert!(WORKFLOW_LIST_MENU_SURFACE.contains(&"buildWorkflowListContextMenuItems"));
        assert_eq!(crate::WORKFLOW_LIST_MENU_GAP.len(), 3);
        assert_eq!(crate::WORKFLOW_LIST_MENU_SURFACE.len(), 8);
        let _: Option<crate::WorkflowMenuItem> = Some(WorkflowMenuItem {
            kind: "item",
            id: "open",
            label: "Open".to_owned(),
            action: on_open,
        });
    }
}
