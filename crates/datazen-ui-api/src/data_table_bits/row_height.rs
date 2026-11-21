use crate::GridDensity;

pub const DATA_TABLE_ROW_HEIGHT: u32 = 28;

pub fn density_row_height_delta(density: GridDensity) -> i64 {
    match density {
        GridDensity::Compact => -6,
        GridDensity::Standard => 0,
        GridDensity::Relaxed => 8,
    }
}

pub fn row_height_for_density(base_row_height: i64, density: GridDensity) -> i64 {
    base_row_height + density_row_height_delta(density)
}
