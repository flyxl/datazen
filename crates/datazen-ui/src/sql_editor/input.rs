use rune::{Ime, Key, KeyEvent, Tx};

use super::{model::SqlEditor, navigation::Motion};

pub fn on_key(editor: &mut SqlEditor, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if !editor.focused() || editor.composition().is_some() {
        return false;
    }
    // Match the standard select-all shortcut in the custom editors, which
    // do not receive the platform text field's built-in selection handling.
    if ev.is_command('a') {
        editor.set_main_selection(0, editor.doc().len());
        editor.completion_close();
        return true;
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
            Key::Down if ev.mods == rune::Mods::default() => {
                editor.completion_down();
                return true;
            }
            Key::Up if ev.mods == rune::Mods::default() => {
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
    if let Some(motion) = navigation(ev) {
        editor.move_caret(motion, ev.mods.shift);
        tx.scroll_into_view("sql-caret");
        return true;
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
        Key::Backspace | Key::Delete => {
            if ev.key == Key::Backspace {
                editor.backspace();
            } else {
                editor.delete_forward();
            }
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

fn navigation(ev: &KeyEvent) -> Option<Motion> {
    let mods = ev.mods;
    // Mac Option / elsewhere Control moves by word horizontally; macOS
    // Command moves to a line edge. Do not take Alt+vertical or combined
    // modifiers reserved by line/multi-cursor commands.
    let line = mods.cmd && (!mods.ctrl || cfg!(target_os = "macos"));
    let word = mods.alt || (mods.ctrl && !cfg!(target_os = "macos"));
    if (mods.alt && (mods.cmd || mods.ctrl)) || (mods.logo && !mods.cmd)
        || (mods.ctrl && cfg!(target_os = "macos"))
    {
        return None;
    }
    match ev.key {
        Key::Left | Key::Right => {
            let right = ev.key == Key::Right;
            Some(if line {
                Motion::LineEdge(right)
            } else if word {
                Motion::Word(right)
            } else {
                Motion::Grapheme(right)
            })
        }
        Key::Up | Key::Down if !mods.alt && !mods.ctrl => Some(if line {
            Motion::DocumentEdge(ev.key == Key::Down)
        } else {
            Motion::Vertical(ev.key == Key::Down)
        }),
        Key::Home | Key::End if !mods.alt => Some(if mods.cmd || mods.ctrl {
            Motion::DocumentEdge(ev.key == Key::End)
        } else {
            Motion::LineEdge(ev.key == Key::End)
        }),
        _ => None,
    }
}

pub fn on_ime(editor: &mut SqlEditor, ev: &Ime) -> bool {
    editor.on_ime(ev)
}
