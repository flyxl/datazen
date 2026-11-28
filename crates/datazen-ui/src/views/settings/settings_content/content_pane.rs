//! Centered Settings body and overflow-dependent scrollbar gutter.
use rune::{col, El};
use rune_kit::Kit;

use super::{scrollbar, BODY_MAX_WIDTH};

// Keep the same centered, width-capped region for host-wired pages and
// pages whose fields are rendered by SettingsContent itself.
pub(super) fn settings_content_pane(k: &Kit, section: &str, body: El, layer: Vec<El>) -> El {
    const VIEWPORT: &str = "settings-content";
    const PADDING_X: f32 = 32.0;
    const PADDING_Y: f32 = 24.0;
    let body_key = format!("settings-scrolling-body-{section}");
    // These reads settle with layout and do not subscribe to scroll position.
    // A section-specific body key cannot reuse another route's old height.
    let overflowing = match (
        rune::memo::layout_with(VIEWPORT, |rect| rect[3]),
        rune::memo::layout_with(&body_key, |rect| rect[3]),
    ) {
        (Some(viewport_height), Some(body_height)) => body_height > viewport_height,
        _ => false,
    };
    let viewport = col()
        .key(VIEWPORT)
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .h_full()
        .items_center()
        .px(PADDING_X)
        .pr(PADDING_X + if overflowing { scrollbar::STRIP } else { 0.0 })
        .scroll(rune::ScrollSpec { stick_bottom: false })
        // Rune's scroll extent ends at its last content box. Reserve the
        // actual bottom padding in that box so it remains visible at the end.
        .child(
            col()
                .key(body_key)
                .w_full()
                .max_w(BODY_MAX_WIDTH)
                .py(PADDING_Y)
                .child(body),
        )
        .children(layer);
    scrollbar::attach(viewport, k)
}

#[cfg(test)]
mod settings_content_overflow_tests {
    use super::*;
    use crate::ui::kit::kit;
    use rune::{row, Cx, Direction, Env, Runtime};

    #[derive(Clone, Copy)]
    struct Fixture {
        short: bool,
        rtl: bool,
        dark: bool,
    }

    fn view(state: &Fixture, _: &mut Cx<Fixture>) -> El {
        let (section, height) = if state.short {
            ("appearance", 192.5)
        } else {
            ("prompts", 1827.5)
        };
        let direction = if state.rtl { Direction::Rtl } else { Direction::Ltr };
        col()
            .w_full()
            .h_full()
            .dir(direction)
            .child(col().w_full().h(40.0).shrink())
            .child(
                row()
                    .w_full()
                    .grow(1.0)
                    .min_h(0.0)
                    .child(col().w(180.0).h_full())
                    .child(settings_content_pane(
                        kit(state.dark),
                        section,
                        col().key("fixture-body").w_full().h(height),
                        Vec::new(),
                    )),
            )
    }

    fn rect(rt: &Runtime<Fixture>, key: &str) -> [f32; 4] {
        rt.elements().into_iter().find(|element| element.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing {key}"))
            .rect
    }

    #[test]
    fn gutter_tracks_actual_overflow_on_first_layout_resize_and_section_switch() {
        for dark in [false, true] {
            let mut rt = Runtime::new(
                Fixture { short: false, rtl: false, dark },
                view,
                (1440.0, 900.0),
                Env::new(dark, 1.0),
            );
            rt.frame(0.0);
            assert_eq!(rect(&rt, "settings-content"), [180.0, 40.0, 1260.0, 860.0]);
            assert_eq!(rect(&rt, "fixture-body"), [469.5, 64.0, 672.0, 1827.5]);

            rt.update(|_, tx| tx.scroll_to_bottom_instant("settings-content"));
            rt.frame(1.0 / 60.0);
            let [x, y, _, h] = rect(&rt, "fixture-body");
            assert_eq!(x, 469.5, "scroll position does not recenter the body");
            assert_eq!(y + h, 876.0, "24 points of real bottom padding at the scroll end");

            rt.set_viewport((1440.0, 2000.0), 1.0);
            rt.frame(1.0 / 60.0);
            assert_eq!(rect(&rt, "fixture-body"), [474.0, 64.0, 672.0, 1827.5]);

            rt.set_viewport((1440.0, 900.0), 1.0);
            rt.frame(1.0 / 60.0);
            assert_eq!(rect(&rt, "fixture-body"), [469.5, 64.0, 672.0, 1827.5]);

            rt.update(|state, _| state.short = true);
            rt.frame(1.0 / 60.0);
            assert_eq!(rect(&rt, "fixture-body"), [474.0, 64.0, 672.0, 192.5]);
        }
    }

    #[test]
    fn classic_thumb_paints_at_idle_and_drags_the_real_settings_viewport() {
        let thumb = "settings-content-scrollbar-thumb";
        let mut rt = Runtime::new(
            Fixture { short: false, rtl: false, dark: true },
            view,
            (1440.0, 900.0),
            Env::new(true, 1.0),
        );
        rt.set_system_scroll_bars(rune::ScrollBars::WhenScrolling);
        rt.set_system_scroller_jumps(false);
        rt.pointer_move(300.0, 300.0);
        rt.frame(0.0);
        assert_eq!(rect(&rt, thumb), [1433.0, 42.0, 5.0, 390.0]);

        rt.pointer_move(1435.0, 48.0);
        rt.pointer_down(1435.0, 48.0);
        rt.pointer_move(1435.0, 600.0);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(1435.0, 600.0);
        rt.frame(1.0 / 60.0);
        let [_, y, _, h] = rect(&rt, "fixture-body");
        assert_eq!(y + h, 876.0, "dragging the thumb scrolls real content");
        assert_eq!(rect(&rt, thumb), [1433.0, 508.0, 5.0, 390.0]);

        rt.pointer_move(1435.0, 45.0);
        rt.pointer_down(1435.0, 45.0);
        rt.pointer_up(1435.0, 45.0);
        rt.frame(1.0 / 60.0);
        assert!(
            (y..64.0).contains(&rect(&rt, "fixture-body")[1])
                && rect(&rt, "fixture-body")[1] > y,
            "pressing the track pages toward the start",
        );

        rt.set_system_scroller_jumps(true);
        rt.frame(1.0 / 60.0);
        rt.pointer_down(1435.0, 45.0);
        rt.pointer_up(1435.0, 45.0);
        rt.frame(1.0 / 60.0);
        assert_eq!(rect(&rt, "fixture-body")[1], 64.0, "jump preference is retained");

        rt.update(|state, _| state.short = true);
        rt.frame(1.0 / 60.0);
        assert_eq!(rect(&rt, thumb)[2..], [0.0, 0.0], "a fitting body has no thumb");
    }

    #[test]
    fn gutter_is_on_the_logical_end_and_counts_bottom_padding_in_the_fit() {
        let mut rt = Runtime::new(
            Fixture { short: true, rtl: true, dark: false },
            view,
            (1440.0, 280.5),
            Env::new(false, 1.0),
        );
        rt.frame(0.0);
        assert_eq!(rect(&rt, "fixture-body"), [294.0, 64.0, 672.0, 192.5]);

        // The same body now exceeds the viewport by one point of bottom padding.
        rt.set_viewport((1440.0, 279.5), 1.0);
        rt.frame(1.0 / 60.0);
        assert_eq!(rect(&rt, "fixture-body"), [298.5, 64.0, 672.0, 192.5]);
    }
}
