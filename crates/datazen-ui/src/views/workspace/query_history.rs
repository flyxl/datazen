//! Render the native host's connection-scoped history without doing IO during a frame.
use crate::i18n::t;
use crate::state_def::AppState;
use crate::views::query::query_sidebar_section::{
    history::{group_history, scope_row, search_row},
    HistoryEntryVm, HistoryScope, QuerySidebarAction, QuerySidebarActionFn, SearchEditFn,
    SidebarSectionVm,
};
use rune::tok::{self, type_};
use rune::{col, row, text, El, ScrollSpec};
use std::{collections::HashMap, rc::Rc};

pub(super) fn panel(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let panel = col().key("query-history-panel").w_full().grow(1.0).gap(6.0);
    let notice = |key, message: String| {
        text(message)
            .key(key)
            .font(type_::LABEL)
            .color(theme.text_secondary)
            .w_full()
    };
    let mut vm = SidebarSectionVm {
        history_scope: state.query_history.scope,
        history_search: state.query_history.search.clone(),
        ..SidebarSectionVm::default()
    };
    let action: Option<QuerySidebarActionFn<AppState>> = Some(Rc::new(|state, action| {
        if let QuerySidebarAction::SetHistoryScope(scope) = action {
            state.query_history.scope = scope;
        }
    }));
    let search: SearchEditFn<AppState> = Rc::new(|state| &mut state.query_history.search);
    let mut panel = panel.child(scope_row(&vm, &action, dark)).child(search_row(
        &vm,
        &action,
        Some(search),
        dark,
    ));
    if state.query_history.pending {
        return panel.child(notice(
            "query-history-loading",
            t("query.historyLoading").to_owned(),
        ));
    }
    match &state.query_history.result {
        Some(Err(error)) => panel.child(notice("query-history-error", error.clone())),
        Some(Ok(history)) if !history.entries.is_empty() => {
            vm.selected_database = history.current_database.clone();
            vm.history = history
                .entries
                .iter()
                .map(|entry| HistoryEntryVm {
                    id: entry.id.clone(),
                    sql: entry.sql.clone(),
                    database: entry.database.clone(),
                    success: entry.success,
                    executed_at: entry.executed_at.clone(),
                    ..HistoryEntryVm::default()
                })
                .collect();
            let groups = group_history(&vm.history, t("query.historyUnknownDb"));
            let sections = vm.sections(&groups);
            if vm.scope_fallback(&groups) {
                panel = panel.child(notice(
                    "history-scope-fallback-hint",
                    t("query.historyScopeFallbackHint").to_owned(),
                ));
            }
            if sections.iter().all(|section| section.items.is_empty()) {
                return panel.child(notice(
                    "query-no-history-match",
                    t("query.noHistoryMatch").to_owned(),
                ));
            }
            if vm.history_scope == HistoryScope::Current && !vm.scope_fallback(&groups) {
                if let Some(database) = vm.selected_database.as_deref() {
                    panel = panel.child(notice(
                        "history-current-db",
                        format!("{}: {database}", t("query.database")),
                    ));
                }
            }
            let indexed = history
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.id.as_str(), (index, entry)))
                .collect::<HashMap<_, _>>();
            let mut list = col()
                .key("query-history-list")
                .w_full()
                .grow(1.0)
                .gap(4.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                });
            for (index, section) in sections
                .into_iter()
                .filter(|section| !section.items.is_empty())
                .enumerate()
            {
                let mut group = col()
                    .key(format!("history-group-{index}"))
                    .w_full()
                    .gap(4.0);
                if let Some(label) = section.label {
                    group = group.child(notice("history-group-label", label));
                }
                for vm_entry in section.items {
                    let Some(&(index, entry)) = indexed.get(vm_entry.id.as_str()) else {
                        continue;
                    };
                    let sql = entry.sql.clone();
                    let preview = sql.split_whitespace().collect::<Vec<_>>().join(" ");
                    let (status_key, status) = if entry.success {
                        ("query-history-success", t("query.historyStatusSuccess"))
                    } else {
                        ("query-history-failure", t("query.historyStatusFailed"))
                    };
                    let mut card = col()
                        .key(format!("query-history-entry-{index}"))
                        .role("button")
                        .focusable(true)
                        .label(sql.clone())
                        .w_full()
                        .px(6.0)
                        .py(4.0)
                        .radius(6.0)
                        .fill(theme.bg)
                        .child(
                            row().w_full().child(
                                text(preview)
                                    .key(format!("query-history-sql-{index}"))
                                    .font(type_::LABEL)
                                    .color(theme.text)
                                    .w_full()
                                    .truncate(),
                            ),
                        )
                        .child(notice(status_key, status.to_owned()))
                        .on_press(move |s: &mut AppState| s.open_history_statement(&sql));
                    if let Some(error) = &entry.error_message {
                        card = card.child(notice("query-history-entry-error", error.clone()));
                    }
                    group = group.child(card);
                }
                list = list.child(group);
            }
            let panel = if history.total > history.entries.len() as u64 {
                panel.child(notice(
                    "query-history-truncated",
                    t("query.historyShowingOf")
                        .replace("{shown}", &history.entries.len().to_string())
                        .replace("{total}", &history.total.to_string()),
                ))
            } else {
                panel
            };
            panel.child(list)
        }
        _ => panel.child(notice(
            "query-history-empty",
            t("query.noHistory").to_owned(),
        )),
    }
}
