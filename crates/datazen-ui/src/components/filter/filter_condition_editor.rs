//! One expanded filter condition: the column, the operator, the value and the
//! remove button.
//!
//! Ported 1:1 from `packages/frontend/src/components/filter/FilterConditionEditor.tsx`.
//!
//! The component owns one piece of state only — which **select listbox** is
//! open — and reports every change to the parent, which owns the draft. Rune
//! has no hooks, so the text drafts live in the host state and are handed in as
//! [`TextEdit`]s, exactly like the ported `FilterBetweenInput` does.
//!
//! **The operator list depends on the column.** A pattern operator on a numeric
//! column would be a query the database is guaranteed to reject, so the list is
//! the intersection of "the column type takes it" and "this database serves it"
//! ([`operators_for_column`]). The draft's current operator stays visible even
//! when it is not in that list: the row holds what was put there, and hiding it
//! would make the editor lie about the draft (React lines 96–99).
//!
//! ## Keys
//!
//! | part            | key                              | React `data-testid` / label |
//! |-----------------|----------------------------------|-----------------------------|
//! | root            | `<key>`                          | —                           |
//! | column select   | `<key>-column` (trigger `-trigger`)| —                          |
//! | operator select | `<key>-operator` (trigger `-trigger`)| —                        |
//! | single value    | `<key>-value`                    | `filter-value`              |
//! | like hint       | `<key>-like-case-insensitive`     | `filter-like-case-insensitive` |
//! | tokens root     | `<key>-tokens`                   | `filter-token-input`        |
//! | one token       | `<key>-token-<n>`                | `filter-token`              |
//! | token remove    | `<key>-token-<n>-remove`         | `filter-token-remove`       |
//! | token field     | `<key>-token-field`              | `filter-token-field`        |
//! | between bounds  | `<key>-between-lower/-upper/-connector/-inverted` | `filter-value` ×2 |
//! | remove          | `<key>-remove`                   | `aria-label="filter.remove"` |
//!
//! ## The value controls and their owners
//!
//! React renders `FilterValueInput` / `FilterTokenInput` and
//! `FilterBetweenInput` (T021). All three now use real ports: T166 owns the
//! single-value and token controls (`filter_value_input` / `filter_token_input`)
//! and this module only wires them to its own commit/collapse rules. The
//! earlier local stand-ins were removed by T166; keep that replacement note
//! here so a future revert is obvious.
//!
//! ## Collapse on completion (React `onBlur` / deferred timer)
//!
//! React defers a `setTimeout(10)` on blur, re-checks `document.activeElement`,
//! the open listbox and `isCompleteFilter`, then calls `onCollapse`. Rune has
//! no per-element focus/blur event (only the window-level `Runtime::on_key`), so
//! the *expressible* parts are wired here and the rest is left to the panel:
//!
//! 1. choosing an operator whose value kind is `none` collapses immediately
//!    (React does this synchronously, before any timer);
//! 2. Enter in the single-value or token field commits and collapses when the
//!    condition is complete (React `onEnterComplete`);
//! 3. every other "focus left and the condition is complete" case is the
//!    caller's: `on_collapse` is the same hook, and the filter panel (T024)
//!    decides when focus left the row.
//!
//! ## i18n
//!
//! Labels are `t(filter.<operator>)` for every operator; the rest are the
//! keys React resolves: `filter.value`, `filter.tokensPlaceholder`,
//! `filter.and`, `filter.betweenInverted`, `filter.likeCaseInsensitiveHint`,
//! `filter.tokenRemove`, `filter.remove`. No key is invented.

// Dead-code lint is silenced per item below, each naming its eventual owner
// and removal stage, rather than with a crate-wide or module-wide blanket.

use std::rc::Rc;

use rune::{row, text, El, Key, KeyEvent, TextEdit, Tx};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit};

use crate::i18n::t;
use crate::icons::ICON_X;
use crate::support::cell_types::CellTypeHints;
use crate::ui::kit::kit;
use crate::ui::select::Select;

use super::filter_between_input::FilterBetweenInput;
use super::filter_condition_chip::{FilterCondition, FilterValue};
use super::filter_token_input::FilterTokenInput;
use super::filter_value_input::FilterValueInput;
use super::filter_operators::{
    convert_filter_value, normalize_operator_for_column, operators_for_column, static_operator,
    value_input_for, FilterValueInputKind,
};
use super::filter_values::{to_pair, to_tokens};

/// A column the filter editor can filter on.
///
/// React `components/filter/types.ts` (`FilterColumn`), owned by T216. It lives
/// here until T216 lands because T056 owns `components/filter/`; T216 moves it
/// and deletes this copy. `data_type` is React's `type` — the driver's type
/// string (`int4`, `varchar(64)`, …); `None` falls back to the text operators.
#[allow(dead_code)] // T216 owns this type; delete this local copy when its module lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FilterColumn {
    pub name: String,
    pub data_type: Option<String>,
}

#[allow(dead_code)] // T216 owns this type; delete this local copy when its module lands.
impl FilterColumn {
    pub(crate) fn new(name: impl Into<String>, data_type: Option<&str>) -> Self {
        FilterColumn {
            name: name.into(),
            data_type: data_type.map(str::to_owned),
        }
    }
}

/// Which of the editor's two select listboxes is open.
///
/// Rune's select needs its open state from the caller; React's `Select` keeps
/// it internally. The parent owns it, one slot per editor row.
#[allow(dead_code)] // the filter panel (T024) will own the open-listbox state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditorSelect {
    Column,
    Operator,
}

/// Whether a condition carries everything it needs to be sent to the database.
///
/// React `stores/tableData/filterUtils.ts` (`isCompleteFilter`), ported here
/// because the table-data store has no Rune home yet; that store port owns it
/// later. The predicate mirrors the back end's `filter_is_complete`: the
/// operators that carry no value are complete on their own, the set operators
/// need at least one value, and the range operators need exactly two bounds.
#[allow(dead_code)] // the table-data store port owns this predicate; delete when it lands.
pub(crate) fn is_complete_filter(filter: &FilterCondition) -> bool {
    if filter.column.is_empty() {
        return false;
    }
    match value_input_for(filter.operator) {
        FilterValueInputKind::None => true,
        FilterValueInputKind::Pair => is_complete_pair(&filter.value),
        FilterValueInputKind::Tokens => is_complete_token_list(&filter.value),
        // A single literal: only the empty string counts as missing.
        FilterValueInputKind::Single => match &filter.value {
            FilterValue::Single(text) => !text.is_empty(),
            FilterValue::None => false,
            // Not reachable through the editor; React would call it a value.
            FilterValue::Pair(_, _) | FilterValue::Tokens(_) => true,
        },
    }
}

/// The two bounds of a range operator, in the order the back end reads them.
fn is_complete_pair(value: &FilterValue) -> bool {
    match value {
        FilterValue::Pair(lower, upper) => is_filled(lower) && is_filled(upper),
        FilterValue::Tokens(tokens) => tokens.len() == 2 && tokens.iter().all(|t| is_filled(t)),
        FilterValue::Single(text) => {
            let parts: Vec<&str> = text.split(',').collect();
            parts.len() == 2 && parts.iter().all(|part| is_filled(part))
        }
        FilterValue::None => false,
    }
}

/// The values of a set operator: one is enough, none is not.
fn is_complete_token_list(value: &FilterValue) -> bool {
    match value {
        FilterValue::Tokens(tokens) => !tokens.is_empty(),
        FilterValue::Single(text) => text.split(',').any(|part| !part.trim().is_empty()),
        // An array of any length, including two empty strings, is a value.
        FilterValue::Pair(_, _) => true,
        FilterValue::None => false,
    }
}

fn is_filled(text: &str) -> bool {
    !text.trim().is_empty()
}

type Change<S> = Rc<dyn Fn(&mut S, usize, FilterCondition)>;
type Action<S> = Rc<dyn Fn(&mut S)>;
type Remove<S> = Rc<dyn Fn(&mut S, usize)>;
type Toggle<S> = Rc<dyn Fn(&mut S, EditorSelect)>;

/// What a control's handler needs besides the host state.
///
/// A kit closure must be `'static`, so the rules are cloned out of the builder
/// and shared through one `Rc` — the same shape the migration endpoint bar
/// uses.
struct Rules<S: 'static> {
    index: usize,
    columns: Vec<FilterColumn>,
    supported: Vec<&'static str>,
    hints: Option<CellTypeHints>,
    /// The builder's snapshot, used when the caller wires no accessor.
    snapshot: FilterCondition,
    current: Option<fn(&S) -> &FilterCondition>,
    on_change: Option<Change<S>>,
    on_collapse: Option<Action<S>>,
}

impl<S: 'static> Rules<S> {
    /// The live draft (React `latestRef.current`).
    fn live(&self, state: &S) -> FilterCondition {
        match self.current {
            Some(read) => read(state).clone(),
            None => self.snapshot.clone(),
        }
    }

    /// React `changeOperator`: convert the value to the new control's shape and
    /// collapse as soon as the operator carries no value.
    fn change_operator(&self, state: &mut S, operator: &str) {
        let filter = self.live(state);
        // An unknown name can only be a caller-supplied operator, whose id is
        // already `'static`; keep it instead of inventing one.
        let operator = static_operator(operator).unwrap_or(filter.operator);
        let from = value_input_for(filter.operator);
        let to = value_input_for(operator);
        let next = FilterCondition {
            column: filter.column,
            operator,
            value: convert_filter_value(from, to, &filter.value),
            connector: filter.connector,
        };
        if let Some(change) = &self.on_change {
            change(state, self.index, next);
        }
        // An operator without a value has nothing left to edit: collapse to the chip.
        if to == FilterValueInputKind::None {
            if let Some(collapse) = &self.on_collapse {
                collapse(state);
            }
        }
    }

    /// React `changeColumn`: the value is kept, the operator is normalised for
    /// the new column's type, and both are converted together.
    fn change_column(&self, state: &mut S, column: &str) {
        let filter = self.live(state);
        let next_type = self
            .columns
            .iter()
            .find(|entry| entry.name == column)
            .and_then(|entry| entry.data_type.as_deref());
        let operator = normalize_operator_for_column(
            filter.operator,
            next_type,
            &self.supported,
            self.hints.as_ref(),
        );
        let next = FilterCondition {
            column: column.to_owned(),
            operator,
            value: convert_filter_value(
                value_input_for(filter.operator),
                value_input_for(operator),
                &filter.value,
            ),
            connector: filter.connector,
        };
        if let Some(change) = &self.on_change {
            change(state, self.index, next);
        }
    }

    /// Put `value` on the live draft and report whether the result is complete.
    fn commit_value(&self, state: &mut S, value: FilterValue) -> bool {
        let mut next = self.live(state);
        next.value = value;
        let complete = is_complete_filter(&next);
        if let Some(change) = &self.on_change {
            change(state, self.index, next);
        }
        complete
    }

    fn collapse(&self, state: &mut S) {
        if let Some(collapse) = &self.on_collapse {
            collapse(state);
        }
    }
}

/// The editable draft of one value control and the accessor that binds it to
/// the host state (`&mut S`), the way `TextField::on_edit` wants it.
struct ValueDraft<'a, S: 'static> {
    edit: &'a TextEdit,
    bind: fn(&mut S) -> &mut TextEdit,
}

/// The two drafts of a range value.
struct BetweenDrafts<'a, S: 'static> {
    lower: &'a TextEdit,
    upper: &'a TextEdit,
    on_lower: fn(&mut S) -> &mut TextEdit,
    on_upper: fn(&mut S) -> &mut TextEdit,
}

/// Builder for one expanded condition. `S` is the host state that owns the
/// draft and the text the value controls edit.
#[allow(dead_code)] // the filter panel view (T024) is the production consumer.
pub(crate) struct FilterConditionEditor<'a, S: 'static> {
    key: String,
    index: usize,
    filter: &'a FilterCondition,
    columns: &'a [FilterColumn],
    supported: &'a [&'static str],
    hints: Option<&'a CellTypeHints>,
    like_case_insensitive: bool,
    disabled: bool,
    open: Option<EditorSelect>,
    single: Option<ValueDraft<'a, S>>,
    tokens: Option<ValueDraft<'a, S>>,
    between: Option<BetweenDrafts<'a, S>>,
    current: Option<fn(&S) -> &FilterCondition>,
    on_change: Option<Change<S>>,
    on_remove: Option<Remove<S>>,
    on_collapse: Option<Action<S>>,
    on_toggle: Option<Toggle<S>>,
    on_close: Option<Action<S>>,
}

#[allow(dead_code)] // the filter panel view (T024) is the production consumer.
impl<'a, S: 'static> FilterConditionEditor<'a, S> {
    /// The row's key, its index in the draft, the draft it edits and the
    /// columns it may switch to.
    pub(crate) fn new(
        key: impl Into<String>,
        index: usize,
        filter: &'a FilterCondition,
        columns: &'a [FilterColumn],
    ) -> Self {
        FilterConditionEditor {
            key: key.into(),
            index,
            filter,
            columns,
            supported: &[],
            hints: None,
            like_case_insensitive: false,
            disabled: false,
            open: None,
            single: None,
            tokens: None,
            between: None,
            current: None,
            on_change: None,
            on_remove: None,
            on_collapse: None,
            on_toggle: None,
            on_close: None,
        }
    }

    /// The driver's operator whitelist (React `supportedOperators`).
    pub(crate) fn supported_operators(mut self, supported: &'a [&'static str]) -> Self {
        self.supported = supported;
        self
    }

    /// The driver's own column-type rules (React `cellTypeHints`).
    pub(crate) fn cell_type_hints(mut self, hints: Option<&'a CellTypeHints>) -> Self {
        self.hints = hints;
        self
    }

    /// Whether this database ignores case in `LIKE` (React `likeCaseInsensitive`).
    pub(crate) fn like_case_insensitive(mut self, on: bool) -> Self {
        self.like_case_insensitive = on;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// The open listbox, owned by the parent (React keeps it in `Select`).
    pub(crate) fn open(mut self, open: Option<EditorSelect>) -> Self {
        self.open = open;
        self
    }

    /// The single-value draft (React `FilterValueInput`, owned by T166).
    pub(crate) fn single_draft(
        mut self,
        edit: &'a TextEdit,
        bind: fn(&mut S) -> &mut TextEdit,
    ) -> Self {
        self.single = Some(ValueDraft { edit, bind });
        self
    }

    /// The pending-token draft (React `FilterTokenInput`, owned by T166).
    pub(crate) fn token_draft(
        mut self,
        edit: &'a TextEdit,
        bind: fn(&mut S) -> &mut TextEdit,
    ) -> Self {
        self.tokens = Some(ValueDraft { edit, bind });
        self
    }

    /// The two range drafts (the ported `FilterBetweenInput`).
    pub(crate) fn between_drafts(
        mut self,
        lower: &'a TextEdit,
        upper: &'a TextEdit,
        on_lower: fn(&mut S) -> &mut TextEdit,
        on_upper: fn(&mut S) -> &mut TextEdit,
    ) -> Self {
        self.between = Some(BetweenDrafts {
            lower,
            upper,
            on_lower,
            on_upper,
        });
        self
    }

    /// Reads the live draft from the host state (React `latestRef`).
    ///
    /// Every handler runs after the parent has re-rendered at least once, so a
    /// snapshot could be a frame stale; the accessor cannot.
    pub(crate) fn current(mut self, f: fn(&S) -> &FilterCondition) -> Self {
        self.current = Some(f);
        self
    }

    /// React `onChange(index, next)`: the parent owns the draft.
    pub(crate) fn on_change(mut self, f: impl Fn(&mut S, usize, FilterCondition) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// React `onRemove(index)`.
    pub(crate) fn on_remove(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_remove = Some(Rc::new(f));
        self
    }

    /// React `onCollapse()`: the row became a chip.
    pub(crate) fn on_collapse(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_collapse = Some(Rc::new(f));
        self
    }

    /// Route a select's open/close to host state.
    pub(crate) fn on_toggle(mut self, f: impl Fn(&mut S, EditorSelect) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    /// Presses outside and Escape close the listbox.
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// The column's driver type string, or `None` when the column is unknown.
    fn column_type(&self) -> Option<&str> {
        self.columns
            .iter()
            .find(|column| column.name == self.filter.column)
            .and_then(|column| column.data_type.as_deref())
    }

    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let filter = self.filter;
        let rules = Rc::new(Rules {
            index: self.index,
            columns: self.columns.to_vec(),
            supported: self.supported.to_vec(),
            hints: self.hints.cloned(),
            snapshot: filter.clone(),
            current: self.current,
            on_change: self.on_change.clone(),
            on_collapse: self.on_collapse.clone(),
        });

        // ── column select ───────────────────────────────────────────────────
        let column_names: Vec<String> = self
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
        let column_refs: Vec<&str> = column_names.iter().map(String::as_str).collect();
        let mut column_select = Select::new(format!("{}-column", self.key), &column_refs)
            .width(104.0)
            .disabled(self.disabled)
            .open(self.open == Some(EditorSelect::Column));
        if !filter.column.is_empty() {
            column_select = column_select.current(Some(filter.column.as_str()));
        }
        if let Some(toggle) = &self.on_toggle {
            let toggle = toggle.clone();
            column_select =
                column_select.on_toggle(move |s: &mut S| toggle(s, EditorSelect::Column));
        }
        if let Some(close) = &self.on_close {
            let close = close.clone();
            column_select = column_select.on_close(move |s: &mut S| close(s));
        }
        let choosing = rules.clone();
        column_select = column_select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
            choosing.change_column(s, value);
        });
        let mut root = row()
            .key(self.key.clone())
            .self_start()
            .min_w(0.0)
            .gap(4.0)
            .center_y()
            .radius(k.radius.sm)
            .py(2.0)
            .pl(2.0)
            .pr(2.0)
            .fill(k.c.surface_weak)
            .border(1.0, k.c.border_weak)
            .child(column_select.build(&k, layer));

        // ── operator select ─────────────────────────────────────────────────
        let mut operators = operators_for_column(self.column_type(), self.supported, self.hints);
        // The draft's operator stays visible even out of the intersection.
        if !operators.contains(&filter.operator) {
            operators.push(filter.operator);
        }
        let operator_labels: Vec<String> = operators
            .iter()
            .map(|operator| t(&format!("filter.{operator}")).to_owned())
            .collect();
        let operator_refs: Vec<&str> = operators.to_vec();
        let label_refs: Vec<&str> = operator_labels.iter().map(String::as_str).collect();
        let current_label = t(&format!("filter.{}", filter.operator)).to_owned();
        let mut operator_select = Select::new(format!("{}-operator", self.key), &label_refs)
            .values(&operator_refs)
            .current(Some(&current_label))
            .width(76.0)
            .disabled(self.disabled)
            .open(self.open == Some(EditorSelect::Operator));
        if let Some(toggle) = &self.on_toggle {
            let toggle = toggle.clone();
            operator_select =
                operator_select.on_toggle(move |s: &mut S| toggle(s, EditorSelect::Operator));
        }
        if let Some(close) = &self.on_close {
            let close = close.clone();
            operator_select = operator_select.on_close(move |s: &mut S| close(s));
        }
        let choosing = rules.clone();
        operator_select = operator_select.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
            choosing.change_operator(s, value);
        });
        root = root.child(operator_select.build(&k, layer));

        // ── value control, by the operator's kind ────────────────────────────
        match value_input_for(filter.operator) {
            FilterValueInputKind::None => {}
            FilterValueInputKind::Single => {
                // T166: the real `FilterValueInput` owns the draft/field/Enter
                // contract; this module only translates its callbacks.
                root = root.child(match &self.single {
                    Some(draft) => {
                        let bind = draft.bind;
                        let committing = rules.clone();
                        let collapsing = rules.clone();
                        FilterValueInput::<S>::new(self.key.clone(), draft.edit, bind)
                        .placeholder(t("filter.value"))
                        .disabled(self.disabled)
                        // T056's editor contract commits on every Enter; keep it
                        // until its tests follow React's unchanged-value skip.
                        .commit_unchanged(true)
                        .on_commit(move |s: &mut S, value: String| {
                            committing.commit_value(s, FilterValue::Single(value));
                        })
                        .on_enter_complete(move |s: &mut S| {
                            if is_complete_filter(&collapsing.live(s)) {
                                collapsing.collapse(s);
                            }
                        })
                        .build(dark)
                    }
                    None => self.read_only_single(&k),
                });
                if self.like_case_insensitive && filter.operator == "like" {
                    let hint = t("filter.likeCaseInsensitiveHint");
                    root = root.child(
                        text(hint)
                            .key(format!("{}-like-case-insensitive", self.key))
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .max_w(192.0)
                            .truncate()
                            .help(hint),
                    );
                }
            }
            FilterValueInputKind::Tokens => {
                // T166: the real `FilterTokenInput` owns chips/Enter/comma/
                // Backspace; this module owns the token list and collapse rule.
                let tokens = to_tokens(&self.filter.value);
                root = root.child(match &self.tokens {
                    Some(draft) => {
                        let bind = draft.bind;
                        let committing = rules.clone();
                        let collapsing = rules.clone();
                        FilterTokenInput::<S>::new(
                            self.key.clone(),
                            tokens,
                            draft.edit,
                            bind,
                        )
                        .placeholder(t("filter.tokensPlaceholder"))
                        .disabled(self.disabled)
                        .remove_label(t("filter.tokenRemove"))
                        .on_commit(move |s: &mut S, next: Vec<String>| {
                            committing.commit_value(s, FilterValue::Tokens(next));
                        })
                        .on_enter_complete(move |s: &mut S| {
                            if is_complete_filter(&collapsing.live(s)) {
                                collapsing.collapse(s);
                            }
                        })
                        .build(dark)
                    }
                    None => self.read_only_tokens(&k),
                });
            }
            FilterValueInputKind::Pair => {
                root = root.child(self.pair_value(dark));
            }
        }

        // ── remove button ───────────────────────────────────────────────────
        let remove_label = t("filter.remove");
        let mut remove = IconButton::new(&ICON_X)
            .variant(IconButtonVariant::GhostMuted)
            .control_size(ControlSize::Small)
            .label(remove_label)
            .help(remove_label)
            .disabled(self.disabled);
        if let Some(on_remove) = &self.on_remove {
            let on_remove = on_remove.clone();
            let index = self.index;
            remove = remove.on_press(move |s: &mut S| on_remove(s, index));
        }
        let mut remove = remove
            .build(k)
            .key(format!("{}-remove", self.key));
        // React's native button also fires on Enter; Rune presses a focused
        // button with Space only, so the row carries an explicit Enter handler
        // to keep the two activations at parity.
        if let Some(on_remove) = &self.on_remove {
            let on_remove = on_remove.clone();
            let index = self.index;
            remove = remove.on_key(move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                if event.key == Key::Enter {
                    on_remove(s, index);
                    true
                } else {
                    false
                }
            });
        }
        root.child(remove)
    }

    /// The range control, reusing the ported `FilterBetweenInput` (T021).
    ///
    /// React commits each keystroke through `onCommit`; Rune edits the host's
    /// two drafts directly, so the parent reads them when it builds the query.
    fn pair_value(&self, dark: bool) -> El {
        let Some(drafts) = &self.between else {
            return self.read_only_pair(dark);
        };
        let mut built = FilterBetweenInput::new(
            format!("{}-between", self.key),
            drafts.lower,
            drafts.upper,
            drafts.on_lower,
            drafts.on_upper,
        )
        .placeholder(t("filter.value"))
        .connector_label(t("filter.and"))
        .inverted_label(t("filter.betweenInverted"))
        .build(dark);
        if self.disabled {
            built = built.disabled(true);
        }
        built
    }

    /// A value with no draft: the condition's value, read only.
    fn read_only_value(&self, k: &Kit, suffix: &str, value: String) -> El {
        row()
            .key(format!("{}-{suffix}", self.key))
            .h(28.0)
            .min_w(0.0)
            .px(8.0)
            .center_y()
            .radius(k.radius.sm)
            .fill(k.c.surface_raised)
            .child(
                text(value)
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .truncate(),
            )
    }

    /// The scalar value with no draft (a single operator on a row the caller
    /// does not let the user edit).
    fn read_only_single(&self, k: &Kit) -> El {
        let value = match &self.filter.value {
            FilterValue::Single(text) => text.clone(),
            FilterValue::Pair(lower, _) => lower.clone(),
            FilterValue::Tokens(tokens) => tokens.join(", "),
            FilterValue::None => String::new(),
        };
        self.read_only_value(k, "value", value)
    }

    /// The token chips with no editable draft (a read-only row).
    fn read_only_tokens(&self, k: &Kit) -> El {
        let tokens = to_tokens(&self.filter.value);
        let mut list = row()
            .key(format!("{}-tokens", self.key))
            .min_w(0.0)
            .max_w(256.0)
            .gap(2.0)
            .center_y();
        for (position, token) in tokens.iter().enumerate() {
            list = list.child(
                row()
                    .key(format!("{}-token-{position}", self.key))
                    .min_w(0.0)
                    .max_w(128.0)
                    .gap(2.0)
                    .center_y()
                    .radius(k.radius.sm)
                    .pl(4.0)
                    .pr(2.0)
                    .py(1.0)
                    .fill(k.alpha(k.c.bg_accent, 0.15))
                    .child(
                        text(token.clone())
                            .key("label")
                            .font(k.ty.small)
                            .color(k.c.text_accent)
                            .truncate(),
                    ),
            );
        }
        list
    }

    /// The two bounds of a range with no drafts, read only.
    fn read_only_pair(&self, dark: bool) -> El {
        let k = kit(dark);
        let (lower, upper) = to_pair(&self.filter.value);
        let mut line = row()
            .key(format!("{}-between", self.key))
            .gap(4.0)
            .center_y()
            .min_w(0.0)
            .child(self.read_only_value(&k, "between-lower", lower));
        line = line.child(
            text(t("filter.and"))
                .key(format!("{}-between-connector", self.key))
                .font(k.ty.small)
                .color(k.c.text_muted),
        );
        line.child(self.read_only_value(&k, "between-upper", upper))
    }
}
