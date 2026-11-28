//! Observe actual shell geometry while varying the window's real controls.

use datazen_ui::test_support::*;

fn rect(rt: &Runtime<AppState>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("shell key {key} is laid out"))
        .rect
}

fn settle(rt: &mut Runtime<AppState>) {
    for _ in 0..48 {
        rt.frame(1.0 / 60.0);
    }
}

fn assert_title_is_centered(rt: &Runtime<AppState>) {
    let [slot_x, _, slot_w, _] = rect(rt, "datazen-titlebar-title");
    let [label_x, _, label_w, _] = rect(rt, "datazen-titlebar-label");
    assert!((slot_x + slot_w * 0.5 - label_x - label_w * 0.5).abs() < 1.0);
}

#[test]
fn titlebar_follows_platform_insets_without_reserving_phantom_buttons() {
    let mut rt = runtime();
    settle(&mut rt);
    let left = rect(&rt, "rune-titlebar-left")[0];
    let right = rect(&rt, "datazen-theme-toggle")[0];
    assert_title_is_centered(&rt);

    let leading = rune::Chrome::DEFAULT.titlebar_inset();
    assert!(leading > 50.0, "legacy default emulates macOS traffic lights");
    // Native headless can still report a legacy Mac traffic-light inset
    // even when the per-platform chrome has no leading caption.
    rt.set_titlebar_inset(leading);
    settle(&mut rt);
    assert_eq!(rect(&rt, "rune-titlebar-left")[0], left);

    let mut platform = rune::ChromeInsets::default();
    platform.leading = leading;
    platform.trailing = 3.0 * rune::Titlebar::CAPTION_BUTTON_WIDTH;
    platform.height = 40.0;
    rt.set_chrome_insets(platform);
    settle(&mut rt);
    assert!((rect(&rt, "rune-titlebar-left")[0] - left - leading).abs() < 1.0);
    let [toggle_x, _, toggle_w, _] = rect(&rt, "datazen-theme-toggle");
    assert!(toggle_x < right);
    assert!((toggle_x + toggle_w - (1440.0 - platform.trailing - 2.0)).abs() < 1.0);
    assert_title_is_centered(&rt);
}

#[test]
fn settings_shows_its_original_localized_title_and_leading_back_button() {
    let mut rt = runtime();
    rt.state.open_settings(None);
    settle(&mut rt);
    assert_eq!(rect(&rt, "settings-back"), [12.0, 6.0, 28.0, 28.0]);
    let title = rt
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some("datazen-titlebar-label"))
        .expect("Settings title");
    assert_eq!(title.text.as_deref(), Some(datazen_ui::i18n::t("win.settings")));
    assert_title_is_centered(&rt);
}

// ConnectionPage's Back button uses surface tokens, not the menu's titlebar inks.
// Hover the button padding (outside the SVG): the entire hit target must tint it.
#[test]
fn settings_back_glyph_tracks_the_button_hover_in_both_themes() {
    for (dark, normal, hovered, radius) in [
        (false, "#58708dff", "#26364dff", 9.0),
        (true, "#8b94a7ff", "#e6eaf2ff", 10.0),
    ] {
        let mut rt = Runtime::new(
            AppState::default(),
            view,
            (1440.0, 900.0),
            Env { system_dark: dark, scale: 1.0 },
        );
        rt.state.open_settings(None);
        settle(&mut rt);
        let node = |rt: &Runtime<AppState>, key: &str| {
            rt.elements().into_iter()
                .find(|e| !e.exiting && e.key.as_deref() == Some(key))
                .unwrap_or_else(|| panic!("missing {key}"))
        };
        let back = node(&rt, "settings-back");
        assert!(back.focusable);
        assert_eq!(back.radius, radius);
        let initial = node(&rt, "settings-back-icon");
        assert_eq!(initial.rect, [18.0, 12.0, 16.0, 16.0]);
        assert_eq!(initial.text_color.unwrap().hex(), normal);

        let [x, y, _, _] = back.rect;
        rt.pointer_move(x + 3.0, y + 14.0);
        settle(&mut rt);
        assert_eq!(node(&rt, "settings-back-icon").text_color.unwrap().hex(), hovered);
        rt.pointer_move(x + 3.0, y + 40.0);
        settle(&mut rt);
        assert_eq!(node(&rt, "settings-back-icon").text_color.unwrap().hex(), normal);
    }
}
