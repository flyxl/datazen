//! `TableReadOnlyTip`, ported 1:1 from
//! `packages/frontend/src/windows/connection/TableReadOnlyTip.tsx` (DB-05).
//!
//! "Read-only" alone leaves the user guessing: three different causes need
//! three different fixes. This one line names the reason from
//! `lib/cellEditorKind.readOnlyReasonMessageKey`. A host-supplied dismiss
//! affordance is optional (React's component has none); when present it uses
//! `common.close` and emits [`TableReadOnlyTipAction::Dismiss`].
//!
//! ## Removal stage
//!
//! No production host mounts this tip yet; the table host will feed the reason
//! and handle dismissal. Per-item allow annotations carry that removal stage.

use std::rc::Rc;

use rune::{row, text, El};

use crate::i18n::t;
use crate::icons::ICON_X;
use crate::ui::kit::kit;

/// React `CellReadOnlyReason`.
#[allow(dead_code)] // VM mirror; supplied by the cell editability host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellReadOnlyReason {
    #[default]
    ConnectionReadOnly,
    DriverCapability,
    View,
    NoPrimaryKey,
    AmbiguousRowIdentity,
    AutoIncrement,
    Generated,
    BinaryColumn,
    LargeValue,
    TypeNotEditable,
}

/// React `readOnlyReasonMessageKey(reason)`.
#[allow(dead_code)] // pure helper; consumed by the tip and tests.
pub fn reason_message_key(reason: CellReadOnlyReason) -> &'static str {
    match reason {
        CellReadOnlyReason::ConnectionReadOnly => "cellEditor.readOnly.connection",
        CellReadOnlyReason::DriverCapability => "cellEditor.readOnly.driverCapability",
        CellReadOnlyReason::View => "cellEditor.readOnly.view",
        CellReadOnlyReason::NoPrimaryKey => "cellEditor.readOnly.noPrimaryKey",
        CellReadOnlyReason::AmbiguousRowIdentity => "cellEditor.readOnly.ambiguousRowIdentity",
        CellReadOnlyReason::AutoIncrement => "cellEditor.readOnly.autoIncrement",
        CellReadOnlyReason::Generated => "cellEditor.readOnly.generated",
        CellReadOnlyReason::BinaryColumn => "cellEditor.readOnly.binaryColumn",
        CellReadOnlyReason::LargeValue => "cellEditor.readOnly.largeValue",
        CellReadOnlyReason::TypeNotEditable => "cellEditor.readOnly.typeNotEditable",
    }
}

/// Host-owned projection of the tip props.
#[allow(dead_code)] // VM surface; owned by the table host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableReadOnlyTipVm {
    /// `None` means there is no read-only reason to report (React would not mount).
    pub reason: Option<CellReadOnlyReason>,
    pub dismissible: bool,
}

/// The optional host intent from the dismiss affordance.
#[allow(dead_code)] // host port; removed with the table wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableReadOnlyTipAction {
    Dismiss,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, TableReadOnlyTipAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: TableReadOnlyTipAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the tip; a missing reason mounts only an inert empty marker.
#[allow(dead_code)] // consumer: the table host (not landed).
pub fn build<S: 'static>(
    vm: &TableReadOnlyTipVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let Some(reason) = vm.reason else {
        return rune::el().key("table-read-only-tip-empty").a11y_hidden();
    };
    let k = kit(dark);
    let mut line = row()
        .key("table-read-only-tip")
        .role("status")
        .w_full()
        .gap(6.0)
        .center_y()
        .px(12.0)
        .py(6.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.warning.border)
        .child(rune::icon(&ICON_X, 14.0).color(k.c.warning.icon))
        .child(text(t("tableData.readOnlyEditDisabled")).color(k.c.warning.fg))
        .child(
            text(t(reason_message_key(reason)))
                .key("table-read-only-reason")
                .color(k.c.warning.fg),
        );
    if vm.dismissible {
        line = line.child(
            row()
                .grow(1.0)
                .min_w(0.0),
        ).child(
            rune::icon(&ICON_X, 14.0)
                .key("table-read-only-tip-dismiss")
                .role("button")
                .focusable(true)
                .label(t("common.close"))
                .on_press(emit(&on_action, TableReadOnlyTipAction::Dismiss)),
        );
    }
    line
}
