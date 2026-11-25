//! Render the native host's connection-scoped history without doing IO during a frame.
use crate::i18n::t;
use crate::state_def::AppState;
use rune::tok::{self, type_};
use rune::{col, row, text, El, ScrollSpec};

pub(super) fn panel(state: &AppState, theme: tok::Theme) -> El {
    let panel = col().key("query-history-panel").w_full().grow(1.0).gap(6.0);
    let notice = |key, message: String| {
        text(message)
            .key(key)
            .font(type_::LABEL)
            .color(theme.text_secondary)
            .w_full()
    };
    if state.query_history.pending {
        return panel.child(notice(
            "query-history-loading",
            t("query.historyLoading").to_owned(),
        ));
    }
    match &state.query_history.result {
        Some(Err(error)) => panel.child(notice("query-history-error", error.clone())),
        Some(Ok(history)) if !history.entries.is_empty() => {
            let mut list = col()
                .key("query-history-list")
                .w_full()
                .grow(1.0)
                .gap(4.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                });
            for (index, entry) in history.entries.iter().enumerate() {
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
                list = list.child(card);
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
