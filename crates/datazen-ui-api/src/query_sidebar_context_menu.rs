#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuerySidebarMenuItem {
    pub kind: &'static str,
    pub id: &'static str,
    pub label: String,
    pub action: fn(),
}

impl QuerySidebarMenuItem {
    pub fn invoke(&self) {
        (self.action)();
    }
}

fn item(
    id: &'static str,
    label: Option<&str>,
    action: Option<fn()>,
) -> Option<QuerySidebarMenuItem> {
    match (label, action) {
        (Some(label), Some(action)) if !label.is_empty() => Some(QuerySidebarMenuItem {
            kind: "item",
            id,
            label: label.to_owned(),
            action,
        }),
        _ => None,
    }
}

fn push<const N: usize>(defs: [Option<QuerySidebarMenuItem>; N]) -> Vec<QuerySidebarMenuItem> {
    defs.into_iter().flatten().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuerySidebarFavoriteLabels {
    pub apply_sql: String,
    pub open_in_new_tab: Option<String>,
    pub copy_sql: String,
    pub delete: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuerySidebarFavoriteHandlers {
    pub on_apply_sql: Option<fn()>,
    pub on_open_in_new_tab: Option<fn()>,
    pub on_copy_sql: Option<fn()>,
    pub on_delete: Option<fn()>,
}

pub struct BuildFavoriteSidebarContextMenuArgs {
    pub labels: QuerySidebarFavoriteLabels,
    pub handlers: QuerySidebarFavoriteHandlers,
}

pub fn build_favorite_sidebar_context_menu_items(
    args: BuildFavoriteSidebarContextMenuArgs,
) -> Vec<QuerySidebarMenuItem> {
    let BuildFavoriteSidebarContextMenuArgs { labels, handlers } = args;
    push([
        item(
            "open-in-new-tab",
            labels.open_in_new_tab.as_deref(),
            handlers.on_open_in_new_tab,
        ),
        item("apply-sql", Some(&labels.apply_sql), handlers.on_apply_sql),
        item("copy-sql", Some(&labels.copy_sql), handlers.on_copy_sql),
        item("delete", Some(&labels.delete), handlers.on_delete),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuerySidebarHistoryLabels {
    pub apply_sql: String,
    pub copy_sql: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuerySidebarHistoryHandlers {
    pub on_apply_sql: Option<fn()>,
    pub on_copy_sql: Option<fn()>,
}

pub struct BuildHistorySidebarContextMenuArgs {
    pub labels: QuerySidebarHistoryLabels,
    pub handlers: QuerySidebarHistoryHandlers,
}

pub fn build_history_sidebar_context_menu_items(
    args: BuildHistorySidebarContextMenuArgs,
) -> Vec<QuerySidebarMenuItem> {
    let BuildHistorySidebarContextMenuArgs { labels, handlers } = args;
    push([
        item("apply-sql", Some(&labels.apply_sql), handlers.on_apply_sql),
        item("copy-sql", Some(&labels.copy_sql), handlers.on_copy_sql),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuerySidebarHistoryHeaderLabels {
    pub clear_history: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuerySidebarHistoryHeaderHandlers {
    pub on_clear_history: Option<fn()>,
}

pub struct BuildHistorySidebarHeaderContextMenuArgs {
    pub labels: QuerySidebarHistoryHeaderLabels,
    pub handlers: QuerySidebarHistoryHeaderHandlers,
}

pub fn build_history_sidebar_header_context_menu_items(
    args: BuildHistorySidebarHeaderContextMenuArgs,
) -> Vec<QuerySidebarMenuItem> {
    let BuildHistorySidebarHeaderContextMenuArgs { labels, handlers } = args;
    push([item(
        "clear-history",
        Some(&labels.clear_history),
        handlers.on_clear_history,
    )])
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

    fn on_open() {
        record("open");
    }
    fn on_apply() {
        record("apply");
    }
    fn on_copy() {
        record("copy");
    }
    fn on_delete() {
        record("delete");
    }
    fn on_clear() {
        record("clear");
    }

    fn favorite_labels() -> QuerySidebarFavoriteLabels {
        QuerySidebarFavoriteLabels {
            apply_sql: "Apply".to_owned(),
            open_in_new_tab: Some("Open in new tab".to_owned()),
            copy_sql: "Copy".to_owned(),
            delete: "Delete".to_owned(),
        }
    }

    fn favorite_handlers() -> QuerySidebarFavoriteHandlers {
        QuerySidebarFavoriteHandlers {
            on_apply_sql: Some(on_apply),
            on_open_in_new_tab: Some(on_open),
            on_copy_sql: Some(on_copy),
            on_delete: Some(on_delete),
        }
    }

    #[test]
    fn favorite_builds_all_items_in_order() {
        let items =
            build_favorite_sidebar_context_menu_items(BuildFavoriteSidebarContextMenuArgs {
                labels: favorite_labels(),
                handlers: favorite_handlers(),
            });
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            ["open-in-new-tab", "apply-sql", "copy-sql", "delete"]
        );
        for item in &items {
            assert_eq!(item.kind, "item");
        }
        assert_eq!(items[1].label, "Apply");
    }

    #[test]
    fn favorite_skips_missing_handlers_and_blank_labels() {
        let handlers = QuerySidebarFavoriteHandlers {
            on_apply_sql: Some(on_apply),
            ..Default::default()
        };
        let items =
            build_favorite_sidebar_context_menu_items(BuildFavoriteSidebarContextMenuArgs {
                labels: favorite_labels(),
                handlers,
            });
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            ["apply-sql"]
        );

        let mut labels = favorite_labels();
        labels.copy_sql = String::new();
        labels.delete = String::new();
        let items =
            build_favorite_sidebar_context_menu_items(BuildFavoriteSidebarContextMenuArgs {
                labels,
                handlers: favorite_handlers(),
            });
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            ["open-in-new-tab", "apply-sql"]
        );

        let mut labels = favorite_labels();
        labels.open_in_new_tab = None;
        let items =
            build_favorite_sidebar_context_menu_items(BuildFavoriteSidebarContextMenuArgs {
                labels,
                handlers: favorite_handlers(),
            });
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            ["apply-sql", "copy-sql", "delete"]
        );
    }

    #[test]
    fn history_builds_apply_and_copy_only() {
        let items = build_history_sidebar_context_menu_items(BuildHistorySidebarContextMenuArgs {
            labels: QuerySidebarHistoryLabels {
                apply_sql: "Apply".to_owned(),
                copy_sql: "Copy".to_owned(),
            },
            handlers: QuerySidebarHistoryHandlers {
                on_apply_sql: Some(on_apply),
                on_copy_sql: Some(on_copy),
            },
        });
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            ["apply-sql", "copy-sql"]
        );
        assert!(items.iter().all(|item| item.kind == "item"));

        let items = build_history_sidebar_context_menu_items(BuildHistorySidebarContextMenuArgs {
            labels: QuerySidebarHistoryLabels {
                apply_sql: "Apply".to_owned(),
                copy_sql: "Copy".to_owned(),
            },
            handlers: QuerySidebarHistoryHandlers::default(),
        });
        assert!(items.is_empty());
    }

    #[test]
    fn header_builds_clear_history_only() {
        let items = build_history_sidebar_header_context_menu_items(
            BuildHistorySidebarHeaderContextMenuArgs {
                labels: QuerySidebarHistoryHeaderLabels {
                    clear_history: "Clear".to_owned(),
                },
                handlers: QuerySidebarHistoryHeaderHandlers {
                    on_clear_history: Some(on_clear),
                },
            },
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "clear-history");
        assert_eq!(items[0].label, "Clear");

        let items = build_history_sidebar_header_context_menu_items(
            BuildHistorySidebarHeaderContextMenuArgs {
                labels: QuerySidebarHistoryHeaderLabels {
                    clear_history: String::new(),
                },
                handlers: QuerySidebarHistoryHeaderHandlers {
                    on_clear_history: Some(on_clear),
                },
            },
        );
        assert!(items.is_empty());
    }

    #[test]
    fn invoke_dispatches_to_the_stored_handler_in_order() {
        CALLS.with(|calls| calls.borrow_mut().clear());
        let items =
            build_favorite_sidebar_context_menu_items(BuildFavoriteSidebarContextMenuArgs {
                labels: favorite_labels(),
                handlers: favorite_handlers(),
            });
        for item in &items {
            item.invoke();
        }
        assert_eq!(
            CALLS.with(|calls| calls.borrow().clone()),
            ["open", "apply", "copy", "delete"]
        );
    }
}
