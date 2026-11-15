use rune::{Ime, Key, KeyEvent, Tx};

use super::model::SqlEditor;

pub fn on_key(editor: &mut SqlEditor, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if !editor.focused() || editor.composition().is_some() {
        return false;
    }
    // T1: the system clipboard vocabulary. Copy and cut answer the selection;
    // paste asks the shell for the clipboard and the app root's on_paste
    // lands it in the editor (no platform text field is focused here).
    if ev.is_command('c') {
        let text = editor.selected_text();
        if text.is_empty() {
            return false;
        }
        tx.copy(text);
        return true;
    }
    if ev.is_command('x') {
        let text = editor.selected_text();
        if text.is_empty() {
            return false;
        }
        tx.copy(text);
        editor.cut_selection();
        return true;
    }
    if ev.is_command('v') {
        tx.request_paste();
        return true;
    }
    if editor.has_completion() {
        match &ev.key {
            Key::Down => {
                editor.completion_down();
                return true;
            }
            Key::Up => {
                editor.completion_up();
                return true;
            }
            Key::Enter if !ev.mods.cmd && !ev.mods.shift => {
                editor.completion_accept();
                return true;
            }
            Key::Escape => {
                editor.completion_close();
                return true;
            }
            _ => {}
        }
    }
    match &ev.key {
        Key::Text(text) if !ev.mods.cmd && !ev.mods.ctrl => {
            editor.insert(text);
            editor.refresh_completion();
            true
        }
        Key::Enter => {
            editor.insert("\n");
            editor.completion_close();
            true
        }
        Key::Backspace => {
            editor.backspace();
            editor.refresh_completion();
            true
        }
        Key::Command('z') if ev.mods.shift => {
            editor.redo();
            true
        }
        Key::Command('z') => {
            editor.undo();
            true
        }
        Key::Command('y') => {
            editor.redo();
            true
        }
        _ => false,
    }
}

pub fn on_ime(editor: &mut SqlEditor, ev: &Ime) -> bool {
    editor.on_ime(ev)
}
