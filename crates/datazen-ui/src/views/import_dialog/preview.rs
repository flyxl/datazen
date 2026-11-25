//! Result surfaces of the import wizard (React `ImportDialog.tsx` lines
//! 756–926): the first-rows preview table, the live progress line and the
//! finished-run report (completed / stopped / cancelled, per-row failures and
//! the refresh-failed notice).

use rune::{col, rich, row, text, El, Span, TextStyle};

use super::{ImportDialogVm, ImportRunResult, PREVIEW_ROWS, ROOT, RunStatus};
use crate::i18n::t;
use crate::icons::ICON_X;
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;

/// React `parsed.rows.length > 0` preview section. NULL cells are italic and
/// muted; object/array cells keep React's `JSON.stringify` text (the host
/// stores it as [`super::CellValue::Json`]).
pub fn build_preview(vm: &ImportDialogVm, dark: bool) -> El {
    let Some(parsed) = vm.parsed.as_ref() else {
        return row().key(format!("{ROOT}-no-preview"));
    };
    if parsed.rows.is_empty() {
        return row().key(format!("{ROOT}-no-preview"));
    }
    let k = kit(dark);
    let count = PREVIEW_ROWS.min(parsed.rows.len());
    let label = t("import.previewFirstRows")
        .replace("{count}", &count.to_string())
        .replace("{total}", &parsed.rows.len().to_string());

    let mut table = col()
        .key("import-preview")
        .w_full()
        .min_w(0.0)
        .border(1.0, k.c.border_muted)
        .radius(6.0);
    let mut header = row()
        .key(format!("{ROOT}-preview-header"))
        .w_full()
        .gap(8.0);
    for column in &parsed.columns {
        header = header.child(
            text(column.clone())
                .grow(1.0)
                .min_w(0.0)
                .font(TextStyle::new(12.0, 600.0))
                .color(k.c.text_weak),
        );
    }
    table = table.child(header);
    for (r, values) in parsed.rows.iter().take(PREVIEW_ROWS).enumerate() {
        let mut body = row()
            .key(format!("{ROOT}-preview-row-{r}"))
            .w_full()
            .gap(8.0);
        for c in 0..parsed.columns.len() {
            let cell = values.get(c).cloned().unwrap_or(super::CellValue::Null);
            let content = if cell.is_null() {
                rich("NULL", vec![Span::new(0, 4).italic()])
            } else {
                text(cell.display())
            }
            .key(format!("{ROOT}-preview-cell-{r}-{c}"))
            .grow(1.0)
            .min_w(0.0)
            .wrap()
            .color(k.c.text_weak);
            body = body.child(content);
        }
        table = table.child(body);
    }
    col()
        .key(format!("{ROOT}-preview-section"))
        .w_full()
        .min_w(0.0)
        .gap(6.0)
        .child(text(label).color(k.c.text_weak).wrap())
        .child(table)
}

/// React `busy && progress` — shown only while running.
pub fn build_progress(vm: &ImportDialogVm, dark: bool) -> Option<El> {
    if vm.phase != super::WizardPhase::Running {
        return None;
    }
    let progress = vm.progress.as_ref()?;
    let k = kit(dark);
    let message = t("import.progress")
        .replace("{processed}", &progress.processed_rows.to_string())
        .replace("{committed}", &progress.committed_rows.to_string())
        .replace("{failed}", &progress.failed_rows.to_string());
    Some(
        col()
            .key("import-progress")
            .w_full()
            .min_w(0.0)
            .border(1.0, k.c.border_muted)
            .radius(6.0)
            .child(text(message).key(format!("{ROOT}-progress-text")).color(k.c.text_base).wrap()),
    )
}

/// React `phase === 'done' && result` — the honest run report.
pub fn build_result<S: 'static>(vm: &ImportDialogVm, dark: bool) -> Option<El> {
    if vm.phase != super::WizardPhase::Done {
        return None;
    }
    let result = vm.result.as_ref()?;
    let k = kit(dark);
    let mut section = col()
        .key("import-result")
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .child(status_banner::<S>(result, dark));

    if !result.failed_rows.is_empty() {
        let label = t("import.result.failedRows")
            .replace("{count}", &result.failed_rows.len().to_string());
        let mut table = col()
            .key(format!("{ROOT}-failed-rows"))
            .w_full()
            .min_w(0.0)
            .border(1.0, k.c.border_muted)
            .radius(6.0)
            .child(
                row()
                    .w_full()
                    .gap(8.0)
                    .child(text("#").w(32.0).color(k.c.text_weak))
                    .child(text(t("import.sourceLine")).w(96.0).color(k.c.text_weak))
                    .child(text("Error").grow(1.0).min_w(0.0).color(k.c.text_weak)),
            );
        for failure in &result.failed_rows {
            table = table.child(
                row()
                    .key(format!("{ROOT}-failure-{}", failure.row_index))
                    .w_full()
                    .gap(8.0)
                    .child(text((failure.row_index + 1).to_string()).w(32.0))
                    .child(
                        text(failure.source_line.map_or_else(|| "—".to_owned(), |line| line.to_string()))
                            .w(96.0),
                    )
                    .child(text(failure.message.clone()).grow(1.0).min_w(0.0).color(k.c.danger.fg)),
            );
        }
        section = section.child(text(label).color(k.c.text_weak).wrap()).child(table);
    }

    if vm.refresh_failed {
        section = section.child(warning(t("import.refreshFailed").to_owned(), dark));
    }
    Some(section)
}

fn status_banner<S: 'static>(result: &ImportRunResult, dark: bool) -> El {
    let k = kit(dark);
    match result.status {
        RunStatus::Completed => tinted(
            format!("{ROOT}-result-completed"),
            t("import.result.completed").replace("{committed}", &result.committed_rows.to_string()),
            k.c.success.fg,
        ),
        RunStatus::Cancelled => tinted(
            format!("{ROOT}-result-cancelled"),
            t("import.result.cancelled").replace("{committed}", &result.committed_rows.to_string()),
            k.c.warning.fg,
        ),
        RunStatus::Stopped => {
            let message = t("import.result.stopped")
                .replace("{committed}", &result.committed_rows.to_string())
                .replace(
                    "{line}",
                    &result
                        .stopped_by
                        .as_ref()
                        .and_then(|stop| stop.source_line)
                        .map_or_else(|| "—".to_owned(), |line| line.to_string()),
                )
                .replace(
                    "{message}",
                    &result
                        .stopped_by
                        .as_ref()
                        .map_or_else(String::new, |stop| stop.message.clone()),
                );
            CopyableError::new(format!("{ROOT}-result-stopped"), message).build::<S>(&k)
        }
    }
}

fn tinted(key: String, message: String, ink: rune::Color) -> El {
    col()
        .key(key.clone())
        .w_full()
        .min_w(0.0)
        .child(text(message).key(format!("{key}-text")).color(ink).wrap())
}

/// React's amber refresh / ragged notice. The upstream AlertTriangle glyph is
/// not in the registry; a registry X stands in until the icon task adds it.
fn warning(message: String, dark: bool) -> El {
    let k = kit(dark);
    row()
        .key(format!("{ROOT}-refresh-failed"))
        .w_full()
        .gap(8.0)
        .center_y()
        .border(1.0, k.c.warning.border)
        .radius(6.0)
        .child(rune::icon(&ICON_X, 14.0).color(k.c.warning.icon))
        .child(text(message).key(format!("{ROOT}-refresh-failed-text")).color(k.c.warning.fg).wrap())
}
