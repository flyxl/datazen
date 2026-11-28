//! Display positions are independent of raw table indexes. Source rows, PKs,
//! staged edits, and cell/column keys never pass through this projection.
use super::{DataGrid, Selection};

impl DataGrid {
    pub(in crate::data_grid) fn displayed_count(&self) -> u32 {
        self.displayed.as_ref().map_or(self.cols, |columns| columns.len() as u32)
    }

    pub(in crate::data_grid) fn displayed_raw(&self, position: u32) -> Option<u32> {
        match &self.displayed {
            Some(columns) => columns.get(position as usize).copied(),
            None => (position < self.cols).then_some(position),
        }
    }

    pub(super) fn displayed_index(&self, raw: u32) -> Option<u32> {
        match &self.displayed {
            Some(columns) => columns.binary_search(&raw).ok().map(|index| index as u32),
            None => (raw < self.cols).then_some(raw),
        }
    }

    pub(super) fn displayed_range(&self, first: u32, last: u32) -> std::ops::Range<u32> {
        match &self.displayed {
            Some(columns) => {
                let start = columns.partition_point(|&raw| raw < first) as u32;
                let end = columns.partition_point(|&raw| raw <= last) as u32;
                start..end
            }
            None => first..last.saturating_add(1),
        }
    }

    pub(in crate::data_grid) fn display_revision(&self) -> u64 {
        self.display_revision
    }

    pub(in crate::data_grid) fn is_projected(&self) -> bool {
        self.displayed.is_some()
    }

    pub(super) fn displayed_width(&self, position: u32) -> f32 {
        self.displayed_raw(position).map_or(0.0, |raw| self.col_w(raw))
    }

    pub(in crate::data_grid) fn is_primary_key(&self, column: &str) -> bool {
        self.pk.iter().any(|key| key == column)
    }

    /// None is all columns, including new columns on a schema refresh.
    /// An explicit list retains schema order and does not renumber raw cells.
    pub(in crate::data_grid) fn set_displayed_columns(&mut self, names: Option<&[String]>) -> bool {
        let next = names.map(|names| {
            let names: std::collections::HashSet<_> = names.iter().map(String::as_str).collect();
            (0..self.cols).filter(|&raw| {
                self.column_name(raw).is_some_and(|name| names.contains(name))
            }).collect::<Vec<_>>()
        });
        if self.displayed == next {
            return false;
        }

        self.finish_cell_drag(true);
        self.displayed = next;
        self.display_revision = self.display_revision.wrapping_add(1);
        let focus_hidden = self.displayed_index(self.focus.1).is_none();
        if self.selection == Selection::Cells
            && (focus_hidden || self.displayed_index(self.anchor.1).is_none())
        {
            self.clear_selection();
        }
        if focus_hidden {
            // Keep the navigation coordinate usable without implicitly
            // selecting another cell. Row selection has separate identity.
            self.focus.1 = self.displayed_raw(0).unwrap_or(0);
            self.anchor = self.focus;
        }
        // Layout may clamp the real viewport on the next frame. Compute a
        // valid window at the observed offset now, even for a zero-column grid.
        self.origin_col = 0;
        self.sync_scroll();
        true
    }

    pub(in crate::data_grid) fn projected_key_reveal(
        &self, pinned: u32, gutter: f32, viewport: f32, current: f32,
    ) -> Option<f32> {
        if !self.is_projected() || viewport <= 0.0 {
            return None;
        }
        let position = self.displayed_index(self.focus.1)?;
        if position < pinned {
            return None;
        }
        let visible_left = if pinned > 0 { gutter + self.x_of_col(pinned) } else { 0.0 };
        let left = gutter + self.x_of_col(position);
        let right = left + self.displayed_width(position);
        if left < current + visible_left {
            Some((left - visible_left).max(0.0))
        } else if right > current + viewport {
            Some((right - viewport).max(0.0))
        } else {
            None
        }
    }
}
