//! Built-in syntax themes for the actual native SQL workspace.
use datazen_ui_api::{
    format_shortcut_for_display, EditorFontFamily, NativeExecuteShortcut, SqlSyntaxTheme, SQL_SYNTAX_PRESETS,
};
use rune::{col, text, Color, El, Span, TextStyle};
use rune_kit::Kit;

use super::{
    setting_hint::SettingHint,
    settings_content::{shortcut_card, shortcut_card_heading, shortcut_card_rows},
    settings_content_state::SettingsSelect,
    settings_ui::setting_row,
};
use crate::i18n::t;
use crate::components::sql_syntax_preview::live_sample_spans;
use crate::sql_editor::SyntaxPalette;
use crate::state_def::AppState;
use crate::ui::select::Select;

fn font_label(family: EditorFontFamily) -> &'static str {
    match family {
        EditorFontFamily::Bundled => t("settings.nativeEditorFont.bundled"),
        EditorFontFamily::System => t("settings.nativeEditorFont.system"),
    }
}

pub(super) fn font_family_row(state: &AppState, kit: &Kit, overlays: &mut Vec<El>) -> El {
    let label = t("settings.fontFamily");
    let which = SettingsSelect::EditorFontFamily;
    let select = Select::new("settings-editor-font-family", &EditorFontFamily::MODES.map(font_label))
        .values(&EditorFontFamily::MODES.map(EditorFontFamily::css_family))
        .current(Some(font_label(state.editor_font_family())))
        .label(label)
        .open(state.settings_content.open_select == Some(which))
        .on_toggle(move |state: &mut AppState| {
            if state.active == crate::types::Section::Settings && state.settings_section() == "editor" {
                state.settings_content.toggle_select(which);
            }
        })
        .on_close(move |state: &mut AppState| {
            if state.settings_content.open_select == Some(which) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, _tx| {
            if let Some(family) = EditorFontFamily::from_css_family(value) {
                state.save_editor_font_family(family);
            }
        });
    let field = editor_preference_trigger(select, &editor_field_kit(kit), overlays);
    setting_row(kit, overlays, "settings-editor-font-family-row".to_owned(), label, None, field)
}

const THEME_SELECT: &str = "settings-sql-syntax-theme";
const THEME_TRIGGER: &str = "settings-sql-syntax-theme-trigger";

pub(super) fn row(state: &AppState, dark: bool, kit: &Kit, overlays: &mut Vec<El>) -> El {
    let theme = state.sql_syntax_theme();
    let current = SQL_SYNTAX_PRESETS.iter().find(|preset| preset.id == theme.id())
        .expect("validated preset ID");
    let labels = SQL_SYNTAX_PRESETS.map(|preset| preset.label);
    let values = SQL_SYNTAX_PRESETS.map(|preset| preset.id);
    let label = t("settings.sqlSyntaxTheme");
    let open = state.settings_content.open_select == Some(SettingsSelect::SyntaxTheme);
    let select = Select::new(THEME_SELECT, &labels)
        .values(&values)
        .current(Some(current.label))
        .label(label)
        .open(open)
        .on_toggle(|state: &mut AppState| {
            if state.active == crate::types::Section::Settings && state.settings_section() == "editor" {
                state.settings_content.toggle_select(SettingsSelect::SyntaxTheme);
            }
        })
        .on_close(|state: &mut AppState| {
            if state.settings_content.open_select == Some(SettingsSelect::SyntaxTheme) {
                state.settings_content.close_select();
            }
        })
        .on_choose(|state: &mut AppState, value, _tx| {
            if let Some(theme) = SqlSyntaxTheme::from_id(value) {
                state.save_sql_syntax_theme(theme);
            }
        });
    let select = theme_select_trigger(select, kit, overlays, open);
    setting_row(
        kit,
        overlays,
        "settings-live-syntax-theme".to_owned(),
        label,
        Some(t("settings.sqlSyntaxThemeHint")),
        col().w_full().gap(8.0).child(select).child(preview(
            SyntaxPalette::preview(theme, dark),
            if dark { current.dark } else { current.light },
            state.editor_font_family(),
            dark,
            kit,
        )),
    )
}

/// Light Settings uses the reference menu's leading edge, 4px offset and
/// at least trigger-width option surface. Keep Rune's underlying selection,
/// scroll, focus restoration, and dismiss scrim rather than reimplement them.
fn theme_select_trigger(select: Select, kit: &Kit, overlays: &mut Vec<El>, open: bool) -> El {
    if kit.dark {
        return stock_select_trigger(select, kit, overlays);
    }
    let mut menu = Vec::new();
    let mut trigger = stock_select_trigger(select, kit, &mut menu);
    if open {
        // The pinned kit appends its anchored list after the optional scrim.
        if let Some(surface) = menu.pop() {
            let surface = surface.anchored_fit(THEME_TRIGGER, rune::Side::Bottom, rune::Align::Start, 4.0);
            let width = rune::memo::layout_with(THEME_TRIGGER, |rect| rect[2]);
            menu.push(match width {
                // All authored labels fit the full-width Settings field.
                Some(width) if width > 0.0 => surface.w(width).min_w(width).max_w(width),
                _ => surface,
            });
        }
    } else {
        // The reference uses a button. Rune's combobox already handles Space
        // and Up/Down; its app-owned plain Return needs an explicit binding.
        trigger = trigger.on_key(|state: &mut AppState, event, tx| {
            if event.key != rune::Key::Enter || event.mods != rune::Mods::default()
                || tx.focused() != Some(THEME_TRIGGER)
                || state.active != crate::types::Section::Settings
                || state.settings_section() != "editor"
                || state.settings_content.open_select.is_some()
            {
                return false;
            }
            if !tx.key_repeat() {
                state.settings_content.toggle_select(SettingsSelect::SyntaxTheme);
            }
            true
        });
    }
    overlays.extend(menu);
    trigger
}

/// The real Execute SQL action belongs to Customize Shortcuts. Mount
/// only actions with a working Settings writer and live keyboard consumer.
pub(super) fn execute_shortcut_card(state: &AppState, kit: &Kit, overlays: &mut Vec<El>) -> Option<El> {
    let binding = state.desktop.native_execute_shortcut()?;
    let label = t("keymap.action.execute");
    let which = SettingsSelect::ExecuteShortcut;
    let labels = ["Mod+Enter", "Mod+R"];
    let values = NativeExecuteShortcut::OPTIONS.map(NativeExecuteShortcut::wire);
    let selected = match binding {
        NativeExecuteShortcut::ModEnter => labels[0],
        NativeExecuteShortcut::ModR => labels[1],
    };
    let current_editor = |s: &AppState| {
        s.active == crate::types::Section::Settings && s.settings_section() == "editor"
    };
    let select = Select::new("settings-execute-shortcut", &labels)
        .values(&values).current(Some(selected)).label(label).width(128.0)
        .open(state.settings_content.open_select == Some(which))
        .on_toggle(move |s: &mut AppState| {
            if current_editor(s) && s.desktop.native_execute_shortcut().is_some() {
                s.settings_content.toggle_select(which);
            }
        })
        .on_close(move |s: &mut AppState| {
            if s.settings_content.open_select == Some(which) {
                s.settings_content.close_select();
            }
        })
        .on_choose(move |s: &mut AppState, value, _tx| {
            if !current_editor(s) || s.settings_content.open_select != Some(which) {
                return;
            }
            s.settings_content.close_select();
            if let Some(binding) = NativeExecuteShortcut::from_wire(value) {
                // The next render reads only the host's last committed value.
                s.settings_error = s.desktop.write_native_execute_shortcut(binding).err();
            }
        });
    // The native control is a picker, but its size and position are those
    // of the original keymap input, not a full-width Settings select.
    let mut picker_kit = syntax_field_kit(kit);
    picker_kit.ty.ui_compact = TextStyle::new(11.0, 400.0).lh(16.5).mono();
    let field = editor_preference_trigger(select, &picker_kit, overlays)
        .h(28.0).radius(4.0).pr(6.0);
    let display = format_shortcut_for_display(binding.wire(), cfg!(target_os = "macos"));
    let hint = SettingHint::new(
        "settings-execute-shortcut-row-hint", label, t("settings.nativeExecuteShortcut.hint"),
    ).build(kit, overlays);
    let execute = shortcut_action_row(
        kit, "settings-execute-shortcut-row", "settings-keymap-kbd-execute",
        label, hint, field, display,
    );
    let rows = shortcut_card_rows().child(execute).children(format_shortcut_row(state, kit, overlays));
    Some(shortcut_card(kit, shortcut_card_heading(kit), rows))
}

/// Keep transient edits local. Incomplete chords must not silently change a
/// working shortcut on disk or in a different open Query tab.
fn format_shortcut_row(state: &AppState, k: &Kit, overlays: &mut Vec<El>) -> Option<El> {
    use crate::ui::field::TextField;
    let binding = state.desktop.native_format_shortcut()?;
    let display = format_shortcut_for_display(&binding, cfg!(target_os = "macos"));
    let initial = rune::TextEdit::new(display.clone());
    let seed = initial.clone();
    let edit = state.settings_content.format_binding_draft.as_ref().unwrap_or(&initial);
    let label = t("keymap.action.formatSql");
    let field = TextField::new("settings-keymap-input-formatSql", edit)
        .label(label).height(28.0).width(128.0).pad_x(8.0).radius(4.0)
        .text_style(TextStyle::new(11.0, 400.0).mono())
        .border(1.0, k.c.border_base).fill(k.c.surface_base)
        .placeholder("e.g. Mod-Shift-F")
        .on_edit::<AppState>(move |s| s.settings_content.format_binding_draft.get_or_insert_with(|| seed.clone()))
        .on_key::<AppState>(|s, ev, _| {
            if ev.key == rune::Key::Escape {
                s.settings_content.format_binding_draft = None;
                s.settings_error = None;
                true
            } else { false }
        })
        .on_submit::<AppState>(|s, _| {
            if s.active != crate::types::Section::Settings || s.settings_section() != "editor" { return; }
            let Some(draft) = s.settings_content.format_binding_draft.as_ref() else { return };
            let Some(active) = s.desktop.native_format_shortcut() else { return };
            if !draft.text.is_empty() && draft.text == format_shortcut_for_display(&active, cfg!(target_os = "macos")) {
                s.settings_content.format_binding_draft = None;
                s.settings_error = None;
                return;
            }
            match normalize_format_binding(&draft.text) {
                Ok(binding) => {
                    s.settings_error = s.desktop.write_native_format_shortcut(binding.as_deref()).err();
                    if s.settings_error.is_none() { s.settings_content.format_binding_draft = None; }
                }
                Err(()) => {
                    // Text input callbacks are outside the app's render-locale scope.
                    let message = crate::i18n::with_language(s.builtin_language(), || {
                        t("settings.nativeFormatShortcut.invalid")
                    });
                    s.settings_error = Some(message.to_owned());
                }
            }
        })
        .build(k.dark);
    let hint = SettingHint::new("settings-format-shortcut-hint", label, t("settings.nativeFormatShortcut.hint")).build(k, overlays);
    Some(shortcut_action_row(
        k, "settings-keymap-row-formatSql", "settings-keymap-kbd-formatSql",
        label, hint, field, display,
    ))
}

fn shortcut_action_row(
    k: &Kit, key: &str, chip_key: &str, label: &str, hint: El, field: El, display: String,
) -> El {
    let chip = rune::row().key(chip_key.to_owned()).min_w(40.0)
        .px(7.0).py(3.0).radius(4.0).border(1.0, k.c.border_base).fill(k.c.surface_raised).center()
        .child(text(display).font(TextStyle::new(10.0, 400.0).mono()).color(k.c.text_faint));
    rune::row().key(key.to_owned()).w_full().gap(16.0).center_y().justify_between()
        .child(rune::row().key(format!("{key}-label")).gap(4.0).center_y().min_w(0.0).shrink()
            .child(text(label.to_owned()).font(TextStyle::new(12.0, 400.0).lh(16.0))
                .color(k.c.text_muted).min_w(0.0).shrink().wrap())
            .child(hint))
        .child(rune::row().gap(8.0).center_y().child(field).child(chip))
}

fn normalize_format_binding(input: &str) -> Result<Option<String>, ()> {
    if input.trim().is_empty() { return Ok(None); }
    if input.len() > 64 || input.trim().ends_with(['+', '-']) || input.chars().any(char::is_control) { return Err(()); }
    let normalized = datazen_ui_api::normalize_shortcut_input(input);
    let wire = datazen_ui_api::to_code_mirror_key_format(&normalized);
    if wire.len() > 64 || crate::native_query_format::parse_format_chord(&wire).is_none() {
        return Err(());
    }
    Ok(Some(wire))
}

/// The original setting uses a 1pt inside edge on a 36pt field, not
/// the generic Kit control's softer outside elevation.
fn editor_preference_trigger(select: Select, kit: &Kit, overlays: &mut Vec<El>) -> El {
    select
        .build(kit, overlays)
        .h(36.0)
        .radius(9.0)
        .border(1.0, kit.c.border_base)
        // Match the original px-2.5 plus its border; the inner Kit
        // value slot already contributes most of the leading inset.
        .pl(3.0)
        .pr(8.0)
}

fn editor_field_kit(kit: &Kit) -> Kit {
    let mut field = (*kit).clone();
    field.elevation = field.elevation.with_input(Vec::<rune::Shadow>::new());
    // The kit uses button-neutral as a fallback for an empty input list.
    field.elevation.button_neutral.clear();
    field.ty.ui_compact = TextStyle::new(14.0, 400.0).lh(20.0);
    field.c.icon_muted = kit.c.text_faint;
    field
}

// Chromium's Select paints the 14px Lucide chevron inside the trigger.
// Rune Kit's centered canvas is 16px; the paths below are the stock
// 24-unit Lucide paths scaled to 14px and inset 1px on this canvas.
static SYNTAX_CHEVRON_DOWN: rune::Icon = rune::Icon {
    name: "settings-syntax-chevron-down",
    view: 16.0,
    body: r#"<path d="M4.5 6.25L8 9.75L11.5 6.25" fill="none" stroke="currentColor" stroke-width="1.166667" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
static SYNTAX_CHEVRON_UP: rune::Icon = rune::Icon {
    name: "settings-syntax-chevron-up",
    view: 16.0,
    body: r#"<path d="M11.5 9.75L8 6.25L4.5 9.75" fill="none" stroke="currentColor" stroke-width="1.166667" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

pub(super) fn stock_select_trigger(select: Select, kit: &Kit, overlays: &mut Vec<El>) -> El {
    editor_preference_trigger(select, &syntax_field_kit(kit), overlays)
}

fn syntax_field_kit(kit: &Kit) -> Kit {
    let mut field = editor_field_kit(kit);
    field.glyph.chevron_down = &SYNTAX_CHEVRON_DOWN;
    field.glyph.chevron_up = &SYNTAX_CHEVRON_UP;
    field
}

fn preview_radius(dark: bool) -> f32 {
    // The original rounded-lg follows each stock appearance token.
    if dark { 12.0 } else { 11.0 }
}

fn preview(
    colors: SyntaxPalette,
    palette: datazen_ui_api::Palette,
    family: EditorFontFamily,
    dark: bool,
    kit: &Kit,
) -> El {
    let foreground = colors.foreground.unwrap_or(Color::BLACK);
    let (source, spans) = live_sample_spans(palette, foreground);
    // A horizontal viewport lays its direct children out side by side.
    // Give it one naturally sized, vertical code block, so the longest
    // line can scroll without turning the five lines into a filmstrip.
    let mut lines = col()
        .key("settings-sql-syntax-preview-content")
        .items_start()
        .px(16.0)
        .py(12.0);
    let mut start = 0usize;
    let font = TextStyle::new(12.0, 400.0).lh(20.4)
        .family(crate::sql_editor::query_font_family(family));
    for (index, line) in source.split('\n').enumerate() {
        let end = start + line.len();
        let row_spans: Vec<_> = spans.iter().filter_map(|span| {
            let lo = (span.start as usize).max(start);
            let hi = (span.end as usize).min(end);
            (lo < hi).then(|| {
                let mut relative = Span::new(lo - start, hi - start);
                relative.color = span.color;
                relative.weight = span.weight;
                relative.italic = span.italic;
                relative
            })
        }).collect();
        lines = lines.child(
            text(line)
                .key(format!("settings-sql-syntax-preview-line-{index}"))
                .font(font)
                .color(foreground)
                .spans(row_spans),
        );
        start = end + 1;
    }
    let radius = preview_radius(dark);
    col()
        .key("settings-sql-syntax-preview")
        .w_full()
        .radius(radius)
        .clip_rounded(radius)
        .border(1.0, kit.c.border_base)
        // CSS puts the five-line pre inside a 1px border box. Rune
        // paints the edge but does not add it to the child's quadrant.
        .padding(1.0)
        .fill(colors.background.unwrap_or(kit.c.bg_base))
        .child(
            col()
                .key("settings-sql-syntax-preview-scroll")
                .w_full()
                .scroll_x()
                .child(lines),
        )
}

#[cfg(test)]
mod visual_tests {
    use super::*;

    #[test]
    fn format_settings_uses_the_live_formatter_chord_and_empty_inherits() {
        assert_eq!(normalize_format_binding("Cmd + Option + F8"), Ok(Some("Mod-Alt-F8".into())));
        assert_eq!(normalize_format_binding("Ctrl+Shift+F"), Ok(Some("Ctrl-Shift-f".into())));
        assert_eq!(normalize_format_binding(""), Ok(None));
        for invalid in ["Mod+", "Mod+NotAKey", "Mod+F8\n", &"a".repeat(65)] {
            assert_eq!(normalize_format_binding(invalid), Err(()), "{invalid:?}");
        }
    }

    #[test]
    fn picker_sample_preserves_chromium_token_roles_for_all_live_palettes() {
        for preset in SQL_SYNTAX_PRESETS {
            for palette in [preset.light, preset.dark] {
                let (source, spans) = live_sample_spans(palette, Color::BLACK);
                for (word, role) in [
                    ("COUNT", "name"), ("name", "propertyName"), ("status", "propertyName"),
                    ("age", "propertyName"), ("id", "propertyName"), ("DESC", "keyword"),
                    ("ORDER BY", "keyword"), ("'active'", "string"),
                ] {
                    let offset = source.find(word).unwrap();
                    let span = spans.iter().find(|s| s.start as usize == offset).unwrap();
                    let (_, hex) = palette.entries.iter().find(|(name, _)| *name == role).unwrap();
                    assert_eq!(span.end as usize, offset + word.len());
                    assert_eq!(span.color, rune::color::hex(hex), "{} {word}", preset.id);
                    assert!(!span.italic);
                }
                assert!(spans[0].italic && spans[1].italic);
            }
        }
    }

    #[test]
    fn only_the_syntax_theme_field_uses_the_original_select_chevrons() {
        for dark in [false, true] {
            let shared = crate::ui::kit::kit(dark);
            let font_family = editor_field_kit(shared);
            let syntax = syntax_field_kit(shared);
            assert!(std::ptr::eq(font_family.glyph.chevron_down, shared.glyph.chevron_down));
            assert!(std::ptr::eq(font_family.glyph.chevron_up, shared.glyph.chevron_up));
            assert!(std::ptr::eq(syntax.glyph.chevron_down, &SYNTAX_CHEVRON_DOWN));
            assert!(std::ptr::eq(syntax.glyph.chevron_up, &SYNTAX_CHEVRON_UP));
            assert_eq!(syntax.c.icon_muted, font_family.c.icon_muted);
            assert_eq!(syntax.radius.md, font_family.radius.md);
        }
    }

    #[test]
    fn editor_skin_preserves_original_local_select_and_five_live_preview_lines() {
        for dark in [false, true] {
            let shared = crate::ui::kit::kit(dark);
            let trigger = syntax_field_kit(shared);
            assert!(trigger.elevation.input().is_empty());
            assert!(!shared.elevation.input().is_empty());
            assert_eq!(trigger.elevation.floating.len(), shared.elevation.floating.len());
            assert_eq!(trigger.c.border_focus, shared.c.border_focus);
            assert_eq!(trigger.c.icon_muted, shared.c.text_faint);

            fn view(dark: &bool, _: &mut rune::Cx<bool>) -> El {
                let preset = &SQL_SYNTAX_PRESETS[0];
                preview(
                    SyntaxPalette::preview(SqlSyntaxTheme::DEFAULT, *dark),
                    if *dark { preset.dark } else { preset.light },
                    EditorFontFamily::Bundled,
                    *dark,
                    crate::ui::kit::kit(*dark),
                )
            }
            let mut rt = rune::Runtime::new(dark, view, (496.0, 240.0), rune::Env::new(dark, 1.0));
            rt.frame(0.0);
            let outer = rt.presented("settings-sql-syntax-preview").unwrap().0;
            let inner = rt.presented("settings-sql-syntax-preview-scroll").unwrap().0;
            assert_eq!(inner[0] - outer[0], 1.0);
            assert_eq!(inner[1] - outer[1], 1.0);
            assert_eq!(outer[2] - inner[2], 2.0);
            for line in 0..5 {
                assert!(rt.presented(&format!("settings-sql-syntax-preview-line-{line}")).is_some());
            }
        }
    }
}
