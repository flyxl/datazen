//! Geometry of React's repeat(auto-fill, minmax(260px, 1fr)) with gap-3.
//! Width belongs to the grid, after the centered page's padding and shell.
pub(super) const GAP: f32 = 12.0;
const MIN: f32 = 260.0;

pub(super) fn tracks(measured: Option<f32>) -> (usize, f32) {
    let width = measured.filter(|w| w.is_finite()).unwrap_or(MIN).max(MIN);
    let columns = ((width + GAP) / (MIN + GAP)).floor() as usize;
    (columns, (width - (columns - 1) as f32 * GAP) / columns as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_survives_first_layout_and_narrow_panels() {
        for width in [None, Some(0.0), Some(f32::NAN), Some(190.0)] {
            assert_eq!(tracks(width), (1, 260.0));
        }
    }

    #[test]
    fn columns_change_only_when_an_entire_new_track_fits() {
        for (width, columns) in [(531.5, 1), (532.0, 2), (803.5, 2), (804.0, 3)] {
            assert_eq!(tracks(Some(width)).0, columns);
        }
    }

    #[test]
    fn centered_max_width_fills_three_equal_tracks_including_sparse_rows() {
        let (n, track) = tracks(Some(1024.0 - 48.0));
        assert_eq!(n, 3);
        assert!((track * n as f32 + GAP * (n - 1) as f32 - 976.0).abs() < 0.001);
        assert!(track > MIN);
    }
}
