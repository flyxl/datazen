//! Port of `windows/connection/TableLoadErrorState.tsx` (DB-03).
//!
//! The full-surface state of a table whose first load failed and has no columns
//! to keep on screen: a centred copyable error and a retry button. The retry
//! is an injected host action — no fake IPC. Unlike `TableFkErrorBanner` this
//! replaces the whole surface rather than sitting above a usable grid.
//!
//! ## Keys
//!
//! `table-load-error` is the Rune observation handle (React has no test id);
//! `table-load-error-text` is the copyable message and `table-load-error-retry`
//! is the retry control.
//!
//! ## Local stand-ins (removal stage)
//!
//! No production host mounts this state yet; the table host owns the error and
//! the translated retry label. Every reusable item carries a per-item
//! `#[allow(dead_code)]` with removal stage "drop when the table host wires
//! this state".

use std::rc::Rc;

use rune::{col, row, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;

pub const ROOT_KEY: &str = "table-load-error";
pub const TEXT_KEY: &str = "table-load-error-text";
pub const RETRY_KEY: &str = "table-load-error-retry";

/// React props: the error, its already-translated retry label and the callback.
#[allow(dead_code)] // VM mirror; remove when the host wires the state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableLoadErrorVm {
    pub error: String,
    pub retry_label: String,
}

/// The sole React callback (`onRetry`).
#[allow(dead_code)] // host port; remove when the host wires the state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadErrorAction {
    Retry,
}

#[allow(dead_code)] // host port; remove when the host wires the state.
pub type LoadErrorActionFn<S> = Rc<dyn Fn(&mut S, LoadErrorAction) + 'static>;

/// Build the error state into `layer`.
#[allow(dead_code)] // view entry; remove when the host wires the state.
pub fn build<S: 'static>(
    vm: &TableLoadErrorVm,
    on_action: &Option<LoadErrorActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut retry = Button::new(vm.retry_label.clone())
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .build(k)
        .key(RETRY_KEY)
        .label(vm.retry_label.clone());
    if let Some(callback) = on_action.clone() {
        retry = retry.on_press(move |s: &mut S| callback(s, LoadErrorAction::Retry));
    }
    layer.push(
        col()
            .key(ROOT_KEY)
            .w_full()
            .h_full()
            .center()
            .child(
                col()
                    .key("table-load-error-inner")
                    .w_full()
                    .max_w(512.0)
                    .center()
                    .child(
                        CopyableError::new(TEXT_KEY, vm.error.clone())
                            .copy_button(true)
                            .build::<S>(k),
                    )
                    .child(row().key("table-load-error-retry-row").mt(8.0).child(retry)),
            ),
    );
}
