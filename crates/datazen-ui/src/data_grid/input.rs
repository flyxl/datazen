use rune::{Key, KeyEvent, Tx};

use super::DataGrid;

pub fn on_key(grid: &mut DataGrid, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if grid.editing {
        return false;
    }
    if ev.is_command('c') {
        tx.copy(grid.copy_tsv());
        return true;
    }
    let extend = ev.mods.shift;
    match ev.key {
        Key::Left => grid.nudge(0, -1, extend),
        Key::Right => grid.nudge(0, 1, extend),
        Key::Up => grid.nudge(-1, 0, extend),
        Key::Down => grid.nudge(1, 0, extend),
        // C1: a full loaded page of rows one way, clamped to the boundary.
        Key::PageDown => grid.nudge(grid.rows as i32, 0, extend),
        Key::PageUp => grid.nudge(-(grid.rows as i32), 0, extend),
        // C1: the row's first and last column.
        Key::Home => grid.nudge(0, -(grid.cols as i32), extend),
        Key::End => grid.nudge(0, grid.cols as i32, extend),
        Key::Enter => grid.begin_edit(),
        _ => return false,
    }
    true
}
