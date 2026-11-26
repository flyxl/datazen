//! Draw the query plan for the selected editor without replacing its result grid.
use super::{toolbar_button, AppState};
use crate::i18n::t;
use rune::tok::{self, type_};
use rune::{col, row, text, El};

pub(super) fn panel(state: &AppState, theme: tok::Theme) -> Option<El> {
    let plan = state.query_plan.as_ref()?;
    if state
        .tabs
        .selected()
        .is_none_or(|tab| tab.id != plan.tab_id)
    {
        return None;
    }
    let bar = row()
        .gap(8.0)
        .center_y()
        .child(
            text(t("explain.title"))
                .font(type_::LABEL)
                .color(theme.text),
        )
        .child(
            toolbar_button("query-explain-close", t("common.close"), theme)
                .on_press(|state: &mut AppState| state.query_plan = None),
        );
    let body = match &plan.outcome {
        None => text(t("common.loading"))
            .key("query-explain-loading")
            .color(theme.text_secondary),
        Some(Err(error)) => text(error.clone())
            .key("query-explain-error")
            .role("alert")
            .color(theme.text),
        Some(Ok(result)) => col()
            .key("query-explain-plan")
            .w_full()
            .min_h(0.0)
            .h(110.0)
            .scroll(rune::ScrollSpec {
                stick_bottom: false,
            })
            .children(result.plan_text.lines().enumerate().map(|(i, line)| {
                text(line.to_owned())
                    .key(format!("query-explain-plan-line-{i}"))
                    .font(type_::LABEL)
                    .color(theme.text)
            })),
    };
    Some(
        col()
            .key("query-explain-panel")
            .role("region")
            .w_full()
            .gap(4.0)
            .child(bar)
            .child(body),
    )
}
