#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErMenuAction {
    OpenTable,
    CopyName,
    FocusTable,
}

impl ErMenuAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenTable => "open-table",
            Self::CopyName => "copy-name",
            Self::FocusTable => "focus-table",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErMenuItem {
    pub id: String,
    pub label: String,
    pub action: ErMenuAction,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErNodeContextMenuLabels {
    pub open_table: String,
    pub copy_name: String,
    pub focus_table: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErNodeContextMenuHandlers {
    pub on_open_table: bool,
    pub on_copy_name: bool,
    pub on_focus_table: bool,
}

fn item(id: &str, label: String, action: ErMenuAction, present: bool) -> Option<ErMenuItem> {
    if !present {
        return None;
    }
    Some(ErMenuItem {
        id: id.to_owned(),
        label,
        action,
    })
}

fn push(defs: [Option<ErMenuItem>; 3]) -> Vec<ErMenuItem> {
    defs.into_iter().flatten().collect()
}

pub fn build_er_node_context_menu_items(
    labels: ErNodeContextMenuLabels,
    handlers: ErNodeContextMenuHandlers,
) -> Vec<ErMenuItem> {
    push([
        item("open-table", labels.open_table, ErMenuAction::OpenTable, handlers.on_open_table),
        item("copy-name", labels.copy_name, ErMenuAction::CopyName, handlers.on_copy_name),
        item("focus-table", labels.focus_table, ErMenuAction::FocusTable, handlers.on_focus_table),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> ErNodeContextMenuLabels {
        ErNodeContextMenuLabels {
            open_table: "Open".to_owned(),
            copy_name: "Copy".to_owned(),
            focus_table: "Focus".to_owned(),
        }
    }

    fn ids(items: &[ErMenuItem]) -> Vec<&str> {
        items.iter().map(|item| item.id.as_str()).collect()
    }

    #[test]
    fn all_handlers_produce_three_items_in_a_fixed_order() {
        let handlers = ErNodeContextMenuHandlers {
            on_open_table: true,
            on_copy_name: true,
            on_focus_table: true,
        };
        let items = build_er_node_context_menu_items(labels(), handlers);
        assert_eq!(ids(&items), vec!["open-table", "copy-name", "focus-table"]);
        assert_eq!(items[0].label, "Open");
        assert_eq!(items[1].label, "Copy");
        assert_eq!(items[2].label, "Focus");
        assert_eq!(items[0].action, ErMenuAction::OpenTable);
        assert_eq!(items[1].action, ErMenuAction::CopyName);
        assert_eq!(items[2].action, ErMenuAction::FocusTable);
    }

    #[test]
    fn a_missing_handler_omits_its_item_without_gaps() {
        let cases = [
            (ErNodeContextMenuHandlers { on_open_table: false, on_copy_name: true, on_focus_table: true }, vec!["copy-name", "focus-table"]),
            (ErNodeContextMenuHandlers { on_open_table: true, on_copy_name: false, on_focus_table: true }, vec!["open-table", "focus-table"]),
            (ErNodeContextMenuHandlers { on_open_table: true, on_copy_name: true, on_focus_table: false }, vec!["open-table", "copy-name"]),
        ];
        for (handlers, expected) in cases {
            let items = build_er_node_context_menu_items(labels(), handlers);
            assert_eq!(ids(&items), expected);
        }
    }

    #[test]
    fn no_handlers_produces_an_empty_menu() {
        let items = build_er_node_context_menu_items(labels(), ErNodeContextMenuHandlers::default());
        assert!(items.is_empty());
    }

    #[test]
    fn only_the_focus_item_is_optional_by_default() {
        let handlers = ErNodeContextMenuHandlers { on_open_table: true, on_copy_name: true, on_focus_table: false };
        assert_eq!(ids(&build_er_node_context_menu_items(labels(), handlers)), vec!["open-table", "copy-name"]);
    }

    #[test]
    fn action_ids_are_the_react_literals() {
        assert_eq!(ErMenuAction::OpenTable.as_str(), "open-table");
        assert_eq!(ErMenuAction::CopyName.as_str(), "copy-name");
        assert_eq!(ErMenuAction::FocusTable.as_str(), "focus-table");
    }

    #[test]
    fn labels_are_forwarded_verbatim() {
        let supplied = ErNodeContextMenuLabels {
            open_table: String::new(),
            copy_name: "  spaced  ".to_owned(),
            focus_table: "聚焦".to_owned(),
        };
        let handlers = ErNodeContextMenuHandlers { on_open_table: true, on_copy_name: true, on_focus_table: true };
        let items = build_er_node_context_menu_items(supplied, handlers);
        assert_eq!(items[0].label, "");
        assert_eq!(items[1].label, "  spaced  ");
        assert_eq!(items[2].label, "聚焦");
    }
}
