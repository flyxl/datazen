//! The history sidebar of `QuerySidebarSection`: scope switch, case-insensitive
//! SQL search, grouped entries with status/meta rows, current/all fallback and
//! the clear-history control. Grouping and filtering are pure ports of
//! `historyGroups.ts`; commands go through the shared host port.

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::ICON_TRASH;
use crate::support::formatters::format_last_connected;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

use super::{
    bottom_border, icon_button, HistoryEntryVm, HistoryScope, QuerySidebarAction,
    QuerySidebarActionFn, SearchEditFn, SidebarSectionVm, FALLBACK_KEY, GROUP_LABEL_KEY,
    HISTORY_ITEM_KEY, HISTORY_PANEL_KEY, HISTORY_SEARCH_KEY, SCOPE_ALL_KEY, SCOPE_CURRENT_KEY,
};

/// React `HISTORY_UNKNOWN_DB_KEY`.
#[allow(dead_code)] // grouping constant; remove when the shared helper lands.
pub(crate) const UNKNOWN_DB_KEY: &str = "__unknown_db__";

/// React `QueryHistoryGroup`.
#[allow(dead_code)] // grouping VM; remove when the shared helper lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryGroup {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) entries: Vec<HistoryEntryVm>,
}

/// React `groupQueryHistory`, ordered by each group's newest entry.
#[allow(dead_code)] // pure helper; remove when the shared helper lands.
pub(crate) fn group_history(entries: &[HistoryEntryVm], unknown_label: &str) -> Vec<HistoryGroup> {
    let mut groups: Vec<HistoryGroup> = Vec::new();
    for entry in entries {
        let db = entry.database.as_deref().map(str::trim).filter(|db| !db.is_empty());
        let key = db.unwrap_or(UNKNOWN_DB_KEY).to_owned();
        match groups.iter_mut().find(|group| group.key == key) {
            Some(group) => group.entries.push(entry.clone()),
            None => groups.push(HistoryGroup {
                label: if key == UNKNOWN_DB_KEY {
                    unknown_label.to_owned()
                } else {
                    key.clone()
                },
                key,
                entries: vec![entry.clone()],
            }),
        }
    }
    groups.sort_by(|a, b| {
        let a_time = a
            .entries
            .first()
            .and_then(|entry| crate::support::formatters::parse_zoned_instant(&entry.executed_at));
        let b_time = b
            .entries
            .first()
            .and_then(|entry| crate::support::formatters::parse_zoned_instant(&entry.executed_at));
        b_time.cmp(&a_time)
    });
    groups
}

/// React `findGroupForDatabase`.
#[allow(dead_code)] // pure helper; remove when the shared helper lands.
pub(crate) fn find_group<'a>(
    groups: &'a [HistoryGroup],
    database: Option<&str>,
) -> Option<&'a HistoryGroup> {
    let db = database?.trim();
    if db.is_empty() {
        return None;
    }
    groups.iter().find(|group| group.key == db)
}

/// Build the history aside into `layer`.
#[allow(dead_code)] // panel builder; remove when the query host wires the sidebar.
pub(crate) fn build<S: 'static>(
    vm: &SidebarSectionVm,
    on_action: &Option<QuerySidebarActionFn<S>>,
    search_edit: Option<SearchEditFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let groups = group_history(&vm.history, t("query.historyUnknownDb"));
    let sections = vm.sections(&groups);
    let filtered_count: usize = sections.iter().map(|section| section.items.len()).sum();

    let clear = icon_button::<S>(
        "query-history-clear",
        t("query.clearHistory"),
        &ICON_TRASH,
        on_action.clone().map(|callback| {
            Rc::new(move |s: &mut S| callback(s, QuerySidebarAction::ClearHistory))
                as Rc<dyn Fn(&mut S)>
        }),
        dark,
    );
    let mut header = row()
        .key("query-history-header")
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .justify_between()
        .px(12.0)
        .py(8.0)
        .child(
            text(t("query.historyTitle"))
                .key("query-history-title")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    if !vm.history.is_empty() {
        header = header.child(clear);
        if let Some(callback) = on_action.clone() {
            header = header.on_secondary_press(move |s, press, _tx| {
                callback(s, QuerySidebarAction::HistoryHeaderContextMenu(press));
            });
        }
    }
    let mut aside = col()
        .key(HISTORY_PANEL_KEY)
        .w(256.0)
        .shrink()
        .min_h(0.0)
        .fill(k.c.layer_02)
        .child(header)
        .child(bottom_border(dark));

    if !vm.history.is_empty() {
        aside = aside.child(search_row(vm, on_action, search_edit, dark));
        aside = aside.child(scope_row(vm, on_action, dark));
        if vm.scope_fallback(&groups) {
            aside = aside.child(
                row()
                    .key(FALLBACK_KEY)
                    .w_full()
                    .px(12.0)
                    .py(6.0)
                    .child(
                        text(t("query.historyScopeFallbackHint"))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    ),
            );
        }
    }

    if vm.history.is_empty() {
        aside = aside.child(center_text("query-no-history", t("query.noHistory"), dark));
    } else if filtered_count == 0 {
        aside = aside.child(center_text("query-no-history-match", t("query.noHistoryMatch"), dark));
    } else {
        if vm.history_scope == HistoryScope::Current {
            if let Some(group) = find_group(&groups, vm.selected_database.as_deref()) {
                aside = aside.child(
                    row()
                        .key("history-current-db")
                        .w_full()
                        .gap(4.0)
                        .px(12.0)
                        .py(6.0)
                        .child(
                            text(t("query.database"))
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_muted),
                        )
                        .child(text(":").font(k.ty.text_xsmall).color(k.c.text_muted))
                        .child(
                            text(group.label.clone())
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_base),
                        ),
                );
            }
        }
        for (section_at, section) in sections.iter().enumerate() {
            if let Some(label) = section.label.as_deref() {
                let label_text = format!("{label} ({})", section.items.len());
                aside = aside.child(
                    row()
                        .key(format!("history-group-{section_at}"))
                        .w_full()
                        .px(12.0)
                        .py(4.0)
                        .child(
                            text(label_text)
                                .key(format!("history-group-text-{section_at}"))
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_muted),
                        )
                        .child(
                            row()
                                .key(GROUP_LABEL_KEY)
                                .size(0.0, 0.0)
                                .a11y_hidden(),
                        ),
                );
            }
            for (item_at, entry) in section.items.iter().enumerate() {
                aside = aside.child(history_item(entry, section_at, item_at, on_action, dark));
            }
        }
    }
    layer.push(aside);
}

fn center_text(key: &str, message: &str, dark: bool) -> El {
    let k = kit(dark);
    row()
        .key(key)
        .w_full()
        .px(12.0)
        .py(16.0)
        .child(
            text(message.to_owned())
                .key(format!("{key}-text"))
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        )
}

fn search_row<S: 'static>(
    vm: &SidebarSectionVm,
    on_action: &Option<QuerySidebarActionFn<S>>,
    search_edit: Option<SearchEditFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut field = TextField::new(HISTORY_SEARCH_KEY, &vm.history_search)
        .height(26.0)
        .grow()
        .pad_x(8.0)
        .text_style(k.ty.text_small)
        .placeholder(t("query.searchHistory"));
    if let Some(edit) = search_edit {
        field = field.on_edit(move |s: &mut S| edit(s));
    } else {
        field = field.disabled(true);
    }
    let _ = on_action; // search text is owned by the host's TextEdit accessor
    row()
        .key("query-history-search-row")
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .px(8.0)
        .py(6.0)
        .child(field.build(dark))
}

fn scope_row<S: 'static>(
    vm: &SidebarSectionVm,
    on_action: &Option<QuerySidebarActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let current = vm.history_scope == HistoryScope::Current;
    let make = |key: &str, label: &str, active: bool, scope: HistoryScope| {
        let mut button = Button::new(label.to_owned())
            .variant(if active {
                ButtonVariant::Contrast
            } else {
                ButtonVariant::Neutral
            })
            .control_size(ControlSize::Small)
            .build(k)
            .key(key.to_owned())
            .checked(active);
        if let Some(callback) = on_action.clone() {
            button = button.on_press(move |s: &mut S| {
                callback(s, QuerySidebarAction::SetHistoryScope(scope));
            });
        }
        button
    };
    row()
        .key("query-history-scope-row")
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .px(8.0)
        .py(6.0)
        .child(make(
            SCOPE_CURRENT_KEY,
            t("query.historyScopeCurrent"),
            current,
            HistoryScope::Current,
        ))
        .child(make(
            SCOPE_ALL_KEY,
            t("query.historyScopeAll"),
            !current,
            HistoryScope::All,
        ))
}

fn history_item<S: 'static>(
    entry: &HistoryEntryVm,
    section_at: usize,
    item_at: usize,
    on_action: &Option<QuerySidebarActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let key = format!("{HISTORY_ITEM_KEY}-{section_at}-{item_at}");
    let mut item = col()
        .key(key.clone())
        .role("button")
        .focusable(true)
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .px(12.0)
        .py(8.0)
        .child(
            text(entry.sql.clone())
                .key(format!("history-sql-{section_at}-{item_at}"))
                .font(k.ty.text_small)
                .color(k.c.text_weak)
                .truncate()
                .w_full()
                .min_w(0.0),
        );
    let mut meta = row()
        .key(format!("history-meta-{section_at}-{item_at}"))
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .child(
            text(if entry.success {
                t("common.success")
            } else {
                t("common.failed")
            })
            .key(format!("history-status-{section_at}-{item_at}"))
            .font(k.ty.text_xsmall)
            .color(if entry.success {
                k.c.success.fg
            } else {
                k.c.danger.fg
            }),
        )
        .child(
            text(format!("{}ms", entry.execution_time_ms))
                .key(format!("history-time-{section_at}-{item_at}"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        )
        .child(
            text(format_last_connected(Some(&entry.executed_at)))
                .key(format!("history-date-{section_at}-{item_at}"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    if let Some(rows) = entry.rows_affected {
        meta = meta.child(
            text(t("query.historyRows").replace("{count}", &rows.to_string()))
                .key(format!("history-rows-{section_at}-{item_at}"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    item = item.child(meta);
    if let Some(callback) = on_action.clone() {
        let press_callback = callback.clone();
        let sql = entry.sql.clone();
        item = item.on_press(move |s: &mut S| {
            callback(s, QuerySidebarAction::SelectHistory(sql.clone()));
        });
        let sql = entry.sql.clone();
        item = item.on_secondary_press(move |s, press, _tx| {
            press_callback(
                s,
                QuerySidebarAction::HistoryContextMenu {
                    sql: sql.clone(),
                    press,
                },
            );
        });
    }
    item
}
