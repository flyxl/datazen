//! Port of `windows/connection/TableNoPrimaryKeyTip.tsx` (DB-03/DB-16).
//!
//! A small informational strip telling the user the table has no primary key.
//! DB-16 adds an optional "Edit in SQL" escape hatch; a caller that passes no
//! action gets the exact pre-DB-16 shape. The optional action is an injected
//! host callback — no fake IPC. The registry has no `Info` glyph, so the
//! registered `ICON_TABLE` stands in for the leading info icon.
//!
//! ## Keys
//!
//! `table-no-primary-key-tip` is React's `testId` and `role="status"`;
//! `table-no-primary-key-edit-in-sql` is the optional action's test id.
//!
//! ## Local stand-ins (removal stage)
//!
//! No production host mounts this tip yet; the table host owns the message and
//! the action label. Every reusable item carries a per-item `#[allow(dead_code)]`
//! with removal stage "drop when the table host wires this tip".

use std::rc::Rc;

use rune::{icon, row, text, El};
use rune_kit::Kit;

use crate::icons::ICON_TABLE;
use crate::ui::kit::kit;

pub(crate) const ROOT_KEY: &str = "table-no-primary-key-tip";
pub(crate) const TEXT_KEY: &str = "table-no-primary-key-text";
pub(crate) const ACTION_KEY: &str = "table-no-primary-key-edit-in-sql";

/// React `TableNoPrimaryKeyTipAction` (label already translated by the host).
#[allow(dead_code)] // VM mirror; remove when the host wires the tip.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NoPkAction {
    pub(crate) label: String,
}

/// React props: the message and the optional action slot.
#[allow(dead_code)] // VM mirror; remove when the host wires the tip.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NoPrimaryKeyTipVm {
    pub(crate) message: String,
    pub(crate) action: Option<NoPkAction>,
}

/// The sole optional React callback (`action.onClick`).
#[allow(dead_code)] // host port; remove when the host wires the tip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoPkTipAction {
    EditInSql,
}

#[allow(dead_code)] // host port; remove when the host wires the tip.
pub(crate) type NoPkTipActionFn<S> = Rc<dyn Fn(&mut S, NoPkTipAction) + 'static>;

/// Build the tip into `layer`.
#[allow(dead_code)] // view entry; remove when the host wires the tip.
pub(crate) fn build<S: 'static>(
    vm: &NoPrimaryKeyTipVm,
    on_action: &Option<NoPkTipActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k: &Kit = kit(dark);
    let mut root = row()
        .key(ROOT_KEY)
        .role("status")
        .w_full()
        .min_w(0.0)
        .gap(6.0)
        .center_y()
        .px(12.0)
        .py(6.0)
        .fill(k.c.layer_02)
        .child(icon(&ICON_TABLE, 14.0).color(k.c.text_muted).a11y_hidden())
        .child(
            text(vm.message.clone())
                .key(TEXT_KEY)
                .label(vm.message.clone())
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        );
    if let Some(action) = vm.action.as_ref() {
        let mut button = text(action.label.clone())
            .key(ACTION_KEY)
            .role("button")
            .focusable(true)
            .label(action.label.clone())
            .font(k.ty.text_small)
            .color(k.c.text_base)
            .px(6.0)
            .py(2.0)
            .radius(4.0)
            .border(1.0, k.c.border_base);
        if let Some(callback) = on_action.clone() {
            button = button.on_press(move |s: &mut S| callback(s, NoPkTipAction::EditInSql));
        }
        root = root.child(button);
    }
    layer.push(root);
}
