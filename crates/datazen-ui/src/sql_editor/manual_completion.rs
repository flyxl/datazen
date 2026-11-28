//! The explicit completion chord on events actually delivered by Rune.
//! No global shortcut, OS override, Window-menu chord or Option dead keys.
use rune::{Key, KeyEvent, Mods, PhysicalKeyCode, Tx};

pub(crate) fn is_manual_completion_key(event: &KeyEvent, tx: &Tx) -> bool {
    completion_chord(
        &event.key, event.mods, tx.physical_key(), cfg!(target_os = "macos"),
    )
}

fn completion_chord(key: &Key, mods: Mods, physical: Option<PhysicalKeyCode>, mac: bool) -> bool {
    if mods.alt || mods.shift {
        return false;
    }
    // Winit's cmd mirrors Control off macOS and Command on macOS.
    // Preserve CodeMirror's raw Control-Space option on macOS too.
    let exact_modifier = if mac {
        (mods.ctrl && !mods.cmd && !mods.logo) || (!mods.ctrl && (mods.cmd || mods.logo))
    } else {
        mods.ctrl && !mods.logo
    };
    exact_modifier && match physical {
        Some(code) => code == PhysicalKeyCode::Space,
        None => matches!(key, Key::Command(' ')) || matches!(key, Key::Text(text) if text == " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_completion_chord_keeps_platform_control_and_command_distinct() {
        let physical = Some(PhysicalKeyCode::Space);
        let space = Key::Text(" ".into());
        let control = Mods { ctrl: true, ..Mods::default() };
        let linux_primary = Mods { cmd: true, ..control };
        let mac_primary = Mods { cmd: true, logo: true, ..Mods::default() };
        assert!(completion_chord(&space, control, physical, true));
        assert!(completion_chord(&space, linux_primary, physical, false));
        assert!(completion_chord(&space, mac_primary, physical, true));
        assert!(!completion_chord(&space, mac_primary, physical, false));
        assert!(!completion_chord(&space, Mods { ctrl: true, ..mac_primary }, physical, true));
        for extra in [
            Mods::default(), Mods { shift: true, ..control }, Mods { alt: true, ..control },
            Mods { alt: true, ..Mods::default() },
        ] {
            assert!(!completion_chord(&space, extra, physical, true));
            assert!(!completion_chord(&space, extra, physical, false));
        }
        assert!(!completion_chord(&space, control, Some(PhysicalKeyCode::KeyI), true));
        assert!(completion_chord(&Key::Command(' '), linux_primary, None, false));
    }
}
