//! Independent audit for T021 (the BrandSidebar Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/onboarding/BrandSidebar.tsx`: the 352px
//! fixed-width column, the brand row (database glyph + DataZen), the
//! heading/description texts from the zh-CN pack, the three feature rows in
//! React's order (import / AI / dashboard) with the registry icons, the
//! mt-auto version footer sharing the wizard footer's 56px height, and the
//! `v{version}` label with the em-dash placeholder when the host never
//! delivers. Element evidence comes from `Runtime::elements()` (runtime
//! layout), never from a value the component reports about itself.
//!
//! This port's only logic is [`version_label`]; the rest is paint, so the
//! mutation proof lives at the runtime level (typechecked here, executed on
//! the coordinator's reference machine) plus the label helper below.

use rune::{Cx, El, Env, Runtime};

use crate::views::onboarding::brand_sidebar::{ASIDE_WIDTH, FOOTER_HEIGHT, BrandSidebar, version_label};

#[derive(Clone)]
struct Scene {
    version: Option<&'static str>,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut sidebar = BrandSidebar::new();
    if let Some(version) = s.version {
        sidebar = sidebar.version(version);
    }
    // The wizard root is a full-height row; the aside stretches inside it.
    rune::row().key("probe").h_full().child(sidebar.build(s.dark))
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 720.0), // the onboarding window's default size
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<Scene>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

// ---- the one piece of logic -------------------------------------------------

#[test]
fn version_label_prefixes_the_v() {
    // React renders `v{version}` with the useState('…') default.
    assert_eq!(version_label("…"), "v…");
    assert_eq!(version_label("1.2.3"), "v1.2.3");
    assert_eq!(version_label(""), "v");
}

// ---- runtime evidence -------------------------------------------------------

#[test]
fn the_column_has_the_brands_fixed_width_and_fills_the_height() {
    for dark in [true, false] {
        let rt = rt(Scene { version: None, dark });
        let aside = maybe(&rt, "onboarding-sidebar").expect("the aside is keyed");
        let [x, y, w, h] = aside.rect;
        assert_eq!((x, y), (0.0, 0.0), "flush top-left (dark={dark})");
        assert_eq!(w, ASIDE_WIDTH, "React w-[352px] = 264pt (dark={dark})");
        assert_eq!(h, 720.0, "stretched by the wizard row (dark={dark})");
    }
}

#[test]
fn the_brand_row_wordmark_and_texts_come_from_the_zh_pack() {
    let rt = rt(Scene { version: None, dark: true });
    assert!(has(&rt, "onboarding-sidebar-brand"));
    assert_eq!(text_of(&rt, "onboarding-sidebar-brand-name"), "DataZen");
    assert_eq!(text_of(&rt, "onboarding-sidebar-heading"), "欢迎使用 DataZen");
    assert_eq!(
        text_of(&rt, "onboarding-sidebar-desc"),
        "已经有数据库客户端？几秒内导入连接——或从零开始。所有数据仅存储在本机。"
    );
}

#[test]
fn the_three_feature_rows_render_in_react_order() {
    let rt = rt(Scene { version: None, dark: true });
    // React's order: import (download), AI (sparkles), dashboard (gauge).
    let expected = [
        ("0", "一键导入", "DBeaver、DataGrip、Navicat、TablePlus。"),
        ("1", "AI 原生", "感知 Schema 的 SQL 辅助。"),
        ("2", "可视化仪表盘", "查询结果秒变图表，报告无忧。"),
    ];
    for (index, title, desc) in expected {
        let row = maybe(&rt, &format!("onboarding-sidebar-feature-{index}"))
            .unwrap_or_else(|| panic!("feature row {index} is keyed"));
        assert_eq!(row.role, "row", "a feature row");
        assert_eq!(
            text_of(&rt, &format!("onboarding-sidebar-feature-{index}-title")),
            title
        );
        assert_eq!(
            text_of(&rt, &format!("onboarding-sidebar-feature-{index}-desc")),
            desc
        );
        let icon = maybe(&rt, &format!("onboarding-sidebar-feature-{index}-icon"))
            .unwrap_or_else(|| panic!("feature {index}'s icon well is keyed"));
        let [ix, iy, iw, ih] = icon.rect;
        assert_eq!((iw, ih), (22.5, 22.5), "React h-[30px] w-[30px] = 22.5pt");
        assert!(ix >= 0.0 && iy >= 0.0);
    }
    assert!(
        !has(&rt, "onboarding-sidebar-feature-3"),
        "exactly three feature rows"
    );
}

#[test]
fn the_version_footer_sits_on_the_bottom_line_with_its_height() {
    // React: mt-auto + WIZARD_FOOTER_HEIGHT_CLASS — the footer ends at the
    // sidebar's bottom edge, 42pt tall.
    let rt = rt(Scene { version: None, dark: false });
    let aside = maybe(&rt, "onboarding-sidebar").expect("aside");
    let foot = maybe(&rt, "onboarding-sidebar-foot").expect("footer");
    let [_, _, _, ah] = aside.rect;
    let [fx, fy, fw, fh] = foot.rect;
    assert_eq!(fh, FOOTER_HEIGHT, "React h-[56px] = 42pt");
    assert!(
        (fy + fh - ah).abs() < 0.5,
        "the footer ends on the sidebar's bottom line ({fy}+{fh} vs {ah})"
    );
    assert_eq!(fx, 0.0, "still inside the padded column flow");
    let _ = fw;
    // The em-dash placeholder while the host never delivers.
    assert_eq!(text_of(&rt, "onboarding-version-label"), "v…");
}

#[test]
fn a_delivered_version_reaches_the_label() {
    let rt = rt(Scene { version: Some("0.9.7"), dark: true });
    assert_eq!(text_of(&rt, "onboarding-version-label"), "v0.9.7");
}

#[test]
fn every_part_stays_inside_the_column() {
    for dark in [true, false] {
        let rt = rt(Scene { version: None, dark });
        let mut outside = Vec::new();
        for element in rt.elements() {
            let Some(key) = &element.key else { continue };
            if !key.starts_with("onboarding-sidebar") && key != "onboarding-version-label" {
                continue;
            }
            let [x, y, w, h] = element.rect;
            if x < -0.5 || y < -0.5 || x + w > ASIDE_WIDTH + 0.5 || y + h > 720.5 {
                outside.push(format!("{key} [{x}, {y}, {w}, {h}]"));
            }
        }
        assert!(
            outside.is_empty(),
            "dark={dark}: parts leave the column: {outside:?}"
        );
    }
}
