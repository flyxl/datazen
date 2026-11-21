//! `TableStatusBanners`, ported 1:1 from
//! `packages/frontend/src/windows/connection/TableStatusBanners.tsx` (DB-06).
//!
//! The status strip above the toolbar: an error banner (copyable, with Retry),
//! the "changes are still staged" hint shown only when both an error and staged
//! work exist, and the insert-degraded notice in its own live region. Every
//! data-testid from the E2E specs is kept as a Rune key. Retry goes through an
//! injected host port — no fake IPC.
//!
//! ## Removal stage
//!
//! No production host mounts this strip yet; the table host will feed the
//! snapshot and handle Retry. Per-item allow annotations carry that removal
//! stage until then.

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;

/// Host-owned projection of the banner props.
#[allow(dead_code)] // VM surface; owned by the table host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TableStatusBannersVm {
    pub(crate) error: Option<String>,
    pub(crate) has_work: bool,
    pub(crate) insert_notice: Option<String>,
}

/// The only host intent in the React source.
#[allow(dead_code)] // host port; removed with the table wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableStatusBannersAction {
    Retry,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, TableStatusBannersAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: TableStatusBannersAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the three conditional banners into one column (React's fragment).
#[allow(dead_code)] // consumer: the table host (not landed).
pub(crate) fn build<S: 'static>(
    vm: &TableStatusBannersVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut column = col().key("table-status-banners").w_full().min_w(0.0);
    let has_error = vm.error.as_deref().is_some_and(|error| !error.is_empty());

    if has_error {
        let message = vm.error.clone().unwrap_or_default();
        let retry = Button::new(t("common.retry"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .on_press(emit(&on_action, TableStatusBannersAction::Retry))
            .build(k)
            .key("table-status-retry");
        column = column.child(
            row()
                .key("table-status-error")
                .w_full()
                .gap(12.0)
                .center_y()
                .flex_wrap()
                .px(12.0)
                .py(8.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.danger.border)
                .child(
                    CopyableError::new("table-status-error-copy", message)
                        .copy_button(true)
                        .build::<S>(k)
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(retry),
        );
        if vm.has_work {
            column = column.child(
                row()
                    .key("table-status-pending-hint")
                    .w_full()
                    .px(12.0)
                    .py(6.0)
                    .border_sides([0.0, 0.0, 1.0, 0.0], k.c.warning.border)
                    .child(text(t("tableData.errorPendingHint")).color(k.c.warning.fg)),
            );
        }
    }

    if let Some(notice) = vm.insert_notice.as_deref() {
        column = column.child(
            row()
                .key("table-insert-degraded")
                .role("status")
                .w_full()
                .px(12.0)
                .py(6.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .child(text(notice.to_owned()).color(k.c.text_muted)),
        );
    }
    column
}

