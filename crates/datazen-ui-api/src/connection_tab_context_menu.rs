#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionTabAction {
    Close,
    CloseOthers,
    CloseToTheRight,
    CloseToTheLeft,
    CloseAll,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionTabMenuItem {
    pub kind: &'static str,
    pub id: &'static str,
    pub label: String,
    pub enabled: bool,
    pub action: ConnectionTabAction,
}

impl ConnectionTabMenuItem {
    pub fn invoke(&self, handlers: &ConnectionTabContextMenuHandlers) {
        let handler = match self.action {
            ConnectionTabAction::Close => handlers.on_close,
            ConnectionTabAction::CloseOthers => handlers.on_close_others,
            ConnectionTabAction::CloseToTheRight => handlers.on_close_to_the_right,
            ConnectionTabAction::CloseToTheLeft => handlers.on_close_to_the_left,
            ConnectionTabAction::CloseAll => handlers.on_close_all,
        };
        if let Some(handler) = handler {
            handler();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionTabContextMenuLabels {
    pub close: String,
    pub close_others: String,
    pub close_all: String,
    pub close_to_the_right: String,
    pub close_to_the_left: String,
}

#[derive(Clone, Copy, Default)]
pub struct ConnectionTabContextMenuHandlers {
    pub on_close: Option<fn()>,
    pub on_close_others: Option<fn()>,
    pub on_close_all: Option<fn()>,
    pub on_close_to_the_right: Option<fn()>,
    pub on_close_to_the_left: Option<fn()>,
}

pub struct BuildConnectionTabContextMenuArgs {
    pub labels: ConnectionTabContextMenuLabels,
    pub handlers: ConnectionTabContextMenuHandlers,
    pub only_one_tab: bool,
    pub has_tabs_to_right: bool,
    pub has_tabs_to_left: bool,
}

fn item(
    id: &'static str,
    label: String,
    action: ConnectionTabAction,
    handler: Option<fn()>,
    enabled: bool,
) -> Option<ConnectionTabMenuItem> {
    handler?;
    Some(ConnectionTabMenuItem {
        kind: "item",
        id,
        label,
        enabled,
        action,
    })
}

pub fn build_connection_tab_context_menu_items(
    args: BuildConnectionTabContextMenuArgs,
) -> Vec<ConnectionTabMenuItem> {
    let BuildConnectionTabContextMenuArgs {
        labels,
        handlers,
        only_one_tab,
        has_tabs_to_right,
        has_tabs_to_left,
    } = args;
    let mut items = Vec::new();
    if let Some(entry) = item(
        "close",
        labels.close,
        ConnectionTabAction::Close,
        handlers.on_close,
        true,
    ) {
        items.push(entry);
    }
    if let Some(entry) = item(
        "close-others",
        labels.close_others,
        ConnectionTabAction::CloseOthers,
        handlers.on_close_others,
        !only_one_tab,
    ) {
        items.push(entry);
    }
    if let Some(entry) = item(
        "close-to-the-right",
        labels.close_to_the_right,
        ConnectionTabAction::CloseToTheRight,
        handlers.on_close_to_the_right,
        has_tabs_to_right,
    ) {
        items.push(entry);
    }
    if let Some(entry) = item(
        "close-to-the-left",
        labels.close_to_the_left,
        ConnectionTabAction::CloseToTheLeft,
        handlers.on_close_to_the_left,
        has_tabs_to_left,
    ) {
        items.push(entry);
    }
    if let Some(entry) = item(
        "close-all",
        labels.close_all,
        ConnectionTabAction::CloseAll,
        handlers.on_close_all,
        true,
    ) {
        items.push(entry);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static CALLS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    fn record(name: &'static str) {
        CALLS.with(|calls| calls.borrow_mut().push(name));
    }

    fn on_close() {
        record("close");
    }
    fn on_close_others() {
        record("others");
    }
    fn on_close_all() {
        record("all");
    }
    fn on_close_to_the_right() {
        record("right");
    }
    fn on_close_to_the_left() {
        record("left");
    }

    fn labels() -> ConnectionTabContextMenuLabels {
        ConnectionTabContextMenuLabels {
            close: "Close".to_owned(),
            close_others: "Close Others".to_owned(),
            close_all: "Close All".to_owned(),
            close_to_the_right: "Close to the Right".to_owned(),
            close_to_the_left: "Close to the Left".to_owned(),
        }
    }

    fn all_handlers() -> ConnectionTabContextMenuHandlers {
        ConnectionTabContextMenuHandlers {
            on_close: Some(on_close),
            on_close_others: Some(on_close_others),
            on_close_all: Some(on_close_all),
            on_close_to_the_right: Some(on_close_to_the_right),
            on_close_to_the_left: Some(on_close_to_the_left),
        }
    }

    fn args(
        handlers: ConnectionTabContextMenuHandlers,
        only_one_tab: bool,
        has_tabs_to_right: bool,
        has_tabs_to_left: bool,
    ) -> BuildConnectionTabContextMenuArgs {
        BuildConnectionTabContextMenuArgs {
            labels: labels(),
            handlers,
            only_one_tab,
            has_tabs_to_right,
            has_tabs_to_left,
        }
    }

    #[test]
    fn builds_all_items_in_order_when_handlers_are_set() {
        let items =
            build_connection_tab_context_menu_items(args(all_handlers(), false, true, true));
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            [
                "close",
                "close-others",
                "close-to-the-right",
                "close-to-the-left",
                "close-all"
            ]
        );
        for item in &items {
            assert_eq!(item.kind, "item");
            assert!(item.enabled);
        }
        assert_eq!(items[0].action, ConnectionTabAction::Close);
        assert_eq!(items[1].action, ConnectionTabAction::CloseOthers);
        assert_eq!(items[2].action, ConnectionTabAction::CloseToTheRight);
        assert_eq!(items[3].action, ConnectionTabAction::CloseToTheLeft);
        assert_eq!(items[4].action, ConnectionTabAction::CloseAll);
    }

    #[test]
    fn disabled_states_follow_the_flags() {
        let items =
            build_connection_tab_context_menu_items(args(all_handlers(), true, false, false));
        let by_id = |id: &str| items.iter().find(|item| item.id == id).unwrap();
        assert!(by_id("close").enabled);
        assert!(!by_id("close-others").enabled);
        assert!(!by_id("close-to-the-right").enabled);
        assert!(!by_id("close-to-the-left").enabled);
        assert!(by_id("close-all").enabled);

        let items =
            build_connection_tab_context_menu_items(args(all_handlers(), false, false, false));
        let by_id = |id: &str| items.iter().find(|item| item.id == id).unwrap();
        assert!(by_id("close-others").enabled);
        assert!(!by_id("close-to-the-right").enabled);
        assert!(!by_id("close-to-the-left").enabled);
    }

    #[test]
    fn missing_handlers_skip_only_their_items() {
        let handlers = ConnectionTabContextMenuHandlers {
            on_close: Some(on_close),
            ..Default::default()
        };
        let items = build_connection_tab_context_menu_items(args(handlers, true, true, true));
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            ["close"]
        );
        let empty = build_connection_tab_context_menu_items(args(
            ConnectionTabContextMenuHandlers::default(),
            false,
            false,
            false,
        ));
        assert!(empty.is_empty());
    }

    #[test]
    fn caller_supplied_labels_are_kept() {
        let mut custom = labels();
        custom.close = "关闭".to_owned();
        custom.close_others = "关闭其他".to_owned();
        custom.close_to_the_right = "关闭右侧".to_owned();
        custom.close_to_the_left = "关闭左侧".to_owned();
        custom.close_all = "关闭全部".to_owned();
        let items = build_connection_tab_context_menu_items(BuildConnectionTabContextMenuArgs {
            labels: custom,
            handlers: all_handlers(),
            only_one_tab: false,
            has_tabs_to_right: true,
            has_tabs_to_left: true,
        });
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["关闭", "关闭其他", "关闭右侧", "关闭左侧", "关闭全部"]
        );
    }

    #[test]
    fn invoke_dispatches_to_the_matching_handler() {
        CALLS.with(|calls| calls.borrow_mut().clear());
        let handlers = all_handlers();
        let items = build_connection_tab_context_menu_items(args(handlers, false, true, true));
        for item in &items {
            item.invoke(&handlers);
        }
        assert_eq!(
            CALLS.with(|calls| calls.borrow().clone()),
            ["close", "others", "right", "left", "all"]
        );
    }

    #[test]
    fn invoke_is_a_noop_when_no_handler_is_registered() {
        CALLS.with(|calls| calls.borrow_mut().clear());
        let handlers = ConnectionTabContextMenuHandlers::default();
        let menu = ConnectionTabMenuItem {
            kind: "item",
            id: "close",
            label: "Close".to_owned(),
            enabled: true,
            action: ConnectionTabAction::Close,
        };
        menu.invoke(&handlers);
        assert!(CALLS.with(|calls| calls.borrow().is_empty()));
    }
}
