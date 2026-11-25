//! `TableFkBreadcrumb`, ported 1:1 from
//! `packages/frontend/src/windows/connection/TableFkBreadcrumb.tsx` (DB-07).
//!
//! A pure presentation bar answering "where did this FK jump come from and how
//! do I go back". The navigation stack stays in the host: the model comes in,
//! `Back` / `Forward` / `Dismiss` go out through an injected port. No fake IPC.
//!
//! ## Removal stage
//!
//! No production host mounts this bar yet; the table host will feed the origin
//! and apply the three actions to its FK navigation stack. Per-item allow
//! annotations carry that removal stage until then.

use std::rc::Rc;

use rune::{el, row, text, El};

use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, ICON_X};
use crate::ui::kit::kit;

/// React `FkOrigin` (the fields the breadcrumb reads).
#[allow(dead_code)] // VM mirror; supplied by the FK navigation host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FkOrigin {
    pub source_panel_id: String,
    pub source_table: String,
    pub local_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
    pub nav_entry_id: String,
}

/// Host-owned projection of the component props.
#[allow(dead_code)] // VM surface; owned by the table host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableFkBreadcrumbVm {
    /// `None` means there is no FK jump to describe (React would not mount).
    pub origin: Option<FkOrigin>,
    pub truncated: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// Whether the host supplied `onDismiss`.
    pub dismissible: bool,
}

/// React `fkOriginLabel`: `orders.customer_id → customers.id`, with every
/// column of a composite key shown on both sides.
#[allow(dead_code)] // pure helper; consumed by the view and tests.
pub fn fk_origin_label(model: &FkOrigin) -> String {
    let side = |table: &str, columns: &[String]| -> String {
        match columns.len() {
            0 => table.to_owned(),
            1 => format!("{}.{}", table, columns[0]),
            _ => format!("{}.({})", table, columns.join(", ")),
        }
    };
    format!(
        "{} → {}",
        side(&model.source_table, &model.local_columns),
        side(&model.target_table, &model.target_columns)
    )
}

/// Host intents from the three buttons.
#[allow(dead_code)] // host port; removed with the table wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableFkAction {
    Back,
    Forward,
    Dismiss,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, TableFkAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: TableFkAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the breadcrumb; a missing origin mounts only an inert empty marker.
#[allow(dead_code)] // consumer: the table host (not landed).
pub fn build<S: 'static>(
    vm: &TableFkBreadcrumbVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let Some(origin) = vm.origin.as_ref() else {
        return el().key("table-fk-breadcrumb-empty").a11y_hidden();
    };
    let k = kit(dark);
    let label = fk_origin_label(origin);
    let source = t("fkNav.breadcrumbFrom").replace("{source}", &label);

    let mut bar = row()
        .key("table-fk-breadcrumb")
        .w_full()
        .gap(8.0)
        .center_y()
        .px(12.0)
        .py(6.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(text(source).key("table-fk-breadcrumb-label").color(k.c.text_muted).truncate().min_w(0.0));
    if vm.truncated {
        bar = bar.child(
            text(t("fkNav.stackTruncated"))
                .key("table-fk-breadcrumb-truncated")
                .color(k.c.text_muted),
        );
    }
    bar = bar.child(row().grow(1.0).min_w(0.0));

    let mut back = rune_kit::Button::new(t("fkNav.back"))
        .variant(rune_kit::ButtonVariant::Ghost)
        .control_size(rune_kit::ControlSize::Small)
        .icon(&ICON_CHEVRON_LEFT)
        .disabled(!vm.can_go_back)
        .help(t("fkNav.backToSource").replace("{table}", &origin.source_table));
    if vm.can_go_back {
        back = back.on_press(emit(&on_action, TableFkAction::Back));
    }
    bar = bar.child(back.build(k).key("table-fk-back"));

    let mut forward = rune_kit::Button::new(t("fkNav.forward"))
        .variant(rune_kit::ButtonVariant::Ghost)
        .control_size(rune_kit::ControlSize::Small)
        .icon(&ICON_CHEVRON_RIGHT)
        .disabled(!vm.can_go_forward)
        .help(t("fkNav.forward"));
    if vm.can_go_forward {
        forward = forward.on_press(emit(&on_action, TableFkAction::Forward));
    }
    bar = bar.child(forward.build(k).key("table-fk-forward"));

    if vm.dismissible {
        let mut dismiss = rune_kit::IconButton::new(&ICON_X)
            .variant(rune_kit::IconButtonVariant::Ghost)
            .control_size(rune_kit::ControlSize::Mini)
            .label(t("common.close"));
        dismiss = dismiss.on_press(emit(&on_action, TableFkAction::Dismiss));
        bar = bar.child(dismiss.build(k).key("table-fk-dismiss"));
    }
    bar
}
