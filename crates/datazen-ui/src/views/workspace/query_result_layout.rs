//! The host for the live Query Table result, shared by automatic/pinned layout.
use crate::{data_grid, AppState};
use rune::{col, El};

pub(super) const KEY: &str = "query-results-host";
const FRAME_KEY: &str = "result-data-grid-scroll-scroll-area";
const MIN_ESTIMATE: f32 = 140.0;

/// Keep the same result/tab/viewport path when dragging changes the sizing
/// policy. Otherwise Rune gives the grid new identities for scroll and input.
pub(super) fn table_host(
    state: &AppState,
    grid: &data_grid::DataGrid,
    dark: bool,
    automatic: bool,
    tabs: El,
    body: Option<El>,
) -> El {
    let frame = rune::memo::layout_size(FRAME_KEY);
    // The difference includes the genuine tab strip, status, and optional
    // Find/export/truncation controls. Measure the outer sheet before gutters;
    // reading the scroll viewport here would feed scrollbar height back in.
    let chrome = rune::memo::layout_size(KEY)
        .zip(frame)
        .map_or(0.0, |([_, host], [_, sheet])| (host - sheet).max(0.0));
    let host = col().key(KEY).w_full().min_h(0.0);
    let host = if automatic {
        // Match the original's cap on the results as a whole, measured
        // against the query column rather than the editor or application.
        let cap = rune::memo::layout_with("query-main", |rect| rect[3] * 0.5);
        let width = frame.map(|[width, _]| width)
            .or_else(|| rune::memo::layout_with("query-main", |rect| rect[2]))
            .unwrap_or(f32::INFINITY);
        let grid_height = data_grid::query_result_auto_grid_height(state, grid, dark, width);
        host.h(automatic_height(chrome, grid_height, cap)).shrink()
    } else {
        let floor = (chrome + data_grid::query_result_viewport_floor(state, dark)).max(MIN_ESTIMATE);
        host.grow(1.0).min_h(floor)
    };
    host.child(tabs).children(body)
}

fn automatic_height(chrome: f32, grid: f32, cap: Option<f32>) -> f32 {
    (chrome + grid).max(MIN_ESTIMATE).min(cap.unwrap_or(MIN_ESTIMATE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_result_uses_content_below_half_and_half_beats_the_floor() {
        // Default chrome is not compiled into the estimator: it is measured.
        assert_eq!(automatic_height(100.0, 60.0, Some(362.0)), 160.0);
        assert_eq!(automatic_height(100.0, 124.0, Some(362.0)), 224.0);
        assert_eq!(automatic_height(100.0, 3100.0, Some(362.0)), 362.0);
        assert_eq!(automatic_height(20.0, 60.0, Some(362.0)), 140.0);
        assert_eq!(automatic_height(20.0, 60.0, Some(72.0)), 72.0);
        assert_eq!(automatic_height(100.0, 3100.0, None), 140.0);
    }
}
