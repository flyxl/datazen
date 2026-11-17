//! T0-12: base geometry regression.
//!
//! Every Phase-0 base is rendered inside one padded page frame at the minimum
//! window (960×640) and at the design size (1440×816), in dark and light. Two
//! invariants are checked on the runtime's own element list:
//!
//! 1. every element's rect stays inside the window (with a 0.5pt tolerance),
//!    except the virtual scroll extents listed in [`EXCLUDED_EXTENTS`];
//! 2. siblings under the same parent path never overlap, except when one rect
//!    contains the other (a child inside its parent's box is layering, not a
//!    collision) or when either has zero area.
//!
//! A failure prints the element key and rect, the window and the theme, so the
//! report can point at the widget without re-running the scene.

use rune::{col, row, text, Cx, El, Env, Runtime};
use rune_kit::ControlSize;

use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::menu::{Item, Menu};
use crate::ui::path_input::PathInput;
use crate::ui::select::Select;
use crate::ui::slider::Slider;
use crate::ui::table::{CellValue, Column, DataTable};
use crate::ui::toast::{self, Toast};
use crate::ui::toggle::{Checkbox, RadioGroup, RadioItem, Switch};
use crate::ui::tooltip::{HoverCard, Tooltip};
use rune_kit::{Button, ButtonVariant, Placement};

/// Virtual scroll extents: content that is intentionally taller than its
/// viewport. The viewport itself (the dialog body) is still bounds-checked, and
/// the dialog scene separately asserts that the body keeps its cap and the
/// footer stays pinned; excluding the content here is not a blanket filter.
const EXCLUDED_EXTENTS: &[&str] = &["geo-dialog-content"];

/// Intentional layering inside one base, keyed by (prefix, prefix). The table
/// header paints a 1pt rule on the bottom edge of its column boxes; that 1pt
/// intersection is the divider, not two controls on top of each other.
const ALLOWED_OVERLAPS: &[(&str, &str)] = &[("geo-table-head-rule", "geo-table-col-")];

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const PAD: f32 = 24.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scene {
    #[default]
    TextFields,
    Buttons,
    Dialog,
    SelectClosed,
    SelectOpen,
    Choices,
    MenuEdge,
    TableRows,
    TableEmpty,
    Toast,
    Tooltip,
    HoverCard,
    Banners,
    Copyable,
    Sliders,
    PathInputs,
}

impl Scene {
    fn label(self) -> &'static str {
        match self {
            Scene::TextFields => "text-fields",
            Scene::Buttons => "buttons",
            Scene::Dialog => "dialog",
            Scene::SelectClosed => "select-closed",
            Scene::SelectOpen => "select-open",
            Scene::Choices => "checkbox-switch-radio",
            Scene::MenuEdge => "menu-edge",
            Scene::TableRows => "table-rows",
            Scene::TableEmpty => "table-empty",
            Scene::Toast => "toast",
            Scene::Tooltip => "tooltip",
            Scene::HoverCard => "hover-card",
            Scene::Banners => "banners",
            Scene::Copyable => "copyable",
            Scene::Sliders => "sliders",
            Scene::PathInputs => "path-inputs",
        }
    }

    /// The widget root whose subtree gets the sibling-overlap check. Overlay
    /// surfaces are checked instead of the page they cover.
    fn sibling_root(self) -> &'static str {
        match self {
            Scene::TextFields => "geo-fields",
            Scene::Buttons => "geo-buttons",
            Scene::Dialog => "geo-dialog",
            Scene::SelectClosed => "geo-select-page",
            Scene::SelectOpen => "geo-select-content",
            Scene::Choices => "geo-choices",
            Scene::MenuEdge => "geo-menu-menu",
            Scene::TableRows | Scene::TableEmpty => "geo-table",
            Scene::Toast => "toast-region",
            Scene::Tooltip => "geo-tip-tooltip",
            Scene::HoverCard => "geo-card-card",
            Scene::Banners => "geo-banners",
            Scene::Copyable => "geo-copyables",
            Scene::Sliders => "geo-sliders",
            Scene::PathInputs => "geo-paths",
        }
    }
}

#[derive(Default)]
struct Geo {
    scene: Scene,
    width: f32,
    height: f32,
    dark: bool,
    field: rune::TextEdit,
    path: rune::TextEdit,
    open: bool,
}

fn page(content: El) -> El {
    col()
        .key("geo-frame")
        .w_full()
        .h_full()
        .padding(PAD)
        .child(content)
}

fn pin_bottom_right(el: El) -> El {
    col()
        .key("geo-anchor")
        .pin(None, None, Some(PAD), Some(PAD))
        .child(el)
}

fn view(state: &Geo, _cx: &mut Cx<Geo>) -> El {
    let k = kit(state.dark);
    let mut layer: Vec<El> = Vec::new();
    let content: El = match state.scene {
        Scene::TextFields => col()
            .key("geo-fields")
            .gap(8.0)
            .child(
                TextField::new("geo-field-24", &state.field)
                    .height(24.0)
                    .width(260.0)
                    .placeholder("Name")
                    .on_edit(|s: &mut Geo| &mut s.field)
                    .build(state.dark),
            )
            .child(
                TextField::new("geo-field-28", &state.field)
                    .height(28.0)
                    .width(260.0)
                    .placeholder("Name")
                    .on_edit(|s: &mut Geo| &mut s.field)
                    .build(state.dark),
            )
            .child(
                TextField::new("geo-field-36", &state.field)
                    .height(36.0)
                    .width(260.0)
                    .placeholder("Name")
                    .on_edit(|s: &mut Geo| &mut s.field)
                    .build(state.dark),
            ),
        Scene::Buttons => row().key("geo-buttons").gap(8.0).children([
            Button::new("Mini")
                .control_size(ControlSize::Mini)
                .build(k)
                .key("geo-btn-mini"),
            Button::new("Small")
                .control_size(ControlSize::Small)
                .build(k)
                .key("geo-btn-small"),
            Button::new("Regular")
                .control_size(ControlSize::Regular)
                .build(k)
                .key("geo-btn-regular"),
            Button::new("Large")
                .control_size(ControlSize::Large)
                .build(k)
                .key("geo-btn-large"),
            Button::new("ExtraLarge")
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .build(k)
                .key("geo-btn-xl"),
        ]),
        Scene::Dialog => {
            let mut dialog = Dialog::new("geo-dialog", "Import connections")
                .open(true)
                .width(720.0)
                .fit()
                .body_max_height(160.0)
                .child(
                    col()
                        .key("geo-dialog-content")
                        .w_full()
                        .h(900.0)
                        .gap(8.0)
                        .children((0..8).map(|i| text(format!("row {i}")))),
                )
                .footer(row().gap(8.0).children((0..5).map(|i| {
                    Button::new(format!("Action {i}"))
                        .control_size(ControlSize::ExtraLarge)
                        .build(k)
                        .key(format!("geo-dialog-action-{i}"))
                })))
                .on_close(|s: &mut Geo| s.open = false);
            dialog = dialog.footer_height(64.0);
            dialog.build(k, &mut layer);
            col()
                .key("geo-dialog-page")
                .child(text("page behind the dialog"))
        }
        Scene::SelectClosed | Scene::SelectOpen => {
            let select = Select::new("geo-select", &["Alpha", "Bravo", "Charlie", "Delta"])
                .current(Some("Bravo"))
                .placeholder("Pick one")
                .label("Target")
                .width(260.0)
                .open(state.scene == Scene::SelectOpen)
                .on_toggle(|s: &mut Geo| s.open = !s.open)
                .on_close(|s: &mut Geo| s.open = false)
                .on_choose(|s: &mut Geo, _v, _tx| s.open = false);
            let trigger = select.build(k, &mut layer);
            col()
                .key("geo-select-page")
                .w_full()
                .h_full()
                .child(pin_bottom_right(trigger))
        }
        Scene::Choices => col()
            .key("geo-choices")
            .gap(12.0)
            .child(
                Checkbox::new("geo-check", true)
                    .label("Enable checks")
                    .description("Adds a check step")
                    .build(k),
            )
            .child(
                Switch::new("geo-switch", true)
                    .label("Enable switch")
                    .description("Turns the feature on")
                    .build(k),
            )
            .child(
                RadioGroup::new("geo-radio")
                    .label("Mode")
                    .item(RadioItem::new("First", true))
                    .item(RadioItem::new("Second", false))
                    .item(RadioItem::new("Third", false))
                    .build(k),
            ),
        Scene::MenuEdge => {
            let menu = Menu::new("geo-menu")
                .trigger(
                    row()
                        .key("geo-menu-trigger")
                        .size(120.0, 28.0)
                        .center()
                        .child(text("Open")),
                )
                .open(true)
                .at(state.width - 8.0, state.height - 8.0)
                .placement(Placement::BottomStart)
                .min_width(180.0)
                .group_label("Group")
                .item(Item::new("Alpha").on_select(|s: &mut Geo| s.open = false))
                .item(Item::new("Bravo").on_select(|s: &mut Geo| s.open = false))
                .separator()
                .item(Item::new("Charlie").disabled(true))
                .on_close(|s: &mut Geo| s.open = false);
            menu.build(k, &mut layer);
            col().key("geo-menu-page").child(text("page"))
        }
        Scene::TableRows | Scene::TableEmpty => {
            let columns = [
                Column::new("Name"),
                Column::new("Size").width(90.0).end(),
                Column::new("Rows").width(80.0).end(),
            ];
            let rows: Vec<Vec<CellValue>> = if state.scene == Scene::TableRows {
                (0..8)
                    .map(|i| {
                        vec![
                            CellValue::from(format!("table_{i}")),
                            CellValue::from(format!("{}", 1024 * (i + 1))),
                            CellValue::from(format!("{i}")),
                        ]
                    })
                    .collect()
            } else {
                Vec::new()
            };
            DataTable::new("geo-table", &columns)
                .rows(rows)
                .height(320.0)
                .build(state.dark)
        }
        Scene::Toast => {
            let region = toast::toast_region(vec![
                Toast::new("geo-toast", "Saved")
                    .description("The connection was saved")
                    .closable()
                    .build(k)
                    .key("geo-toast-a"),
                Toast::new("geo-toast-2", "Warning")
                    .description("A longer description that wraps onto a second line")
                    .build(k)
                    .key("geo-toast-b"),
                Toast::new("geo-toast-3", "Failed")
                    .description("err")
                    .build(k)
                    .key("geo-toast-c"),
            ])
            .build(k);
            region
        }
        Scene::Tooltip => {
            let tip = Tooltip::new("geo-tip")
                .trigger(
                    row()
                        .key("geo-tip-trigger")
                        .size(120.0, 28.0)
                        .center()
                        .child(text("Hover")),
                )
                .child(
                    col()
                        .key("geo-tip-body")
                        .gap(2.0)
                        .children((0..5).map(|line| text(format!("tooltip line {line}")))),
                )
                .label("More info")
                .open(true)
                .placement(Placement::BottomStart)
                .build(k, &mut layer);
            col()
                .key("geo-tip-page")
                .w_full()
                .h_full()
                .child(pin_bottom_right(tip))
        }
        Scene::HoverCard => {
            let card = HoverCard::new("geo-card")
                .trigger(
                    row()
                        .key("geo-card-trigger")
                        .size(120.0, 28.0)
                        .center()
                        .child(text("Hover")),
                )
                .child(
                    col()
                        .key("geo-card-body")
                        .gap(6.0)
                        .child(text("Hover card title"))
                        .child(text(
                            "A body that is wide enough to need flipping near the edge.",
                        ))
                        .child(Button::new("Open").build(k)),
                )
                .open(true)
                .placement(Placement::BottomStart)
                .build(k, &mut layer);
            col()
                .key("geo-card-page")
                .w_full()
                .h_full()
                .child(pin_bottom_right(card))
        }
        Scene::Banners => col().key("geo-banners").gap(12.0).children([
            Banner::new("geo-banner-plain", "Plain banner")
                .variant(BannerVariant::Plain)
                .build(k)
                .unwrap_or_else(|| panic!("plain banner builds")),
            Banner::new("geo-banner-boxed", "Boxed banner")
                .variant(BannerVariant::Boxed)
                .build(k)
                .unwrap_or_else(|| panic!("boxed banner builds")),
            Banner::new("geo-banner-strip", "Strip banner")
                .variant(BannerVariant::Strip)
                .build(k)
                .unwrap_or_else(|| panic!("strip banner builds")),
        ]),
        Scene::Copyable => col()
            .key("geo-copyables")
            .gap(12.0)
            .w(560.0)
            .child(
                CopyableError::new("geo-copy-plain", "plain message without monospace")
                    .copy_button(true)
                    .build::<Geo>(k),
            )
            .child(
                CopyableError::new(
                    "geo-copy-mono",
                    "mono message with a long tail aaaaaaaaaaaaaaaa",
                )
                .copy_button(true)
                .mono(true)
                .build::<Geo>(k),
            ),
        Scene::Sliders => col()
            .key("geo-sliders")
            .gap(16.0)
            .child(
                Slider::new("geo-slider-0", 0.0)
                    .min(0.0)
                    .max(1.0)
                    .label("Ratio 0")
                    .width(320.0)
                    .on_change(|_s: &mut Geo, _v: f64| {})
                    .build(state.dark),
            )
            .child(
                Slider::new("geo-slider-50", 0.5)
                    .min(0.0)
                    .max(1.0)
                    .label("Ratio 0.5")
                    .width(320.0)
                    .on_change(|_s: &mut Geo, _v: f64| {})
                    .build(state.dark),
            )
            .child(
                Slider::new("geo-slider-100", 1.0)
                    .min(0.0)
                    .max(1.0)
                    .label("Ratio 1")
                    .width(320.0)
                    .on_change(|_s: &mut Geo, _v: f64| {})
                    .build(state.dark),
            ),
        Scene::PathInputs => col()
            .key("geo-paths")
            .gap(12.0)
            .child(
                PathInput::new("geo-path-plain", &state.path)
                    .placeholder("Select a path")
                    .label("Plain path")
                    .on_browse(|_s: &mut Geo| {})
                    .on_edit(|s: &mut Geo| &mut s.path)
                    .build(state.dark),
            )
            .child(
                PathInput::new("geo-path-browse", &state.path)
                    .placeholder("Select a path")
                    .label("Path with browse")
                    .on_browse(|_s: &mut Geo| {})
                    .on_edit(|s: &mut Geo| &mut s.path)
                    .build(state.dark),
            )
            .child(
                PathInput::new("geo-path-disabled", &state.path)
                    .placeholder("Select a path")
                    .label("Disabled path")
                    .disabled(true)
                    .on_browse(|_s: &mut Geo| {})
                    .on_edit(|s: &mut Geo| &mut s.path)
                    .build(state.dark),
            ),
    };

    let mut root = col().key("geo-root").w_full().h_full().child(page(content));
    for el in layer {
        root = root.child(el);
    }
    root
}

fn scan(scene: Scene) {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let state = Geo {
                scene,
                width,
                height,
                dark,
                ..Geo::default()
            };
            let mut rt = Runtime::new(
                state,
                view,
                (width, height),
                Env {
                    system_dark: dark,
                    scale: 1.0,
                },
            );
            rt.frame(0.0);
            let theme = if dark { "dark" } else { "light" };
            let context = format!("{} {width}x{height} {theme}", scene.label());
            assert_inside(&rt, &context, width, height);
            assert_siblings(&rt, scene.sibling_root(), &context, width, height);
            match scene {
                Scene::Dialog => assert_dialog_invariants(&rt, &context, width, height),
                Scene::MenuEdge => assert_menu_flipped(&rt, &context, width, height),
                Scene::Tooltip => assert_tooltip_flipped(&rt, &context),
                _ => {}
            }
        }
    }
}

fn rect_for(rt: &Runtime<Geo>, key: &str) -> Option<[f32; 4]> {
    rt.elements()
        .iter()
        .find(|element| element.key.as_deref() == Some(key))
        .map(|element| element.rect)
}

fn assert_dialog_invariants(rt: &Runtime<Geo>, context: &str, width: f32, height: f32) {
    let panel =
        rect_for(rt, "geo-dialog").unwrap_or_else(|| panic!("{context}: geo-dialog missing"));
    let body = rect_for(rt, "geo-dialog-body")
        .unwrap_or_else(|| panic!("{context}: geo-dialog-body missing"));
    let footer = rect_for(rt, "geo-dialog-footer")
        .unwrap_or_else(|| panic!("{context}: geo-dialog-footer missing"));
    let content = rect_for(rt, "geo-dialog-content").unwrap_or_else(|| {
        panic!("{context}: geo-dialog-content missing (the excluded scroll extent)")
    });
    assert!(
        body[3] <= 160.5,
        "{context}: dialog body is {:?} tall, cap is 160 (window {width}x{height})",
        body[3]
    );
    assert!(
        content[3] > body[3] + 0.5,
        "{context}: the content is not a scroll extent; body {:?} content {:?}",
        body,
        content
    );
    assert!(
        (footer[1] + footer[3] - (panel[1] + panel[3])).abs() <= 0.5,
        "{context}: footer {:?} is not pinned to panel bottom {:?}",
        footer,
        panel
    );
    for (name, rect) in [("body", body), ("footer", footer)] {
        assert!(
            contains(panel, rect),
            "{context}: dialog {name} {rect:?} escapes panel {panel:?} (window {width}x{height})"
        );
    }
    for i in 0..5 {
        let key = format!("geo-dialog-action-{i}");
        let action = rect_for(rt, &key).unwrap_or_else(|| panic!("{context}: {key} missing"));
        assert!(
            contains(footer, action),
            "{context}: {key} {action:?} escapes footer {footer:?} (window {width}x{height})"
        );
    }
    assert_eq!(
        rt.elements()
            .iter()
            .filter(|element| element.key.as_deref() == Some("geo-dialog-body"))
            .count(),
        1,
        "{context}: expected exactly one dialog body"
    );
}

fn assert_menu_flipped(rt: &Runtime<Geo>, context: &str, width: f32, height: f32) {
    let panel =
        rect_for(rt, "geo-menu-menu").unwrap_or_else(|| panic!("{context}: geo-menu-menu missing"));
    let right = panel[0] + panel[2];
    let bottom = panel[1] + panel[3];
    assert!(
        right <= width - 8.0 + 0.5 && bottom <= height - 8.0 + 0.5,
        "{context}: edge menu {panel:?} (right {right}, bottom {bottom}) is not flipped back inside \
         the 8pt margin of {width}x{height}"
    );
}

fn assert_tooltip_flipped(rt: &Runtime<Geo>, context: &str) {
    let tip = rect_for(rt, "geo-tip-tooltip")
        .unwrap_or_else(|| panic!("{context}: geo-tip-tooltip missing"));
    let trigger = rect_for(rt, "geo-tip-trigger")
        .unwrap_or_else(|| panic!("{context}: geo-tip-trigger missing"));
    assert!(
        tip[1] + tip[3] <= trigger[1] + 0.5,
        "{context}: tooltip {tip:?} was not flipped above trigger {trigger:?}"
    );
}

fn assert_inside(rt: &Runtime<Geo>, context: &str, width: f32, height: f32) {
    let mut outside = Vec::new();
    for element in rt.elements() {
        let key = element.key.as_deref().unwrap_or(element.path.as_str());
        if EXCLUDED_EXTENTS.contains(&key) {
            continue;
        }
        let [x, y, w, h] = element.rect;
        if x < -0.5 || y < -0.5 || x + w > width + 0.5 || y + h > height + 0.5 {
            outside.push(format!(
                "key={key} rect={:?} role={}",
                element.rect, element.role
            ));
        }
    }
    assert!(
        outside.is_empty(),
        "{context}: {} node(s) cross the {width}x{height} window: {outside:#?} (excluded extents: \
         {EXCLUDED_EXTENTS:?})",
        outside.len()
    );
}

fn contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    let (ox, oy, ow, oh) = (outer[0], outer[1], outer[2], outer[3]);
    let (ix, iy, iw, ih) = (inner[0], inner[1], inner[2], inner[3]);
    ix >= ox - 0.5 && iy >= oy - 0.5 && ix + iw <= ox + ow + 0.5 && iy + ih <= oy + oh + 0.5
}

fn assert_siblings(rt: &Runtime<Geo>, root_key: &str, context: &str, width: f32, height: f32) {
    let elements = rt.elements();
    let root = elements
        .iter()
        .find(|element| element.key.as_deref() == Some(root_key))
        .unwrap_or_else(|| {
            let keys: Vec<String> = elements.iter().filter_map(|e| e.key.clone()).collect();
            panic!("{context}: root key {root_key} is not rendered; keys: {keys:?}")
        });
    let prefix = format!("{}/", root.path);
    let nodes: Vec<&rune::ElementInfo> = elements
        .iter()
        .filter(|element| element.path == root.path || element.path.starts_with(&prefix))
        .collect();

    let mut collisions = Vec::new();
    for (index, first) in nodes.iter().enumerate() {
        for second in &nodes[index + 1..] {
            if parent_path(&first.path) != parent_path(&second.path) {
                continue;
            }
            if zero_area(first.rect) || zero_area(second.rect) {
                continue;
            }
            if contains(first.rect, second.rect) || contains(second.rect, first.rect) {
                continue;
            }
            if allowed_overlap(first, second) {
                continue;
            }
            let overlap_w = (first.rect[0] + first.rect[2]).min(second.rect[0] + second.rect[2])
                - first.rect[0].max(second.rect[0]);
            let overlap_h = (first.rect[1] + first.rect[3]).min(second.rect[1] + second.rect[3])
                - first.rect[1].max(second.rect[1]);
            if overlap_w > 0.5 && overlap_h > 0.5 {
                collisions.push(format!(
                    "{} {:?} x {} {:?}",
                    key_of(first),
                    first.rect,
                    key_of(second),
                    second.rect
                ));
            }
        }
    }
    assert!(
        collisions.is_empty(),
        "{context}: {} sibling collision(s) under {root_key} in {width}x{height}: {collisions:#?}",
        collisions.len()
    );
}

fn allowed_overlap(first: &rune::ElementInfo, second: &rune::ElementInfo) -> bool {
    let key = |element: &rune::ElementInfo| element.key.clone().unwrap_or_default();
    let (a, b) = (key(first), key(second));
    ALLOWED_OVERLAPS.iter().any(|(left, right)| {
        (a.starts_with(left) && b.starts_with(right))
            || (b.starts_with(left) && a.starts_with(right))
    })
}

fn key_of(element: &rune::ElementInfo) -> String {
    element.key.clone().unwrap_or_else(|| element.path.clone())
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn zero_area(rect: [f32; 4]) -> bool {
    rect[2] <= 0.5 || rect[3] <= 0.5
}

#[test]
fn text_fields_stay_inside_the_window() {
    scan(Scene::TextFields);
}

#[test]
fn kit_buttons_stay_inside_the_window() {
    scan(Scene::Buttons);
}

#[test]
fn dialog_stays_inside_the_window() {
    scan(Scene::Dialog);
}

#[test]
fn select_closed_stays_inside_the_window() {
    scan(Scene::SelectClosed);
}

#[test]
fn select_open_stays_inside_the_window() {
    scan(Scene::SelectOpen);
}

#[test]
fn checkbox_switch_and_radio_stay_inside_the_window() {
    scan(Scene::Choices);
}

#[test]
fn edge_menu_stays_inside_the_window() {
    scan(Scene::MenuEdge);
}

#[test]
fn table_rows_stay_inside_the_window() {
    scan(Scene::TableRows);
}

#[test]
fn empty_table_stays_inside_the_window() {
    scan(Scene::TableEmpty);
}

#[test]
fn toast_stack_stays_inside_the_window() {
    scan(Scene::Toast);
}

#[test]
fn tooltip_stays_inside_the_window() {
    scan(Scene::Tooltip);
}

#[test]
fn hover_card_stays_inside_the_window() {
    scan(Scene::HoverCard);
}

#[test]
fn banners_stay_inside_the_window() {
    scan(Scene::Banners);
}

#[test]
fn copyable_error_stays_inside_the_window() {
    scan(Scene::Copyable);
}

#[test]
fn sliders_stay_inside_the_window() {
    scan(Scene::Sliders);
}

#[test]
fn path_inputs_stay_inside_the_window() {
    scan(Scene::PathInputs);
}
