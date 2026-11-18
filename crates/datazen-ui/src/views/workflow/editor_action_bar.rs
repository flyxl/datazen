//! Port of `packages/frontend/src/windows/workflow/WorkflowEditorActionBar.tsx`.
//!
//! React renders one shared bottom bar as a *sibling* of the workflow edit
//! panel's tab contents (Visual + YAML), so both tabs reuse the same
//! Cancel (secondary) / Save (primary, possibly disabled) buttons: a
//! right-aligned flex row over a top border, `px-6 py-3` spacing, `gap-3`.
//!
//! Rune keeps the same contract with kit `Button`s: Cancel is the outlined
//! variant (React's `secondary`: border + transparent surface + fg ink) and
//! Save the contrast variant (React's `primary`: accent surface + on-accent
//! ink). React's `saveTestId` names the Save button for tests; the port
//! takes `save_key` for the same purpose — the workflows page pins the
//! long-standing `workflow-save` key there.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root bar | `workflow-editor-action-bar` |
//! | Cancel button | `workflow-editor-cancel` |
//! | Save button | `save_key` (the React `saveTestId` slot) |

use rune::{col, row, El};

use crate::i18n::t;
use crate::ui::kit::kit;
use rune_kit::{Button, ButtonVariant, ControlSize};

/// The bar's horizontal padding (`px-6` = 24pt).
const PAD_X: f32 = 24.0;
/// The bar's vertical padding (`py-3` = 12pt).
const PAD_Y: f32 = 12.0;
/// The gap between the two buttons (`gap-3` = 12pt).
const GAP: f32 = 12.0;

/// The shared bottom action bar for the workflow edit panel.
///
/// `on_save` / `on_cancel` mutate the host's state `S`; `save_disabled`
/// reproduces React's `disabled` prop (a disabled Save is neither focusable
/// nor pressable, and dims like the kit's contrast skin does).
pub(crate) fn workflow_editor_action_bar<S: 'static>(
    save_key: impl Into<String>,
    save_disabled: bool,
    on_save: impl Fn(&mut S) + 'static,
    on_cancel: impl Fn(&mut S) + 'static,
    dark: bool,
) -> El {
    let k = kit(dark);
    // React's md button (`h-9` = 36pt) is the kit's ExtraLarge control, the
    // size the ported dialog footers already use for the same React source.
    let cancel = Button::new(t("common.cancel").to_owned())
        .variant(ButtonVariant::Outline)
        .control_size(ControlSize::ExtraLarge)
        .on_press(move |state: &mut S| on_cancel(state))
        .build(k)
        .key("workflow-editor-cancel");
    let save = Button::new(t("common.save").to_owned())
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::ExtraLarge)
        .disabled(save_disabled)
        .on_press(move |state: &mut S| on_save(state))
        .build(k)
        .key(save_key.into());
    col()
        .key("workflow-editor-action-bar")
        .w_full()
        .shrink()
        // `border-t border-edge` — one top hairline, no layout room.
        .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
        // `bg-surface` — the bar's own background.
        .fill(k.c.bg_base)
        .px(PAD_X)
        .py(PAD_Y)
        .child(
            // `flex items-center justify-end gap-3`.
            row()
                .key("workflow-editor-action-bar-row")
                .w_full()
                .shrink()
                .gap(GAP)
                .center_y()
                .justify_end()
                .child(cancel)
                .child(save),
        )
}
