//! Privilege tree rendering (by-user / by-object) and privilege pills.

use std::rc::Rc;

use rune::{col, icon, row, text, El};

use crate::i18n::t;
// The registry has no lucide Database/User glyph; the closest registered
// folder/table glyphs stand in for those two node kinds.
use crate::icons::{
    ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_FOLDER_OPEN, ICON_TABLE, ICON_TRASH, ICON_X,
};
use crate::ui::kit::kit;

use super::model::{PrivilegeAction, PrivilegeHostPort, PrivilegeNode, PrivilegeVm, ViewMode};

fn label(l: &str) -> String {
    if l.starts_with("privileges.") {
        t(l).to_owned()
    } else {
        l.to_owned()
    }
}

fn pill<S: 'static>(
    privilege: &str,
    revoke: Option<(String, Vec<String>, String)>,
    host: &PrivilegeHostPort<S>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut el = row()
        .key(format!("priv-pill-{privilege}"))
        .gap(2.0)
        .px(4.0)
        .radius(4.0)
        .fill(k.c.surface_raised)
        .child(text(privilege).font(k.ty.text_xsmall).color(k.c.text_base));
    if let (Some((username, privileges, object)), Some(on_action)) = (revoke, host.on_action.clone()) {
        el = el.child(
            icon(&ICON_X, 10.0)
                .color(k.c.text_muted)
                .key(format!("priv-revoke-{privilege}"))
                .on_press(move |s: &mut S| {
                    on_action(s, PrivilegeAction::Revoke {
                        username: username.clone(),
                        object_name: object.clone(),
                        privileges: privileges.clone(),
                    })
                }),
        );
    }
    el
}

fn tree_node<S: 'static>(
    node: &PrivilegeNode,
    expanded: &std::collections::BTreeSet<String>,
    revoke_user: Option<&str>,
    by_object: bool,
    supports_drop_user: bool,
    host: &PrivilegeHostPort<S>,
    dark: bool,
) -> El {
    let k = kit(dark);
    // React's ByObject view gives each user leaf its own revoke, keyed by the
    // object stored on the leaf; role-level grants have no revoke handler.
    let effective_user = if node.key == "role-level" || node.label == "privileges.roleLevel" {
        None
    } else if by_object && node.key.starts_with("user-") {
        Some(node.label.as_str())
    } else {
        revoke_user
    };
    let open = expanded.contains(&node.key);
    let has_children = !node.children.is_empty();
    let mut header = row()
        .key(format!("priv-tree-node-{}", node.key))
        .role("button")
        .label(label(&node.label))
        .w_full()
        .h(22.0)
        .pl(node.depth as f32 * 16.0 + 4.0)
        .gap(4.0)
        .center_y()
        .focusable(true);
    if has_children {
        header = header.child(icon(
            if open { &ICON_CHEVRON_DOWN } else { &ICON_CHEVRON_RIGHT },
            12.0,
        )
        .color(k.c.text_muted));
    } else {
        header = header.child(row().w(12.0).h(12.0));
    }
    let glyph = if node.key.starts_with("user-") {
        &ICON_TABLE
    } else if node.key.starts_with("db-") {
        &ICON_FOLDER_OPEN
    } else if node.key.starts_with("schema-") {
        &ICON_FOLDER_OPEN
    } else {
        &ICON_TABLE
    };
    header = header
        .child(icon(glyph, 14.0).color(k.c.text_muted))
        .child(text(label(&node.label)).font(k.ty.text_xsmall).color(k.c.text_base));
    if let Some(count) = node.count {
        header = header.child(text(format!("({count})")).font(k.ty.text_xsmall).color(k.c.text_muted));
    }
    if supports_drop_user && !by_object && node.key.starts_with("user-") {
        if let Some(on_action) = host.on_action.clone() {
            let user = node.label.clone();
            header = header.child(
                icon(&ICON_TRASH, 12.0)
                    .color(k.c.text_muted)
                    .key(format!("priv-drop-user-{user}"))
                    .on_press(move |s: &mut S| on_action(s, PrivilegeAction::DropUser(user.clone()))),
            );
        }
    }
    if let Some(on_action) = host.on_action.clone() {
        let key = node.key.clone();
        header = header.on_press(move |s: &mut S| on_action(s, PrivilegeAction::ToggleNode(key.clone())));
    }
    let mut root = col().key(format!("priv-tree-{}", node.key)).w_full().gap(2.0).child(header);
    for (privilege, object) in &node.leaves {
        let revoke = effective_user.map(|user| {
            (user.to_owned(), vec![privilege.clone()], object.clone().unwrap_or_default())
        });
        root = root.child(
            row()
                .key(format!("priv-leaf-{}", privilege))
                .w_full()
                .pl(node.depth as f32 * 16.0 + 16.0)
                .child(pill(privilege, revoke, host, dark)),
        );
    }
    if open {
        for child in &node.children {
            root = root.child(tree_node(child, expanded, revoke_user, by_object, supports_drop_user, host, dark));
        }
    }
    root
}

/// The central tree region (by-user or by-object), with empty/error banners.
#[allow(dead_code)] // view entry point; consumer: the privilege host (not wired).
pub(crate) fn privilege_tree<S: 'static>(
    vm: &PrivilegeVm,
    dark: bool,
    host: &PrivilegeHostPort<S>,
    nodes: Vec<PrivilegeNode>,
) -> El {
    let k = kit(dark);
    let mut root = col().key("privilege-view").w_full().grow(1.0).min_h(0.0).gap(2.0);
    if let Some(message) = &vm.action_error {
        root = root.child(
            row()
                .key("priv-action-error")
                .w_full()
                .gap(6.0)
                .child(text(message.clone()).font(k.ty.text_xsmall).color(k.c.danger.fg))
                .child({
                    let mut close = icon(&ICON_X, 12.0).color(k.c.danger.fg).key("priv-action-error-close");
                    if let Some(on_action) = host.on_action.clone() {
                        close = close.on_press(move |s: &mut S| on_action(s, PrivilegeAction::DismissError));
                    }
                    close
                }),
        );
    }
    if nodes.is_empty() {
        root = root.child(
            text(t("privileges.empty"))
                .key("priv-empty")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    let by_object = vm.view_mode == ViewMode::ByObject;
    let supports_drop_user = vm.supports_drop_user && !by_object;
    for node in nodes {
        let revoke_user = if !by_object {
            Some(node.label.as_str())
        } else {
            None
        };
        root = root.child(tree_node(&node, &vm.expanded, revoke_user, by_object, supports_drop_user, host, dark));
    }
    root
}
