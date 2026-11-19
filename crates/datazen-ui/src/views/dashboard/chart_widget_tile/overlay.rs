//! The `TileExpandOverlay` half of the T069 `ChartWidgetTile` port
//! (`packages/frontend/src/windows/dashboard/ChartWidgetTile.tsx`, the
//! `createPortal(document.body)` subtree): React's near-full-window overlay
//! — `fixed inset-0 top-10 z-50 bg-surface/95 backdrop-blur-sm` — with the
//! title header, the collapse button, the axis configurator, and the large
//! chart or table.
//!
//! The gate is React's `expanded && run?.status === 'ok'`; the collapse
//! button is the only close path (no scrim, no Escape — same as React). The
//! `AxisConfigurator` is the T319 host-fed slot and the overlay's chart data
//! is the T030/T182 transform seam, both documented in the parent module.

use std::rc::Rc;

use rune::{col, el, icon, row, text, El, TextStyle};
use rune_kit::Kit;

use super::{
    has_renderable_chart, run_fields, table_grid, ChartWidgetTile, TileViewMode, WidgetRunStatus,
    ICON_SIZE, OVERLAY_ALPHA, OVERLAY_HEADER_GAP, OVERLAY_HEADER_PX, OVERLAY_HEADER_PY,
    OVERLAY_KEY, OVERLAY_TOP,
};
use crate::components::chart::chart_canvas::ChartCanvas;
use crate::i18n::t;
use crate::icons::ICON_MINIMIZE_2;

impl<'a, S: 'static> ChartWidgetTile<'a, S> {
    /// React's `TileExpandOverlay`: `expanded && run?.status === 'ok'` gates
    /// the portal; the pinned layer element is the `fixed inset-0 top-10`.
    pub(super) fn overlay(&self, k: &Kit, dark: bool, layer: &mut Vec<El>) {
        let Some(run) = self.run else { return };
        if !self.state.expanded || !matches!(run.status, WidgetRunStatus::Ok) {
            return;
        }

        let header = row()
            .key(format!("{OVERLAY_KEY}-header"))
            .w_full()
            .items_center()
            .gap(OVERLAY_HEADER_GAP)
            .px(OVERLAY_HEADER_PX)
            .py(OVERLAY_HEADER_PY)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                text(self.widget.title.clone())
                    .key(format!("{OVERLAY_KEY}-title"))
                    .font(TextStyle::new(14.0, 500.0)) // text-sm font-medium
                    .color(k.c.text_base)
                    .truncate(),
            )
            .child(el().key(format!("{OVERLAY_KEY}-spacer")).grow(1.0).min_w(0.0))
            .child(self.collapse_button(k));

        let mut body_row = row()
            .key(format!("{OVERLAY_KEY}-body"))
            .w_full()
            .grow(1.0)
            .min_h(0.0);

        let fields = run_fields(run);
        if matches!(self.widget.view_mode, TileViewMode::Chart) && !fields.is_empty() {
            // React's `AxisConfigurator` gate; the slot is the T319 seam and
            // paints nothing until the host supplies it.
            if let Some(slot) = self.axis_configurator.clone() {
                body_row = body_row.child(slot);
            }
        }

        let overlay_data = match self.overlay_chart_data {
            Some(data) => Some(data),
            None => {
                // React recomputes `widgetRunToChartData(run, localConfig)`;
                // while the local config is the widget's, that is the tile's
                // data, so the seam's one delivery serves both.
                if self.widget.chart_config.as_ref() == Some(&self.state.local_config) {
                    self.chart_data
                } else {
                    None
                }
            }
        };
        let overlay_has_chart = has_renderable_chart(overlay_data, &self.state.local_config);

        let content = match self.widget.view_mode {
            TileViewMode::Chart => {
                if overlay_has_chart {
                    // `overlay_has_chart` implies the data slice is present.
                    if let Some(data) = overlay_data {
                        col()
                            .key(format!("{OVERLAY_KEY}-content"))
                            .w_full()
                            .grow(1.0)
                            .min_w(0.0)
                            .min_h(0.0)
                            .child(
                                ChartCanvas::<S>::new(&self.state.local_config, data)
                                    .dark(dark)
                                    .build(),
                            )
                    } else {
                        overlay_empty_note(
                            k,
                            if run.row_count == 0 {
                                t("dashboard.emptyResult")
                            } else {
                                t("dashboard.chartNotConfigured")
                            },
                        )
                    }
                } else {
                    overlay_empty_note(
                        k,
                        if run.row_count == 0 {
                            t("dashboard.emptyResult")
                        } else {
                            t("dashboard.chartNotConfigured")
                        },
                    )
                }
            }
            TileViewMode::Table => {
                if run.rows.is_empty() {
                    // React's table-empty branch is `emptyResult` alone.
                    overlay_empty_note(k, t("dashboard.emptyResult"))
                } else {
                    col()
                        .key(format!("{OVERLAY_KEY}-content"))
                        .w_full()
                        .grow(1.0)
                        .min_w(0.0)
                        .min_h(0.0)
                        .child(table_grid(run, OVERLAY_KEY, dark))
                }
            }
        };
        body_row = body_row.child(content);

        layer.push(
            col()
                .key(OVERLAY_KEY)
                .pin(Some(0.0), Some(OVERLAY_TOP), Some(0.0), Some(0.0))
                .fill(k.alpha(k.c.bg_base, OVERLAY_ALPHA)) // bg-surface/95
                .child(header)
                .child(body_row),
        );
    }

    /// The overlay header's collapse button (React's only close path).
    fn collapse_button(&self, k: &Kit) -> El {
        let label = t("dashboard.collapse");
        let mut button = row()
            .key(format!("{OVERLAY_KEY}-collapse"))
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .gap(4.0)
            .px(8.0)
            .py(4.0)
            .radius(k.radius.sm)
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus)
            .hover_group()
            .hover(k.c.hover)
            .child(
                icon(&ICON_MINIMIZE_2, ICON_SIZE)
                    .key(format!("{OVERLAY_KEY}-collapse-icon"))
                    .color(k.c.text_muted)
                    .group_color(k.c.text_weak),
            );
        if let Some(on_collapse) = self.on_collapse.as_ref() {
            let on_collapse = Rc::clone(on_collapse);
            button = button.on_press(move |s: &mut S| on_collapse(s));
        }
        button
    }
}

/// The overlay's centered empty note (chart-unconfigured or empty table).
fn overlay_empty_note(k: &Kit, message: &str) -> El {
    col()
        .key(format!("{OVERLAY_KEY}-empty"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .child(
            text(message)
                .key(format!("{OVERLAY_KEY}-empty-text"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap(),
        )
}
