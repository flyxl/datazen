//! Query grid CSV export. The selected result, not editor text or live session,
//! owns both the published data reference and all asynchronous feedback.
use crate::i18n::t;
use crate::state_def::{AppState, NativeQueryCsvStatus, QueryResultTab};
use crate::types::ResultView;
use crate::ui::{dialog::Dialog, kit::kit};
use datazen_ui_api::AdapterJob;
use rune::tok::{self, type_};
use rune::{text, El, FileDialogOptions, Tx};
use rune_kit::{Button, ButtonVariant};

fn active(state: &AppState) -> Option<&QueryResultTab> {
    let id = state.selected_query_result?;
    let tab = state.query_result_tabs.iter().find(|tab| tab.id == id)?;
    tab.csv_source.is_some().then_some(tab)
}

pub(super) fn trigger(state: &AppState, theme: tok::Theme) -> Option<El> {
    if state.result_view != ResultView::Table {
        return None;
    }
    let tab = active(state)?;
    let id = tab.id;
    let control = super::toolbar_button("data-table-export", t("export.export"), theme);
    if tab
        .csv_status
        .as_ref()
        .is_some_and(NativeQueryCsvStatus::busy)
    {
        return Some(control);
    }
    Some(control.on_press(move |state: &mut AppState| {
        state.query_csv_dialog = Some(id);
    }))
}

pub(super) fn status(state: &AppState, theme: tok::Theme) -> Option<El> {
    let (key, message) = match active(state)?.csv_status.as_ref()? {
        NativeQueryCsvStatus::ChoosingFile => return None,
        NativeQueryCsvStatus::Saving => ("query-csv-saving", t("export.exporting").to_owned()),
        NativeQueryCsvStatus::Saved => ("query-csv-saved", t("export.success").to_owned()),
        NativeQueryCsvStatus::Cancelled => {
            ("query-csv-cancelled", "No file was written.".to_owned())
        }
        NativeQueryCsvStatus::Failed(error) => ("query-csv-error", error.clone()),
    };
    Some(
        text(message)
            .key(key)
            .role("status")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    )
}

pub(super) fn dialog(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    let Some(tab) = active(state).filter(|tab| state.query_csv_dialog == Some(tab.id)) else {
        return;
    };
    let Some(source) = tab.csv_source.as_ref() else {
        return;
    };
    let id = tab.id;
    let k = kit(dark);
    let close = |state: &mut AppState| state.query_csv_dialog = None;
    let cancel = Button::new(t("common.cancel").to_owned())
        .build(k)
        .key("query-csv-dialog-cancel")
        .on_press(close);
    let confirm = Button::new(t("export.export").to_owned())
        .variant(ButtonVariant::Submit)
        .build(k)
        .key("query-csv-dialog-confirm")
        .on_press_tx(move |state: &mut AppState, tx: &mut Tx| choose_path(state, tx, id));
    Dialog::new("data-export-dialog", t("export.export"))
        .open(true)
        .width(400.0)
        .height(210.0)
        .child(text(format!(
            "CSV · {} × {}",
            source.row_count,
            source.columns.len()
        )))
        .footer(cancel)
        .footer(confirm)
        .on_close(close)
        .build(k, layer);
}

fn choose_path(state: &mut AppState, tx: &mut Tx, id: u32) {
    if state.query_csv_dialog.take() != Some(id) || state.selected_query_result != Some(id) {
        return;
    }
    let Some(tab) = state.query_result_tabs.iter_mut().find(|tab| tab.id == id) else {
        return;
    };
    let Some(source) = tab.csv_source.clone() else {
        return;
    };
    tab.csv_status = Some(NativeQueryCsvStatus::ChoosingFile);
    let options = FileDialogOptions {
        title: t("export.export").to_owned(),
        file_name: Some("query_result.csv".into()),
        ..Default::default()
    }
    .filter("CSV", &["csv"]);
    tx.save_file(options, move |state: &mut AppState, response, _| {
        let Some(index) = state.query_result_tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        state.query_result_tabs[index].csv_status = Some(match response {
            Ok(Some(destination)) => {
                if state.desktop.begin_job(AdapterJob::SaveNativeQueryCsv {
                    result_id: id,
                    source,
                    destination,
                }) {
                    NativeQueryCsvStatus::Saving
                } else {
                    NativeQueryCsvStatus::Failed("The native result export did not start.".into())
                }
            }
            Ok(None) => NativeQueryCsvStatus::Cancelled,
            Err(_) => NativeQueryCsvStatus::Failed("The file picker was unavailable.".into()),
        });
    });
}
