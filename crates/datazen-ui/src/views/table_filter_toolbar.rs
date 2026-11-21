//! Port of `packages/frontend/src/windows/connection/TableFilterToolbar.tsx`.
//!
//! React is the filter half of the panel toolbar: a manual-filter toggle, the
//! saved-views menu, an NL filter input and a quick-filter box with an error
//! strip. Pressing the toggle opens/closes the manual editor and seeds a default
//! draft filter when none exists; the quick filter parses its text and applies
//! the resulting flat condition list on Enter/blur. The Rune port keeps every
//! test id, i18n key, enabled/disabled state and action: the toggle, add-filter,
//! clear and quick-filter intents are [`TableFilterAction`]s dispatched to an
//! injected host port — no fake IPC, store, parser or timer lives here. The
//! saved-views / NL filter / leading / trailing nodes arrive as keyed slots
//! (they are separate host-owned components).
//!
//! ## Keys
//!
//! `table-filter-toggle` and `table-quick-filter` are React's E2E test ids,
//! kept verbatim. The `table-filter-*` keys name Rune-side handles for the
//! four slots and the error strip.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this toolbar yet; the table-panel host owns the
//! VM, the expression parser and the filter state. Until then every reusable
//! item carries a per-item `#[allow(dead_code)]` with this stage.

use std::rc::Rc;

use rune::{col, icon, row, text, El, Key, KeyEvent, TextEdit, Tx};
use rune_kit::{ButtonVariant, Kit};

use crate::i18n::t;
use crate::icons::ICON_FILTER;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

const ROOT: &str = "table-filter-toolbar";
pub(crate) const TOGGLE_KEY: &str = "table-filter-toggle";
pub(crate) const QUICK_KEY: &str = "table-quick-filter";
pub(crate) const ERROR_KEY: &str = "table-filter-error";
pub(crate) const CHILDREN_KEY: &str = "table-filter-children";
pub(crate) const NAMED_VIEW_KEY: &str = "table-filter-named-view";
pub(crate) const NL_INPUT_KEY: &str = "table-filter-nl-input";
pub(crate) const TRAILING_KEY: &str = "table-filter-trailing";

/// Host-owned projection of the props + the two local input states.
#[allow(dead_code)] // VM surface; owned by the table-panel host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TableFilterToolbarVm {
    pub loading: bool,
    pub pending_busy: bool,
    pub filter_panel_open: bool,
    pub applied_filter_count: usize,
    pub draft_filter_count: usize,
    /// The host's error text for an invalid quick-filter expression.
    pub quick_filter_error: String,
    /// The first column name, used to seed a default draft filter.
    pub first_column: String,
    /// Whether the named-views backend is available (absent ⇒ entry hidden).
    pub has_named_view: bool,
}

impl TableFilterToolbarVm {
    /// React `filterActive`: the toggle lights up for an open panel or any
    /// applied/draft filter.
    #[allow(dead_code)] // pure helper; consumed by the toggle and tests.
    pub(crate) fn filter_active(&self) -> bool {
        self.filter_panel_open || self.applied_filter_count > 0 || self.draft_filter_count > 0
    }

    /// React `openManualFilter`: opening seeds a default filter only when no
    /// draft exists; closing never seeds.
    #[allow(dead_code)] // pure helper; consumed by the toggle action and tests.
    pub(crate) fn should_seed_default_filter(&self) -> bool {
        !self.filter_panel_open && self.draft_filter_count == 0
    }

    /// React `<input disabled={loading || pendingBusy}>`.
    #[allow(dead_code)] // pure helper; consumed by the quick field and tests.
    pub(crate) fn quick_disabled(&self) -> bool {
        self.loading || self.pending_busy
    }
}

/// Every host intent the toolbar can emit.
#[allow(dead_code)] // host port; remove when the table-panel host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableFilterAction {
    ToggleFilterPanel { open: bool },
    AddDefaultFilter { column: String },
    ClearFilters,
    ApplyQuickFilter { text: String },
    /// React's onChange clears the error while the user corrects the text.
    ClearQuickFilterError,
}

type ActionFn<S> = Rc<dyn Fn(&mut S, TableFilterAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: TableFilterAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the toolbar. `quick_filter` is a host-bound `TextEdit`; the named-view,
/// NL-filter, leading and trailing nodes are optional keyed slots supplied by
/// the host.
#[allow(dead_code)] // view entry; remove when the table-panel host lands.
pub(crate) fn build<S: 'static>(
    vm: &TableFilterToolbarVm,
    quick_filter: &TextEdit,
    quick_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
    leading: Option<El>,
    trailing: Option<El>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut root = col().key(ROOT).w_full().min_w(0.0).gap(0.0);

    let mut bar = row()
        .key("table-filter-bar")
        .w_full()
        .min_w(0.0)
        .gap(2.0)
        .center_y()
        .px(8.0)
        .py(2.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);
    if let Some(children) = leading {
        bar = bar.child(row().key(CHILDREN_KEY).gap(2.0).center_y().child(children));
    }

    // Manual-filter toggle. React disables it while a page load is in flight;
    // `aria-pressed` becomes the accent colour and a focus ring here.
    let mut toggle = row()
        .key(TOGGLE_KEY)
        .w(28.0)
        .h(28.0)
        .center()
        .radius(4.0)
        .focusable(!vm.loading)
        .help(t("filter.filter"))
        .label(t("filter.filter"))
        .fill(if vm.filter_panel_open { k.c.layer_02 } else { k.c.surface_base })
        .child(
            icon(&ICON_FILTER, 14.0)
                .color(if vm.filter_active() { k.c.text_accent } else { k.c.text_muted }),
        );
    if !vm.loading {
        let open = !vm.filter_panel_open;
        let seed = vm.should_seed_default_filter();
        let column = vm.first_column.clone();
        let toggle_action = on_action.clone();
        toggle = toggle.on_press(move |s: &mut S| {
            if let Some(callback) = &toggle_action {
                callback(s, TableFilterAction::ToggleFilterPanel { open });
                if seed {
                    callback(s, TableFilterAction::AddDefaultFilter { column: column.clone() });
                }
            }
        });
    }
    bar = bar.child(toggle);

    if vm.has_named_view {
        bar = bar.child(row().key(NAMED_VIEW_KEY).h(28.0).center_y().child(
            text(t("filter.filter")).font(k.ty.text_xsmall).color(k.c.text_muted),
        ));
    }
    bar = bar.child(row().key(NL_INPUT_KEY).h(28.0).center_y().child(
        icon(&ICON_FILTER, 12.0).color(k.c.text_muted),
    ));

    // Quick filter: bound to the host TextEdit; Enter applies, an empty input
    // clears, and typing clears the host-owned error on the next frame.
    let mut field = TextField::new(QUICK_KEY, quick_filter)
        .height(28.0)
        .disabled(vm.quick_disabled())
        .placeholder(t("filter.quickPlaceholder"));
    if let Some(accessor) = quick_edit {
        let edit_action = on_action.clone();
        // React's onChange only clears when an error is present; a field with
        // no error emits nothing (keeping the semantic action stream clean).
        let has_error = !vm.quick_filter_error.is_empty();
        field = field.on_edit(rune::edit_access(move |s: &mut S| {
            if has_error {
                if let Some(callback) = &edit_action {
                    callback(s, TableFilterAction::ClearQuickFilterError);
                }
            }
            accessor(s)
        }));
    }
    let submit_action = if quick_filter.text.trim().is_empty() {
        TableFilterAction::ClearFilters
    } else {
        TableFilterAction::ApplyQuickFilter { text: quick_filter.text.clone() }
    };
    let submit_cb = on_action.clone();
    field = field.on_submit(move |s: &mut S, _tx: &mut Tx| {
        if let Some(callback) = &submit_cb {
            callback(s, submit_action.clone());
        }
    });
    // React's onBlur applies a non-empty quick filter. Rune delivers Tab to the
    // focused text field's on_key before moving focus, so mirror that behaviour
    // without consuming the key (return false lets focus travel on).
    let blur_action = on_action.clone();
    let blur_text = quick_filter.text.clone();
    field = field.on_key(move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| {
        let plain_tab = event.key == Key::Tab
            && !event.mods.cmd
            && !event.mods.alt
            && !event.mods.ctrl
            && !event.mods.shift;
        if plain_tab && !blur_text.trim().is_empty() {
            if let Some(callback) = &blur_action {
                callback(s, TableFilterAction::ApplyQuickFilter { text: blur_text.clone() });
            }
        }
        false
    });
    bar = bar.child(row().key("table-filter-quick-wrap").w_full().min_w(0.0).grow(1.0).child(field.build(dark)));

    if let Some(trailing) = trailing {
        bar = bar.child(row().key(TRAILING_KEY).gap(2.0).center_y().child(trailing));
    }
    root = root.child(bar);

    if !vm.quick_filter_error.is_empty() {
        root = root.child(error_strip(&vm.quick_filter_error, k, dark));
    }
    root
}

fn error_strip(error: &str, _k: &Kit, dark: bool) -> El {
    let k = kit(dark);
    row()
        .key(ERROR_KEY)
        .role("alert")
        .w_full()
        .min_w(0.0)
        .px(8.0)
        .py(4.0)
        .fill(k.c.layer_02)
        .child(text(error.to_owned()).font(k.ty.text_xsmall).color(k.c.text_accent))
}
