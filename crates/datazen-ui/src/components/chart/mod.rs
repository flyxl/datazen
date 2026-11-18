//! React `components/chart` port (Wave 2, T008).
//!
//! `ChartCanvas.tsx` is a thin dispatcher over `recharts`' `ResponsiveContainer`.
//! The chart engines themselves are separate migration tasks, so this module
//! ships the dispatcher plus a `ChartRenderer` seam and an honest placeholder
//! renderer for each of the five chart types. See `chart_canvas.rs`.

pub(crate) mod chart_canvas;
