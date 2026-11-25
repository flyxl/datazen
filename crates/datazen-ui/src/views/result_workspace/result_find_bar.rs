//! Port of `windows/connection/result-workspace/ResultFindBar.tsx` (DB-14).
//!
//! React's find bar owns the inputs and the cursor of a "replace next" walk:
//! a needle field, a column select, an operator select, a match-case toggle,
//! the matched/total count, a close button, and — when the result is writable —
//! a replacement field, plan count, replace-next / replace-all buttons and an
//! error line. Safe Mode keeps the find row but replaces the write row with a
//! reason. The Rune port preserves every test id and hands every change and
//! replacement request to an injected host port; the filtering rules and the
//! grid write stay with the host, so no fake IPC, promise or timer runs here.
//!
//! Rune has no native `Select`, so column/operator are inline keyed dropdowns
//! (the same stand-in used by the other toolbar ports). The registry has no
//! `CaseSensitive` / `ReplaceAll` glyphs; registered `ICON_FILTER` / `ICON_COPY`
//! / `ICON_X` / `ICON_SEARCH` stand in.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this bar yet; the result workspace owns the VM,
//! computes matches/plan and performs the writes. Until then every reusable
//! item carries a per-item `#[allow(dead_code)]` with this stage.

use std::rc::Rc;

use rune::{col, icon, row, text, El, Key, KeyEvent, TextEdit, Tx};

use crate::i18n::t;
use crate::icons::{ICON_COPY, ICON_FILTER, ICON_SEARCH, ICON_X};
use crate::ui::field::TextField;
use crate::ui::kit::kit;

const BAR_KEY: &str = "result-find-bar";
pub const NEEDLE_KEY: &str = "result-find-needle";
pub const COLUMN_KEY: &str = "result-find-column";
pub const OPERATOR_KEY: &str = "result-find-operator";
pub const MATCH_CASE_KEY: &str = "result-find-match-case";
pub const COUNT_KEY: &str = "result-find-count";
pub const CLOSE_KEY: &str = "result-find-close";
pub const REPLACEMENT_KEY: &str = "result-find-replacement";
pub const PLAN_COUNT_KEY: &str = "result-find-plan-count";
pub const REPLACE_NEXT_KEY: &str = "result-find-replace-next";
pub const REPLACE_ALL_KEY: &str = "result-find-replace-all";
pub const ERROR_KEY: &str = "result-find-error";
pub const BLOCKED_KEY: &str = "result-find-replace-blocked";

/// React `ResultFindOperator`.
#[allow(dead_code)] // contract mirror; consumed by the menu and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FindOperator {
    #[default]
    Contains,
    NotContains,
    Prefix,
    Suffix,
}

impl FindOperator {
    /// React `ORDER = Object.keys(operatorLabel)` (insertion order).
    #[allow(dead_code)] // pure projection; consumed by the menu and tests.
    pub const ALL: [Self; 4] = [Self::Contains, Self::NotContains, Self::Prefix, Self::Suffix];

    #[allow(dead_code)] // pure mapping; consumed by the menu and tests.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::NotContains => "notContains",
            Self::Prefix => "prefix",
            Self::Suffix => "suffix",
        }
    }

    #[allow(dead_code)] // pure mapping; consumed by the menu and tests.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Contains => "resultFind.operator.contains",
            Self::NotContains => "resultFind.operator.notContains",
            Self::Prefix => "resultFind.operator.prefix",
            Self::Suffix => "resultFind.operator.suffix",
        }
    }
}

/// React `ResultReplacePlanItem`, reduced to the statement coordinates and the
/// value a replacement would write. The host owns the real write path.
#[allow(dead_code)] // VM mirror; consumed by the replace actions and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanItemVm {
    pub row: usize,
    pub column: String,
    pub value: String,
}

/// React `ResultFindQuery` plus the two inputs' current text.
#[allow(dead_code)] // VM mirror; owned by the result-workspace host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultFindQueryVm {
    pub needle: String,
    pub column: Option<String>,
    pub operator: FindOperator,
    pub match_case: bool,
}

impl Default for ResultFindQueryVm {
    fn default() -> Self {
        Self {
            needle: String::new(),
            column: None,
            operator: FindOperator::Contains,
            match_case: false,
        }
    }
}

/// React `ResultFindBarProps` as a cloneable VM.
#[allow(dead_code)] // VM surface; owned by the result-workspace host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultFindBarVm {
    pub columns: Vec<String>,
    pub query: ResultFindQueryVm,
    pub replacement: String,
    pub matched_rows: usize,
    pub total_rows: usize,
    pub plan: Vec<PlanItemVm>,
    pub can_replace: bool,
    pub replace_reason: Option<String>,
    /// Whether a replace write is in flight (disables the write row, React's
    /// local `busy`).
    pub busy: bool,
    pub error: Option<String>,
    /// Rune-only dropdown state for the two selects.
    pub column_open: bool,
    pub operator_open: bool,
}

impl Default for ResultFindBarVm {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            query: ResultFindQueryVm::default(),
            replacement: String::new(),
            matched_rows: 0,
            total_rows: 0,
            plan: Vec::new(),
            can_replace: false,
            replace_reason: None,
            busy: false,
            error: None,
            column_open: false,
            operator_open: false,
        }
    }
}

impl ResultFindBarVm {
    /// React `t('resultFind.matchCount', { matched, total })`.
    #[allow(dead_code)] // pure helper; pinned by the find-bar tests.
    pub fn match_count_text(&self) -> String {
        t("resultFind.matchCount")
            .replace("{matched}", &self.matched_rows.to_string())
            .replace("{total}", &self.total_rows.to_string())
    }

    /// React `t('resultFind.planCount', { count })`.
    #[allow(dead_code)] // pure helper; pinned by the find-bar tests.
    pub fn plan_count_text(&self) -> String {
        t("resultFind.planCount").replace("{count}", &self.plan.len().to_string())
    }
}

/// Every host intent the find bar can emit.
#[allow(dead_code)] // host port; remove when the result host wires the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FindAction {
    SetNeedle(String),
    SetColumn(Option<String>),
    SetOperator(FindOperator),
    ToggleMatchCase,
    SetReplacement(String),
    ToggleColumnOptions,
    ToggleOperatorOptions,
    ReplaceNext { item: PlanItemVm },
    ReplaceAll,
    Close,
}

/// Injected host port: actions plus the two text accessors the fields edit.
#[allow(dead_code)] // host port; remove when the result host wires the bar.
pub struct FindHostPort<S> {
    pub on_action: Option<Rc<dyn Fn(&mut S, FindAction) + 'static>>,
    pub needle_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
    pub replacement_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
}

impl<S> Default for FindHostPort<S> {
    fn default() -> Self {
        Self {
            on_action: None,
            needle_edit: None,
            replacement_edit: None,
        }
    }
}

fn emit<S: 'static>(host: &FindHostPort<S>, action: FindAction) -> impl Fn(&mut S) + Clone {
    let callback = host.on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// A keyed dropdown row that emits one action when chosen.
fn option_row<S: 'static>(
    key: &str,
    label: String,
    host: &FindHostPort<S>,
    action: FindAction,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut el = row()
        .key(key)
        .role("menuitem")
        .focusable(true)
        .h(22.0)
        .px(6.0)
        .center_y()
        .child(text(label).font(k.ty.text_small).color(k.c.text_base));
    el = el.on_press(emit(host, action));
    el
}

/// Build the find bar. All inputs are bound through `host` accessors; every
/// change and replacement is a [`FindAction`], never a direct host call.
#[allow(dead_code)] // view entry; remove when the result host wires the bar.
pub fn build<S: 'static>(
    vm: &ResultFindBarVm,
    dark: bool,
    host: &FindHostPort<S>,
) -> El {
    let k = kit(dark);
    let mut root = col()
        .key(BAR_KEY)
        .w_full()
        .gap(3.0)
        .child(
            row().key("execution-summary-anchor").w_full().min_w(0.0)
        );

    // ── Find row ────────────────────────────────────────────────────────────
    let mut find_row = row().key("result-find-find-row").w_full().min_w(0.0).gap(6.0).center_y();
    find_row = find_row.child(icon(&ICON_SEARCH, 14.0).color(k.c.text_muted));

    // Needle field: edits the bound TextEdit; Escape closes, Enter walks to the
    // replacement when one is available (React's onKeyDown).
    let needle = TextEdit::new(vm.query.needle.clone());
    let mut needle_field = TextField::new(NEEDLE_KEY, &needle)
        .height(24.0)
        .width(176.0)
        .placeholder(t("resultFind.placeholder"));
    if let Some(accessor) = host.needle_edit.clone() {
        needle_field = needle_field.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
        let emit_close = emit(host, FindAction::Close);
        let emit_replace = emit(host, vm.plan.first().map(|item| FindAction::ReplaceNext {
            item: item.clone(),
        }).unwrap_or(FindAction::Close));
        let can_replace = vm.can_replace && !vm.plan.is_empty();
        let needle_after = needle.text.clone();
        let emit_search = emit(host, FindAction::SetNeedle(needle_after.clone()));
        needle_field = needle_field
            .on_key(move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| {
                if event.key == Key::Escape {
                    emit_close(s);
                    true
                } else if event.key == Key::Enter {
                    if can_replace {
                        emit_replace(s);
                    } else {
                        emit_search(s);
                    }
                    true
                } else {
                    false
                }
            });
    }
    find_row = find_row.child(needle_field.build(dark));

    // Column select trigger + optional dropdown.
    let column_label = vm.query.column.clone().unwrap_or_else(|| t("resultFind.anyColumn").to_owned());
    let mut column_trigger = row()
        .key(COLUMN_KEY)
        .h(24.0)
        .px(6.0)
        .center_y()
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .focusable(true)
        .help(t("resultFind.columnLabel"))
        .child(text(column_label).font(k.ty.text_xsmall).color(k.c.text_base));
    column_trigger = column_trigger.on_press(emit(host, FindAction::ToggleColumnOptions));
    find_row = find_row.child(column_trigger);

    // Operator select trigger + optional dropdown.
    let op_label = t(vm.query.operator.label_key()).to_owned();
    let mut operator_trigger = row()
        .key(OPERATOR_KEY)
        .h(24.0)
        .px(6.0)
        .center_y()
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .focusable(true)
        .help(t("resultFind.operatorLabel"))
        .child(text(op_label).font(k.ty.text_xsmall).color(k.c.text_base));
    operator_trigger = operator_trigger.on_press(emit(host, FindAction::ToggleOperatorOptions));
    find_row = find_row.child(operator_trigger);

    // Match case toggle: aria-pressed in React, shown by accent fill here.
    let mut match_case = row()
        .key(MATCH_CASE_KEY)
        .w(24.0)
        .h(24.0)
        .center()
        .fill(if vm.query.match_case { k.c.text_accent } else { k.c.surface_base })
        .border(1.0, k.c.border_base)
        .focusable(true)
        .help(t("resultFind.matchCase"))
        .child(icon(&ICON_FILTER, 14.0).color(if vm.query.match_case { k.c.text_inverse } else { k.c.text_muted }));
    match_case = match_case.on_press(emit(host, FindAction::ToggleMatchCase));
    find_row = find_row.child(match_case);

    find_row = find_row.child(
        text(vm.match_count_text())
            .key(COUNT_KEY)
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted),
    );
    let mut close = row()
        .key(CLOSE_KEY)
        .w(24.0)
        .h(24.0)
        .center()
        .focusable(true)
        .help(t("resultFind.close"))
        .child(icon(&ICON_X, 14.0).color(k.c.text_muted));
    close = close.on_press(emit(host, FindAction::Close));
    find_row = find_row.child(close);
    root = root.child(find_row);

    // Dropdowns, kept outside the row so they never stretch it.
    if vm.column_open {
        let mut menu = col().key("result-find-column-menu").role("menu").gap(2.0).min_w(120.0);
        menu = menu.child(option_row(
            "result-find-column-option-any",
            t("resultFind.anyColumn").to_owned(),
            host,
            FindAction::SetColumn(None),
            dark,
        ));
        for name in &vm.columns {
            menu = menu.child(option_row(
                &format!("result-find-column-option-{name}"),
                name.clone(),
                host,
                FindAction::SetColumn(Some(name.clone())),
                dark,
            ));
        }
        root = root.child(menu);
    }
    if vm.operator_open {
        let mut menu = col().key("result-find-operator-menu").role("menu").gap(2.0).min_w(140.0);
        for operator in FindOperator::ALL {
            menu = menu.child(option_row(
                &format!("result-find-operator-option-{}", operator.as_str()),
                t(operator.label_key()).to_owned(),
                host,
                FindAction::SetOperator(operator),
                dark,
            ));
        }
        root = root.child(menu);
    }

    // ── Replace row (only when writable) ───────────────────────────────────
    if vm.can_replace {
        let mut replace_row = row()
            .key("result-find-replace-row")
            .w_full()
            .min_w(0.0)
            .gap(6.0)
            .center_y();
        replace_row = replace_row.child(icon(&ICON_COPY, 14.0).color(k.c.text_muted));
        let replacement = TextEdit::new(vm.replacement.clone());
        let mut replacement_field = TextField::new(REPLACEMENT_KEY, &replacement)
            .height(24.0)
            .width(176.0)
            .disabled(vm.busy)
            .placeholder(t("resultFind.replacementPlaceholder"));
        if let Some(accessor) = host.replacement_edit.clone() {
            replacement_field = replacement_field.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
        }
        let submit_emit = emit(host, FindAction::SetReplacement(replacement.text.clone()));
        replacement_field = replacement_field.on_submit(move |s: &mut S, _tx: &mut Tx| submit_emit(s));
        replace_row = replace_row.child(replacement_field.build(dark));
        replace_row = replace_row.child(
            text(vm.plan_count_text())
                .key(PLAN_COUNT_KEY)
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
        let has_plan = !vm.plan.is_empty();
        let next_disabled = vm.busy || !has_plan;
        let mut next = row()
            .key(REPLACE_NEXT_KEY)
            .h(24.0)
            .px(6.0)
            .center_y()
            .fill(k.c.surface_base)
            .border(1.0, k.c.border_base)
            .focusable(!next_disabled)
            .child(text(t("resultFind.replaceNext")).font(k.ty.text_xsmall).color(k.c.text_base));
        if !next_disabled {
            if let Some(item) = vm.plan.first() {
                next = next.on_press(emit(host, FindAction::ReplaceNext { item: item.clone() }));
            }
        }
        replace_row = replace_row.child(next);
        let mut all = row()
            .key(REPLACE_ALL_KEY)
            .h(24.0)
            .px(6.0)
            .center_y()
            .fill(k.c.surface_base)
            .border(1.0, k.c.border_base)
            .focusable(!next_disabled)
            .child(text(t("resultFind.replaceAll")).font(k.ty.text_xsmall).color(k.c.text_base));
        if !next_disabled {
            all = all.on_press(emit(host, FindAction::ReplaceAll));
        }
        replace_row = replace_row.child(all);
        root = root.child(replace_row);

        if let Some(error) = &vm.error {
            root = root.child(
                text(error.clone())
                    .key(ERROR_KEY)
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg),
            );
        }
    } else if let Some(reason) = &vm.replace_reason {
        root = root.child(
            text(reason.clone())
                .key(BLOCKED_KEY)
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }

    root
}
