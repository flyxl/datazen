//! React `components/chart` port (Wave 2).
//!
//! `ChartCanvas.tsx` (T008) is a thin dispatcher over `recharts`'
//! `ResponsiveContainer`; the chart engines are separate migration tasks, so
//! that module ships the dispatcher plus a `ChartRenderer` seam and honest
//! placeholders. `ChartToolbar.tsx` (T059) is the strip above the chart — the
//! type switch, the display toggles and the export dropdown — and
//! `ChartEmptyState.tsx` (T059) is the centered nothing-to-chart pane. The
//! config/VM types the three share live with the dispatcher in
//! `chart_canvas.rs`.
//!
//! `ChartView.tsx` (T102) is the family's container: it mounts the T059
//! parts, owns the config derivation, the row cap, the split tiles and the
//! expanded overlay. Its own production consumer (React mounts it from
//! `ResultWorkspace`, the workflow execution panel and `ServerStatusView`)
//! is a later migration track, so the module-level allow moves here — and
//! an `#[allow(dead_code)]` item also keeps the parts it mounts live for the
//! lint, which is why the T059 modules no longer need their own.
//!
//! `LogScaleTooltip.tsx` (T136) is the log-scale tooltip half: the custom
//! recharts `<Tooltip>` formatter that reads `__orig__${name}` back
//! (the slots `mapToLogScale` stashes) plus the `contentStyle` chrome — the
//! five chart-engine tasks mount the content box it builds.
//!
//! `renderers/` is the engine directory (one module per React file):
//! `AreaChartRenderer.tsx` (T166) is the first engine on the `ChartRenderer`
//! seam — the log-hint pipeline, the area/grid/axis shapes, the legend toggle
//! and the tooltip mount — and the canvas's renderer factory replaces the
//! Area placeholder with it.
#[allow(dead_code)]
pub(crate) mod chart_view;
pub(crate) mod chart_canvas;
pub(crate) mod chart_empty_state;
pub(crate) mod chart_toolbar;
pub(crate) mod log_scale_tooltip;
pub(crate) mod renderers;
