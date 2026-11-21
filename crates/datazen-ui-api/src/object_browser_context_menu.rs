#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectBrowserMenuAction {
    Refresh,
    Open,
    CopyName,
    CopyDdl,
    Execute,
    Format,
    Comment,
}

impl ObjectBrowserMenuAction {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectBrowserMenuAction::Refresh => "refresh",
            ObjectBrowserMenuAction::Open => "open",
            ObjectBrowserMenuAction::CopyName => "copy-name",
            ObjectBrowserMenuAction::CopyDdl => "copy-ddl",
            ObjectBrowserMenuAction::Execute => "execute",
            ObjectBrowserMenuAction::Format => "format",
            ObjectBrowserMenuAction::Comment => "comment",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEditItem {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl NativeEditItem {
    pub fn as_str(self) -> &'static str {
        match self {
            NativeEditItem::Cut => "cut",
            NativeEditItem::Copy => "copy",
            NativeEditItem::Paste => "paste",
            NativeEditItem::SelectAll => "selectAll",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectBrowserMenuItem {
    Item {
        id: String,
        label: String,
        enabled: bool,
        action: ObjectBrowserMenuAction,
    },
    Separator,
    Predefined(NativeEditItem),
}

pub fn native_edit_menu_items() -> Vec<ObjectBrowserMenuItem> {
    vec![
        ObjectBrowserMenuItem::Predefined(NativeEditItem::Cut),
        ObjectBrowserMenuItem::Predefined(NativeEditItem::Copy),
        ObjectBrowserMenuItem::Predefined(NativeEditItem::Paste),
        ObjectBrowserMenuItem::Predefined(NativeEditItem::SelectAll),
    ]
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectBrowserListMenuLabels {
    pub open: String,
    pub copy_name: String,
    pub copy_ddl: String,
    pub refresh: String,
}

#[derive(Default)]
pub struct ObjectBrowserListMenuHandlers {
    pub on_open: Option<Box<dyn Fn()>>,
    pub on_copy_name: Option<Box<dyn Fn()>>,
    pub on_copy_ddl: Option<Box<dyn Fn()>>,
    pub on_refresh: Option<Box<dyn Fn()>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectBrowserEditorMenuLabels {
    pub execute: String,
    pub format: String,
    pub comment: String,
}

#[derive(Default)]
pub struct ObjectBrowserEditorMenuHandlers {
    pub on_execute: Option<Box<dyn Fn()>>,
    pub on_format: Option<Box<dyn Fn()>>,
    pub on_comment: Option<Box<dyn Fn()>>,
}

fn item(
    id: &str,
    label: &str,
    present: bool,
    enabled: bool,
    action: ObjectBrowserMenuAction,
) -> Option<ObjectBrowserMenuItem> {
    if !present {
        return None;
    }
    Some(ObjectBrowserMenuItem::Item {
        id: id.to_owned(),
        label: label.to_owned(),
        enabled,
        action,
    })
}

fn push(defs: Vec<Option<ObjectBrowserMenuItem>>) -> Vec<ObjectBrowserMenuItem> {
    defs.into_iter().flatten().collect()
}

pub fn build_object_browser_list_menu_items(
    labels: &ObjectBrowserListMenuLabels,
    handlers: &ObjectBrowserListMenuHandlers,
) -> Vec<ObjectBrowserMenuItem> {
    push(vec![
        item("refresh", &labels.refresh, handlers.on_refresh.is_some(), true, ObjectBrowserMenuAction::Refresh),
        item("open", &labels.open, handlers.on_open.is_some(), true, ObjectBrowserMenuAction::Open),
        item("copy-name", &labels.copy_name, handlers.on_copy_name.is_some(), true, ObjectBrowserMenuAction::CopyName),
        item("copy-ddl", &labels.copy_ddl, handlers.on_copy_ddl.is_some(), true, ObjectBrowserMenuAction::CopyDdl),
    ])
}

pub fn build_object_browser_editor_menu_items(
    labels: &ObjectBrowserEditorMenuLabels,
    handlers: &ObjectBrowserEditorMenuHandlers,
    sql_text: &str,
    has_selection: bool,
) -> Vec<ObjectBrowserMenuItem> {
    let non_empty = !sql_text.trim().is_empty();
    let mut items = native_edit_menu_items();
    items.push(ObjectBrowserMenuItem::Separator);
    items.extend(push(vec![
        item("execute", &labels.execute, handlers.on_execute.is_some(), non_empty, ObjectBrowserMenuAction::Execute),
        item("format", &labels.format, handlers.on_format.is_some(), non_empty, ObjectBrowserMenuAction::Format),
        item("comment", &labels.comment, handlers.on_comment.is_some(), non_empty || has_selection, ObjectBrowserMenuAction::Comment),
    ]));
    items
}

impl ObjectBrowserMenuItem {
    pub fn invoke_list(&self, handlers: &ObjectBrowserListMenuHandlers) {
        let ObjectBrowserMenuItem::Item { action, .. } = self else {
            return;
        };
        let callback = match action {
            ObjectBrowserMenuAction::Refresh => handlers.on_refresh.as_ref(),
            ObjectBrowserMenuAction::Open => handlers.on_open.as_ref(),
            ObjectBrowserMenuAction::CopyName => handlers.on_copy_name.as_ref(),
            ObjectBrowserMenuAction::CopyDdl => handlers.on_copy_ddl.as_ref(),
            _ => None,
        };
        if let Some(callback) = callback {
            callback();
        }
    }

    pub fn invoke_editor(&self, handlers: &ObjectBrowserEditorMenuHandlers) {
        let ObjectBrowserMenuItem::Item { action, .. } = self else {
            return;
        };
        let callback = match action {
            ObjectBrowserMenuAction::Execute => handlers.on_execute.as_ref(),
            ObjectBrowserMenuAction::Format => handlers.on_format.as_ref(),
            ObjectBrowserMenuAction::Comment => handlers.on_comment.as_ref(),
            _ => None,
        };
        if let Some(callback) = callback {
            callback();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn edit_block_is_the_four_predefined_items_in_order() {
        let items = native_edit_menu_items();
        assert_eq!(
            items,
            vec![
                ObjectBrowserMenuItem::Predefined(NativeEditItem::Cut),
                ObjectBrowserMenuItem::Predefined(NativeEditItem::Copy),
                ObjectBrowserMenuItem::Predefined(NativeEditItem::Paste),
                ObjectBrowserMenuItem::Predefined(NativeEditItem::SelectAll),
            ]
        );
    }

    #[test]
    fn list_menu_keeps_order_and_skips_missing_handlers() {
        let labels = ObjectBrowserListMenuLabels {
            open: "Open".to_owned(),
            copy_name: "Copy Name".to_owned(),
            copy_ddl: "Copy DDL".to_owned(),
            refresh: "Refresh".to_owned(),
        };
        let handlers = ObjectBrowserListMenuHandlers {
            on_copy_ddl: Some(Box::new(|| ())),
            ..Default::default()
        };
        let items = build_object_browser_list_menu_items(&labels, &handlers);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0], ObjectBrowserMenuItem::Item {
            id: "copy-ddl".to_owned(),
            label: "Copy DDL".to_owned(),
            enabled: true,
            action: ObjectBrowserMenuAction::CopyDdl,
        });

        let all = ObjectBrowserListMenuHandlers {
            on_open: Some(Box::new(|| ())),
            on_copy_name: Some(Box::new(|| ())),
            on_copy_ddl: Some(Box::new(|| ())),
            on_refresh: Some(Box::new(|| ())),
        };
        let items = build_object_browser_list_menu_items(&labels, &all);
        assert_eq!(
            items.iter().map(|item| match item {
                ObjectBrowserMenuItem::Item { id, .. } => id.as_str(),
                _ => "",
            }).collect::<Vec<_>>(),
            vec!["refresh", "open", "copy-name", "copy-ddl"]
        );
    }

    #[test]
    fn editor_menu_has_edit_block_separator_and_actions() {
        let labels = ObjectBrowserEditorMenuLabels {
            execute: "Execute".to_owned(),
            format: "Format".to_owned(),
            comment: "Comment".to_owned(),
        };
        let handlers = ObjectBrowserEditorMenuHandlers {
            on_execute: Some(Box::new(|| ())),
            on_format: Some(Box::new(|| ())),
            on_comment: Some(Box::new(|| ())),
        };
        let items = build_object_browser_editor_menu_items(&labels, &handlers, "SELECT 1", false);
        assert_eq!(items.len(), 8);
        assert_eq!(items[4], ObjectBrowserMenuItem::Separator);
        assert!(matches!(&items[5], ObjectBrowserMenuItem::Item { id, enabled: true, .. } if id == "execute"));
        assert!(matches!(&items[7], ObjectBrowserMenuItem::Item { id, enabled: true, .. } if id == "comment"));
    }

    #[test]
    fn editor_enablement_follows_trimmed_sql_and_selection() {
        let labels = ObjectBrowserEditorMenuLabels::default();
        let handlers = ObjectBrowserEditorMenuHandlers {
            on_execute: Some(Box::new(|| ())),
            on_format: Some(Box::new(|| ())),
            on_comment: Some(Box::new(|| ())),
        };
        let items = build_object_browser_editor_menu_items(&labels, &handlers, "   ", false);
        assert!(matches!(&items[5], ObjectBrowserMenuItem::Item { enabled: false, .. }));
        assert!(matches!(&items[6], ObjectBrowserMenuItem::Item { enabled: false, .. }));
        assert!(matches!(&items[7], ObjectBrowserMenuItem::Item { enabled: false, .. }));

        let items = build_object_browser_editor_menu_items(&labels, &handlers, "", true);
        assert!(matches!(&items[7], ObjectBrowserMenuItem::Item { enabled: true, .. }), "selection alone enables comment");
    }

    #[test]
    fn invoke_dispatches_only_the_matching_handler() {
        let calls = Rc::new(Cell::new(0usize));
        let sink = calls.clone();
        let handlers = ObjectBrowserListMenuHandlers {
            on_open: Some(Box::new(move || sink.set(sink.get() + 1))),
            ..Default::default()
        };
        let items = build_object_browser_list_menu_items(&ObjectBrowserListMenuLabels::default(), &handlers);
        items[0].invoke_list(&handlers);
        assert_eq!(calls.get(), 1);
        ObjectBrowserMenuItem::Separator.invoke_list(&handlers);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::native_edit_menu_items;
        let _ = crate::build_object_browser_list_menu_items;
        let _ = crate::build_object_browser_editor_menu_items;
        let _: Option<crate::ObjectBrowserMenuItem> = None;
        let _: Option<crate::ObjectBrowserMenuAction> = None;
        let _: Option<crate::NativeEditItem> = None;
    }
}
