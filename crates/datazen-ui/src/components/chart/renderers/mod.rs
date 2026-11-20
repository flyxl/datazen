//! React `components/chart/renderers/` port — the five chart engines, one
//! module per React file at the same path. The engines are the migration
//! tasks the canvas dispatcher (`chart_canvas.rs`) has been waiting on; each
//! lands here and replaces its placeholder through the renderer factory.
//!
//! `AreaChartRenderer.tsx` (T166) is the first: `renderers/area_chart_renderer.rs`.
//! `BarChartRenderer.tsx` (T192) is the second: `renderers/bar_chart_renderer.rs`.
//! `LineChartRenderer.tsx` (T216) is the third: `renderers/line_chart_renderer.rs`.
//! `PieChartRenderer.tsx` (T238) is the fourth: `renderers/pie_chart_renderer.rs`.
//! `ScatterChartRenderer.tsx` (T261) is the fifth: `renderers/scatter_chart_renderer.rs`.

pub(crate) mod area_chart_renderer;
pub(crate) mod bar_chart_renderer;
pub(crate) mod line_chart_renderer;
pub(crate) mod pie_chart_renderer;
pub(crate) mod scatter_chart_renderer;
