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

// T104 (the ChartView consumer) is the production caller for the T059 parts;
// until it lands, the lib target sees them as unreferenced. Delete both
// allows when the view mounts them.
#[allow(dead_code)]
pub(crate) mod chart_empty_state;
#[allow(dead_code)]
pub(crate) mod chart_toolbar;
pub(crate) mod chart_canvas;
