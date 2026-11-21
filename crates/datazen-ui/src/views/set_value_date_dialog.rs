//! `SetValueDateDialog`, ported 1:1 from
//! `packages/frontend/src/windows/connection/SetValueDateDialog.tsx` (DBR-05).
//!
//! The dialog only *chooses* a value; applying hands the canonical temporal
//! string to the host through [`SetValueDateAction::Apply`]. No command runs
//! here, and a cleared picker applies SQL NULL exactly as React does.
//!
//! ## Removal stage
//!
//! No production host mounts this dialog yet; the connection host will own the
//! draft and stage the chosen value on the same path as an in-place edit.
//! Per-item allow annotations carry that removal stage until then.

use std::rc::Rc;

use rune::{row, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::ui::temporal_input::{TemporalKind, TemporalValueInput};

const ROOT: &str = "set-value-date-dialog";

/// React's `kind: 'date' | 'datetime'` union.
#[allow(dead_code)] // VM mirror; consumed by the picker and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PickerKind {
    #[default]
    Date,
    DateTime,
}

impl PickerKind {
    /// React passes the literal kind to `TemporalValueInput`.
    #[allow(dead_code)] // pure mapping; consumed by the picker and tests.
    pub(crate) fn from_str(value: &str) -> Self {
        if value.eq_ignore_ascii_case("datetime") {
            Self::DateTime
        } else {
            Self::Date
        }
    }

    fn temporal(self) -> TemporalKind {
        match self {
            Self::Date => TemporalKind::Date,
            Self::DateTime => TemporalKind::DateTime,
        }
    }
}

/// Host-owned projection of the dialog props plus its draft.
#[allow(dead_code)] // VM surface; owned by the connection host.
#[derive(Clone, Debug, Default)]
pub(crate) struct SetValueDateDialogVm {
    pub(crate) open: bool,
    pub(crate) column_name: String,
    pub(crate) column_type: Option<String>,
    pub(crate) kind: PickerKind,
    pub(crate) draft: TextEdit,
}

impl SetValueDateDialogVm {
    /// React's description: `pickDate` plus ` · columnType` when present.
    #[allow(dead_code)] // pure helper; consumed by the dialog and tests.
    pub(crate) fn description(&self) -> String {
        match self.column_type.as_deref() {
            Some(column_type) if !column_type.is_empty() => {
                format!("{} · {}", t("dataTable.pickDate"), column_type)
            }
            _ => t("dataTable.pickDate").to_owned(),
        }
    }
}

/// What the dialog asks the host to do.
#[allow(dead_code)] // host port; removed with the connection host wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SetValueDateAction {
    Apply(String),
    Close,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, SetValueDateAction) + 'static>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: SetValueDateAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the dialog into `layer`; a closed React `Dialog` mounts nothing.
#[allow(dead_code)] // consumer: the connection host (not landed).
pub(crate) fn build<S: 'static>(
    vm: &SetValueDateDialogVm,
    edit: Option<EditFn<S>>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    if !vm.open {
        return;
    }
    let k = kit(dark);
    let mut picker = TemporalValueInput::new("set-value-date-picker", &vm.draft, vm.kind.temporal());
    if let Some(edit) = edit {
        let edit_commit = edit.clone();
        picker = picker.on_actions(
            move |s: &mut S| edit(s),
            move |s: &mut S, value: &str| {
                edit_commit(s).replace_text(value.to_owned());
            },
            move |_s: &mut S| {},
        );
    }
    let picker = picker.build(dark);

    let cancel = Button::new(t("common.cancel"))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .on_press(emit(&on_action, SetValueDateAction::Close))
        .build(k)
        .key("set-value-date-cancel");
    let apply_value = vm.draft.text.clone();
    let apply_callback = on_action.clone();
    let apply = Button::new(t("common.confirm"))
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Regular)
        .on_press(move |s: &mut S| {
            if let Some(callback) = &apply_callback {
                callback(s, SetValueDateAction::Apply(apply_value.clone()));
                // React's confirm handler calls onApply(draft) then onClose().
                callback(s, SetValueDateAction::Close);
            }
        })
        .build(k)
        .key("set-value-date-apply");

    Dialog::new(ROOT, vm.column_name.clone())
        .open(true)
        .description(vm.description())
        .child(row().key("set-value-date-body").w_full().child(picker))
        .footer(cancel)
        .footer(apply)
        .on_close::<S>(emit(&on_action, SetValueDateAction::Close))
        .build(k, layer);
}
