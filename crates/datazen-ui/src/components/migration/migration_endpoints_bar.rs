//! Source/target endpoint bar for the P5 migration tools, ported 1:1 from
//! `packages/frontend/src/components/migration/MigrationEndpointsBar.tsx`.
//!
//! A pure, controlled endpoint selector: source column, optional swap button,
//! target column, optional compare/refresh button. Values and actions all come
//! from the host via an [`EndpointsBarVm`] snapshot plus callbacks over `S`.
//! No production consumer yet, so reusable items carry per-item `#[allow(dead_code)]`.
//!
//! # Key convention
//!
//! React `data-testid` becomes the element key. With prefix `data-sync`: root
//! `data-sync`; columns `data-sync-source` / `-target`; labels `-source-label`;
//! selects `data-sync-source-connection-field-trigger` (kit derives `-trigger`,
//! `-content`, `-scrim` from the logical `-field` key); wrappers `-source-database`,
//! `-source-schema`, with `-field-trigger` selects inside; conditional blocks
//! `-target-read-only-hint` and `-source-session-error`; buttons `-swap` and
//! `-compare` (leading `-compare-icon` / busy `-compare-spinner`).
//!
//! # Documented deviations from React
//!
//! - **Icons** (`crate::icons` is the only glyph source; not edited): no
//!   `ArrowLeftRight` exists, so swap composes the opposing `ICON_ARROW_DOWN` +
//!   `ICON_ARROW_UP`; `RefreshCcw` is represented by `ICON_REFRESH` (`refresh-cw`).
//!   Neither is a visual 1:1.
//! - **Disabled options**: kit `Select` has no per-option disabled state, so a
//!   React `disabled: true` option is left out rather than offered and ignored.
//! - **Open state** lives in [`EndpointsBarVm::open`] (one slot), routed to `S`
//!   through `on_toggle` / `on_close`; React's `Select` owns it internally.
//! - **`actionNote` / `footerNote`** are React nodes; here plain wrapped strings.
//! - Rune has no `data-*` API, so React `data-testid` values are the keys above.

use std::rc::Rc;

use rune::{col, icon, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_ARROW_DOWN, ICON_ARROW_UP, ICON_REFRESH};
use crate::ui::select::Select;
use crate::ui::spinner::{Spinner, SpinnerSize};

/// React `MigrationI18nPrefix` ('sync' | 'schemaDiff' | 'transfer').
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum MigrationI18nPrefix {
    #[default]
    Sync,
    SchemaDiff,
    Transfer,
}

/// React `layout` prop: top bar vs. Transfer wizard card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum EndpointsLayout {
    #[default]
    Bar,
    Grid,
}

/// React's `resolveDefaultPrefix`: explicit prefix wins, then the test id.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) fn resolve_prefix(
    test_id_prefix: &str,
    explicit: Option<MigrationI18nPrefix>,
) -> MigrationI18nPrefix {
    if let Some(prefix) = explicit {
        return prefix;
    }
    if test_id_prefix.starts_with("schema-diff") {
        MigrationI18nPrefix::SchemaDiff
    } else if test_id_prefix.starts_with("data-transfer") {
        MigrationI18nPrefix::Transfer
    } else {
        MigrationI18nPrefix::Sync
    }
}

/// One `{ value, label, disabled }` React `MigrationSelectOption`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct EndpointOption {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl EndpointOption {
    pub(crate) fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// Which of the six controlled selects is currently open. React lets each
/// select hold its own flag; a bar keeps one open listbox at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum SelectSlot {
    SourceConnection,
    SourceDatabase,
    SourceSchema,
    TargetConnection,
    TargetDatabase,
    TargetSchema,
}

/// The controlled state of one endpoint column.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct EndpointVm {
    pub connection_id: String,
    pub options: Vec<EndpointOption>,
    pub database: String,
    pub databases: Vec<String>,
    pub schema: String,
    pub schemas: Vec<String>,
    pub session_error: Option<String>,
    /// Mirrors React's `readOnlyHint` (only the target column sets it).
    pub read_only_hint: bool,
}

/// The whole controlled bar. Fields mirror `MigrationEndpointsBarProps`; every
/// optional string is an i18n-key override resolved by [`resolve_keys`].
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct EndpointsBarVm {
    pub test_id_prefix: String,
    pub i18n_prefix: Option<MigrationI18nPrefix>,
    pub layout: EndpointsLayout,
    pub source: EndpointVm,
    pub target: EndpointVm,
    pub busy: bool,
    pub compare_disabled: bool,
    pub include_empty_connection_option: bool,
    pub hide_database_until_connected: bool,
    /// `None` follows the layout (`bar` shows both, `grid` shows neither).
    pub show_swap: Option<bool>,
    pub show_compare: Option<bool>,
    pub compare_label: String,
    pub action_note: Option<String>,
    pub footer_note: Option<String>,
    pub open: Option<SelectSlot>,
    pub source_label_key: Option<String>,
    pub target_label_key: Option<String>,
    pub source_placeholder_key: Option<String>,
    pub target_placeholder_key: Option<String>,
    pub database_label_key: Option<String>,
    pub select_database_placeholder_key: Option<String>,
    pub schema_label_key: Option<String>,
    pub select_schema_placeholder_key: Option<String>,
    pub swap_tooltip_key: Option<String>,
    pub read_only_hint_key: Option<String>,
    pub empty_connection_label_key: Option<String>,
}

impl Default for EndpointsBarVm {
    fn default() -> Self {
        Self {
            test_id_prefix: "data-sync".to_owned(),
            i18n_prefix: None,
            layout: EndpointsLayout::Bar,
            source: EndpointVm::default(),
            target: EndpointVm::default(),
            busy: false,
            compare_disabled: false,
            include_empty_connection_option: false,
            hide_database_until_connected: false,
            show_swap: None,
            show_compare: None,
            compare_label: String::new(),
            action_note: None,
            footer_note: None,
            open: None,
            source_label_key: None,
            target_label_key: None,
            source_placeholder_key: None,
            target_placeholder_key: None,
            database_label_key: None,
            select_database_placeholder_key: None,
            schema_label_key: None,
            select_schema_placeholder_key: None,
            swap_tooltip_key: None,
            read_only_hint_key: None,
            empty_connection_label_key: None,
        }
    }
}

/// The exact i18n keys React resolves, exposed for parity tests.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct ResolvedKeys {
    pub i18n_prefix: MigrationI18nPrefix,
    pub source_label: String,
    pub target_label: String,
    pub source_placeholder: String,
    pub target_placeholder: String,
    pub database_label: String,
    pub select_database_placeholder: String,
    pub schema_label: String,
    pub select_schema_placeholder: String,
    pub swap_tooltip: String,
    pub read_only_hint: String,
    pub empty_connection_label: String,
}

/// Resolve the React `resolved*Key` chain, in the same order as the source.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) fn resolve_keys(vm: &EndpointsBarVm) -> ResolvedKeys {
    let prefix = resolve_prefix(&vm.test_id_prefix, vm.i18n_prefix);
    let transfer = prefix == MigrationI18nPrefix::Transfer;
    let empty = vm
        .empty_connection_label_key
        .as_deref()
        .unwrap_or("common.selectConnection");
    ResolvedKeys {
        i18n_prefix: prefix,
        source_label: vm
            .source_label_key
            .as_deref()
            .unwrap_or(if transfer { "transfer.source" } else { "sync.source" })
            .to_owned(),
        target_label: vm
            .target_label_key
            .as_deref()
            .unwrap_or(if transfer { "transfer.target" } else { "sync.target" })
            .to_owned(),
        source_placeholder: vm
            .source_placeholder_key
            .as_deref()
            .unwrap_or({
                if vm.include_empty_connection_option {
                    empty
                } else {
                    "sync.selectSource"
                }
            })
            .to_owned(),
        target_placeholder: vm
            .target_placeholder_key
            .as_deref()
            .unwrap_or({
                if vm.include_empty_connection_option {
                    empty
                } else {
                    "sync.selectTarget"
                }
            })
            .to_owned(),
        database_label: vm
            .database_label_key
            .as_deref()
            .unwrap_or("common.database")
            .to_owned(),
        select_database_placeholder: vm
            .select_database_placeholder_key
            .as_deref()
            .unwrap_or("common.selectDatabase")
            .to_owned(),
        schema_label: vm
            .schema_label_key
            .as_deref()
            .unwrap_or("sync.schema")
            .to_owned(),
        select_schema_placeholder: vm
            .select_schema_placeholder_key
            .as_deref()
            .unwrap_or("sync.selectSchema")
            .to_owned(),
        swap_tooltip: vm
            .swap_tooltip_key
            .as_deref()
            .unwrap_or("sync.swapEndpoints")
            .to_owned(),
        read_only_hint: vm
            .read_only_hint_key
            .as_deref()
            .unwrap_or("transfer.readOnlyHint")
            .to_owned(),
        empty_connection_label: empty.to_owned(),
    }
}

type Change<S> = Rc<dyn Fn(&mut S, String)>;
type Action<S> = Rc<dyn Fn(&mut S)>;
type Toggle<S> = Rc<dyn Fn(&mut S, SelectSlot)>;

/// Callback bundle. `Rc` lets one handler be folded into several kit closures
/// (a select needs a toggle, a close and a choose closure, all `'static`).
struct BarCallbacks<S: 'static> {
    source_change: Option<Change<S>>,
    target_change: Option<Change<S>>,
    source_db_change: Option<Change<S>>,
    target_db_change: Option<Change<S>>,
    source_schema_change: Option<Change<S>>,
    target_schema_change: Option<Change<S>>,
    swap: Option<Action<S>>,
    refresh: Option<Action<S>>,
    toggle: Option<Toggle<S>>,
    close: Option<Action<S>>,
}

impl<S: 'static> Default for BarCallbacks<S> {
    fn default() -> Self {
        Self {
            source_change: None,
            target_change: None,
            source_db_change: None,
            target_db_change: None,
            source_schema_change: None,
            target_schema_change: None,
            swap: None,
            refresh: None,
            toggle: None,
            close: None,
        }
    }
}

impl<S: 'static> BarCallbacks<S> {
    fn change_for(&self, slot: SelectSlot) -> Option<&Change<S>> {
        match slot {
            SelectSlot::SourceConnection => self.source_change.as_ref(),
            SelectSlot::SourceDatabase => self.source_db_change.as_ref(),
            SelectSlot::SourceSchema => self.source_schema_change.as_ref(),
            SelectSlot::TargetConnection => self.target_change.as_ref(),
            SelectSlot::TargetDatabase => self.target_db_change.as_ref(),
            SelectSlot::TargetSchema => self.target_schema_change.as_ref(),
        }
    }
}

/// Builder for the endpoint bar. `S` is the host state that owns every value.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct MigrationEndpointsBar<S: 'static> {
    vm: EndpointsBarVm,
    cb: BarCallbacks<S>,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl<S: 'static> MigrationEndpointsBar<S> {
    pub(crate) fn new(vm: EndpointsBarVm) -> Self {
        Self {
            vm,
            cb: BarCallbacks::default(),
        }
    }

    /// React `onSourceChange`: receives the chosen connection value.
    pub(crate) fn on_source_change(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.cb.source_change = Some(Rc::new(f));
        self
    }

    /// React `onTargetChange`.
    pub(crate) fn on_target_change(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.cb.target_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_source_database_change(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.cb.source_db_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_target_database_change(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.cb.target_db_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_source_schema_change(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.cb.source_schema_change = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_target_schema_change(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.cb.target_schema_change = Some(Rc::new(f));
        self
    }

    /// React `onSwap`.
    pub(crate) fn on_swap(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.cb.swap = Some(Rc::new(f));
        self
    }

    /// React `onCompare` — the refresh/compare primary action.
    pub(crate) fn on_refresh(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.cb.refresh = Some(Rc::new(f));
        self
    }

    /// Route a select's open/close to host state. Without it the selects build
    /// but never open (a documented base-only limitation).
    pub(crate) fn on_toggle(mut self, f: impl Fn(&mut S, SelectSlot) + 'static) -> Self {
        self.cb.toggle = Some(Rc::new(f));
        self
    }

    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.cb.close = Some(Rc::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k: &'static Kit = crate::ui::kit::kit(dark);
        let keys = resolve_keys(&self.vm);
        let prefix = self.vm.test_id_prefix.clone();
        let is_grid = self.vm.layout == EndpointsLayout::Grid;
        let show_swap = self.vm.show_swap.unwrap_or(!is_grid);
        let show_compare = self.vm.show_compare.unwrap_or(!is_grid);
        let mut layer = Vec::new();

        let source = build_endpoint(
            k,
            &self.vm,
            &keys,
            &self.vm.source,
            &format!("{prefix}-source"),
            &keys.source_label,
            &keys.source_placeholder,
            (
                SelectSlot::SourceConnection,
                SelectSlot::SourceDatabase,
                SelectSlot::SourceSchema,
            ),
            &self.cb,
            &mut layer,
        );
        let target = build_endpoint(
            k,
            &self.vm,
            &keys,
            &self.vm.target,
            &format!("{prefix}-target"),
            &keys.target_label,
            &keys.target_placeholder,
            (
                SelectSlot::TargetConnection,
                SelectSlot::TargetDatabase,
                SelectSlot::TargetSchema,
            ),
            &self.cb,
            &mut layer,
        );

        let root = if is_grid {
            let grid = row()
                .key(format!("{prefix}-grid"))
                .items_start()
                .gap(24.0)
                .min_w(0.0)
                .child(source)
                .child(target);
            let mut card = col()
                .key(prefix.clone())
                .min_w(0.0)
                .gap(24.0)
                .radius(8.0)
                .border(1.0, k.c.border_base)
                .fill(k.c.layer_02)
                .px(24.0)
                .py(24.0)
                .child(grid);
            if let Some(note) = self
                .vm
                .footer_note
                .as_deref()
                .or(self.vm.action_note.as_deref())
            {
                card = card.child(
                    text(note.to_owned())
                        .key(format!("{prefix}-footer-note"))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted)
                        .wrap(),
                );
            }
            card
        } else {
            let mut bar = row()
                .key(prefix.clone())
                .items_center()
                .gap(12.0)
                .min_w(0.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .px(24.0)
                .py(16.0);
            bar = bar.child(source);
            if show_swap {
                bar = bar.child(build_swap_button(k, &self.vm, &keys, &self.cb).mt(20.0));
            }
            bar = bar.child(target);
            if show_compare {
                let mut compare_col = col()
                    .key(format!("{prefix}-compare-col"))
                    .mt(20.0)
                    .shrink()
                    .min_w(0.0)
                    .items_end()
                    .gap(4.0);
                if let Some(note) = self.vm.action_note.as_deref() {
                    compare_col = compare_col.child(
                        text(note.to_owned())
                            .key(format!("{prefix}-action-note"))
                            .font(k.ty.text_small)
                            .color(k.c.text_muted)
                            .wrap(),
                    );
                }
                compare_col =
                    compare_col.child(build_compare_button(k, &self.vm, &self.cb, dark));
                bar = bar.child(compare_col);
            }
            bar
        };

        rune_kit::root(root, layer)
    }
}

/// React's `emptyConn` prepended to each connection option list.
fn effective_options(
    vm: &EndpointsBarVm,
    options: &[EndpointOption],
    empty_label: &str,
) -> Vec<EndpointOption> {
    let mut out = Vec::with_capacity(options.len() + 1);
    if vm.include_empty_connection_option {
        out.push(EndpointOption::new(String::new(), empty_label.to_owned()));
    }
    out.extend(options.iter().filter(|o| !o.disabled).cloned());
    out
}

/// Build one controlled `ui/select.rs` field for `slot`.
fn build_select<S: 'static>(
    k: &'static Kit,
    key: String,
    options: &[EndpointOption],
    current: &str,
    placeholder: &str,
    label: &str,
    open: bool,
    slot: SelectSlot,
    cb: &BarCallbacks<S>,
    layer: &mut Vec<El>,
) -> El {
    let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
    let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
    // kit renders `current` verbatim and marks a row selected only when it equals
    // that row's LABEL. React stores an opaque id and shows the resolved label, so
    // translate value -> label here; an unknown/empty value falls back to the
    // placeholder (`current(None)`), matching React's `selectedOption?.label`.
    let current_label = options
        .iter()
        .find(|o| o.value == current)
        .map(|o| o.label.as_str());
    let mut select = Select::new(key, &labels)
        .values(&values)
        .current(current_label)
        .placeholder(placeholder)
        .label(label)
        .open(open);
    // kit needs both hooks: no toggle is a dead control, no close leaves the
    // keyboard dismissal layer unmounted (see ui/select.rs module doc).
    let toggle = cb
        .toggle
        .clone()
        .unwrap_or_else(|| Rc::new(|_: &mut S, _: SelectSlot| {}));
    let close = cb.close.clone().unwrap_or_else(|| Rc::new(|_: &mut S| {}));
    select = select.on_toggle(move |s: &mut S| toggle(s, slot));
    let close_on_dismiss = close.clone();
    select = select.on_close(move |s: &mut S| close_on_dismiss(s));
    if let Some(change) = cb.change_for(slot).cloned() {
        select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
            change(s, value.to_owned())
        });
    }
    select.build(k, layer)
}

/// React `EndpointColumn`: label, connection select, optional database block,
/// optional read-only hint, optional schema block, optional session error.
fn build_endpoint<S: 'static>(
    k: &'static Kit,
    vm: &EndpointsBarVm,
    keys: &ResolvedKeys,
    ep: &EndpointVm,
    test_id: &str,
    label_key: &str,
    placeholder_key: &str,
    slots: (SelectSlot, SelectSlot, SelectSlot),
    cb: &BarCallbacks<S>,
    layer: &mut Vec<El>,
) -> El {
    let mut column = col().key(test_id.to_owned()).min_w(0.0).grow(1.0);
    column = column.child(
        text(t(label_key))
            .key(format!("{test_id}-label"))
            .font(k.ty.ui)
            .color(k.c.text_muted)
            .mb(4.0),
    );

    let conn_options = effective_options(vm, &ep.options, &keys.empty_connection_label);
    column = column.child(build_select(
        k,
        format!("{test_id}-connection-field"),
        &conn_options,
        &ep.connection_id,
        placeholder_key,
        label_key,
        vm.open == Some(slots.0),
        slots.0,
        cb,
        layer,
    ));

    let show_database = !vm.hide_database_until_connected || !ep.connection_id.is_empty();
    if show_database {
        let mut database = col().key(format!("{test_id}-database")).mt(8.0).min_w(0.0);
        if vm.layout != EndpointsLayout::Grid {
            database = database.child(
                text(t(&keys.database_label))
                    .key(format!("{test_id}-database-label"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted)
                    .mb(4.0),
            );
        }
        let db_options: Vec<EndpointOption> = ep
            .databases
            .iter()
            .map(|db| EndpointOption::new(db.clone(), db.clone()))
            .collect();
        database = database.child(build_select(
            k,
            format!("{test_id}-database-field"),
            &db_options,
            &ep.database,
            &keys.select_database_placeholder,
            &keys.database_label,
            vm.open == Some(slots.1),
            slots.1,
            cb,
            layer,
        ));
        column = column.child(database);
    }

    if ep.read_only_hint {
        column = column.child(
            text(t(&keys.read_only_hint))
                .key(format!("{test_id}-read-only-hint"))
                .font(k.ty.text_small)
                .color(k.c.warning.fg)
                .mt(8.0)
                .px(8.0)
                .py(4.0)
                .radius(4.0)
                .fill(k.c.warning.bg)
                .border(1.0, k.c.warning.border)
                .wrap(),
        );
    }

    let show_schema = !ep.schemas.is_empty() && cb.change_for(slots.2).is_some();
    if show_schema {
        let mut schema = col().key(format!("{test_id}-schema")).mt(8.0).min_w(0.0);
        schema = schema.child(
            text(t(&keys.schema_label))
                .key(format!("{test_id}-schema-label"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .mb(4.0),
        );
        let schema_options: Vec<EndpointOption> = ep
            .schemas
            .iter()
            .map(|s| EndpointOption::new(s.clone(), s.clone()))
            .collect();
        schema = schema.child(build_select(
            k,
            format!("{test_id}-schema-field"),
            &schema_options,
            &ep.schema,
            &keys.select_schema_placeholder,
            &keys.schema_label,
            vm.open == Some(slots.2),
            slots.2,
            cb,
            layer,
        ));
        column = column.child(schema);
    }

    if let Some(error) = ep.session_error.as_deref() {
        column = column.child(
            text(error.to_owned())
                .key(format!("{test_id}-session-error"))
                .font(k.ty.text_small)
                .color(k.c.danger.fg)
                .mt(4.0)
                .wrap(),
        );
    }

    column
}

/// React's ghost, small, icon-only swap button. The registry has no
/// `ArrowLeftRight`, so two opposing arrows compose the glyph (see module doc).
fn build_swap_button<S: 'static>(
    k: &Kit,
    vm: &EndpointsBarVm,
    keys: &ResolvedKeys,
    cb: &BarCallbacks<S>,
) -> El {
    let tooltip = t(&keys.swap_tooltip).to_owned();
    let mut button = Button::new(tooltip.clone())
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .help(tooltip)
        .label_with(move |_font, ink| {
            row()
                .gap(2.0)
                .child(icon(&ICON_ARROW_DOWN, 14.0).color(ink).a11y_hidden())
                .child(icon(&ICON_ARROW_UP, 14.0).color(ink).a11y_hidden())
        });
    button = button.disabled(
        vm.busy
            || (vm.source.connection_id.is_empty() && vm.target.connection_id.is_empty()),
    );
    if let Some(swap) = cb.swap.clone() {
        button = button.on_press(move |s: &mut S| swap(s));
    }
    button.build(k).key(format!("{}-swap", vm.test_id_prefix)).shrink()
}

/// React's primary compare button: a spinner while busy, otherwise the refresh
/// glyph, followed by the host-supplied `compareLabel`.
fn build_compare_button<S: 'static>(
    k: &Kit,
    vm: &EndpointsBarVm,
    cb: &BarCallbacks<S>,
    dark: bool,
) -> El {
    let prefix = vm.test_id_prefix.clone();
    let leading = if vm.busy {
        Spinner::new(format!("{prefix}-compare-spinner"))
            .size(SpinnerSize::Lg)
            .build(dark)
    } else {
        icon(&ICON_REFRESH, 16.0)
            .key(format!("{prefix}-compare-icon"))
            .a11y_hidden()
    };
    let label = vm.compare_label.clone();
    let label_key = format!("{prefix}-compare-label");
    let mut button = Button::new(label.clone())
        .variant(ButtonVariant::Contrast)
        .disabled(vm.busy || vm.compare_disabled)
        .label_with(move |font, ink| {
            row()
                .gap(6.0)
                .child(leading)
                .child(text(label).key(label_key).font(font).color(ink))
        });
    if let Some(refresh) = cb.refresh.clone() {
        button = button.on_press(move |s: &mut S| refresh(s));
    }
    button
        .build(k)
        .key(format!("{prefix}-compare"))
        .shrink()
        .min_w(0.0)
}
