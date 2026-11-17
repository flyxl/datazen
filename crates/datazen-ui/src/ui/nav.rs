//! Navigator primitives: toolbar, search field, and tree row building blocks.

use crate::state_def::AppState;
use rune::tok::{self, type_};
use rune::{col, row, text, Color, El, Icon, TextEdit};

use super::field::TextField;
use super::theme::{nav_palette, NAV_CHEVRON, NAV_ICON, NAV_INDENT, NAV_RADIUS, NAV_ROW_H, NAV_SEARCH_H};

fn base_icon_button(key: &str, icon: &'static Icon, label: &str, theme: tok::Theme) -> El {
    row()
        .key(key.to_owned())
        .w(24.0)
        .h(24.0)
        .center()
        .radius(6.0)
        .hover(theme.hover)
        .label(label.to_owned())
        .child(rune::icon(icon, 14.0).color(theme.text_secondary))
}

pub(crate) fn icon_button_action(
    key: &str,
    icon: &'static Icon,
    label: &str,
    theme: tok::Theme,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    base_icon_button(key, icon, label, theme).on_press(on_press)
}

pub(crate) fn search_field(
    key: &str,
    edit: &TextEdit,
    placeholder: &str,
    theme: tok::Theme,
    on_edit: fn(&mut AppState) -> &mut TextEdit,
) -> El {
    let palette = nav_palette(theme, false);
    // Keep the legacy `navigator-filter-input` key on the editable input for
    // existing tests and scripted interactions.
    let input_key = if key == "navigator-filter" {
        "navigator-filter-input".to_owned()
    } else {
        format!("{key}-input")
    };
    let input = TextField::new(input_key, edit)
        .height(NAV_SEARCH_H)
        .grow()
        .radius(NAV_RADIUS)
        .fill(Color::TRANSPARENT)
        .ink(theme.text)
        .placeholder(placeholder.to_owned(), palette.text_faint)
        .on_edit(on_edit)
        .build();
    row()
        .key(key.to_owned())
        .w_full()
        .h(NAV_SEARCH_H)
        .px(8.0)
        .gap(6.0)
        .center_y()
        .radius(NAV_RADIUS)
        .fill(theme.control_off)
        .child(rune::icon(&crate::icons::ICON_SEARCH, 14.0).color(theme.text_secondary))
        .child(input)
}

pub(crate) fn nav_section_header(
    key: &str,
    label: &str,
    count: usize,
    expanded: bool,
    theme: tok::Theme,
) -> El {
    let chevron = if expanded {
        &crate::icons::ICON_CHEVRON_DOWN
    } else {
        &crate::icons::ICON_CHEVRON_RIGHT
    };
    let folder = if expanded {
        &crate::icons::ICON_FOLDER_OPEN
    } else {
        &crate::icons::ICON_FOLDER
    };
    // React navigator section/group headers use an amber folder icon.
    let folder_color = super::palette::folder_icon(false);
    row()
        .key(key.to_owned())
        .w_full()
        .h(24.0)
        .px(6.0)
        .gap(6.0)
        .center_y()
        .radius(NAV_RADIUS)
        .hover(theme.hover)
        .child(rune::icon(chevron, NAV_CHEVRON).color(theme.text_secondary))
        .child(rune::icon(folder, NAV_ICON).color(folder_color))
        .child(
            text(label.to_uppercase())
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        )
        .child(
            text(format!("({count})"))
                .font(type_::LABEL)
                .color(theme.text_secondary.alpha(0.8)),
        )
}

pub(crate) fn nav_empty_row(key: &str, label: &str, depth: u32, theme: tok::Theme) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .h(24.0)
        .pl(8.0 + depth as f32 * NAV_INDENT)
        .pr(8.0)
        .center_y()
        .child(text(label.to_owned()).font(type_::LABEL).color(theme.text_secondary))
}

pub(crate) fn nav_connection_row(
    key: &str,
    name: &str,
    engine: &str,
    depth: u32,
    selected: bool,
    theme: tok::Theme,
    dark: bool,
) -> El {
    let palette = nav_palette(theme, dark);
    let fill = if selected { palette.selected } else { Color::TRANSPARENT };
    let mut r = row()
        .key(key.to_owned())
        .w_full()
        .h(NAV_ROW_H)
        .pl(8.0 + depth as f32 * NAV_INDENT)
        .pr(8.0)
        .gap(8.0)
        .center_y()
        .radius(NAV_RADIUS)
        .fill(fill)
        .label(format!("{name} · {engine}"));
    if !selected {
        r = r.hover(theme.hover);
    }
    r.child(rune::icon(&crate::icons::ICON_CONNECTIONS, NAV_ICON).color(if selected {
        palette.accent
    } else {
        palette.icon_muted
    }))
    .child(
        col()
            .grow(1.0)
            .min_w(0.0)
            .gap(0.0)
            .child(
                text(name.to_owned())
                    .font(type_::LABEL)
                    .color(palette.text)
                    .truncate(),
            )
            .child(
                text(engine.to_owned())
                    .font(type_::LABEL)
                    .color(palette.text_faint)
                    .truncate(),
            ),
    )
}

pub(crate) fn nav_branch_row(
    key: &str,
    label: &str,
    secondary: Option<&str>,
    depth: u32,
    expanded: bool,
    icon: &'static Icon,
    theme: tok::Theme,
) -> El {
    let chevron = if expanded {
        &crate::icons::ICON_CHEVRON_DOWN
    } else {
        &crate::icons::ICON_CHEVRON_RIGHT
    };
    let row_el = row()
        .key(key.to_owned())
        .w_full()
        .h(NAV_ROW_H)
        .pl(8.0 + depth as f32 * NAV_INDENT)
        .pr(8.0)
        .gap(6.0)
        .center_y()
        .radius(NAV_RADIUS)
        .hover(theme.hover)
        .child(rune::icon(chevron, NAV_CHEVRON).color(theme.text_secondary))
        .child(rune::icon(icon, NAV_ICON).color(theme.text_secondary));
    let mut label_col = col().grow(1.0).min_w(0.0).gap(0.0).child(
        text(label.to_owned())
            .font(type_::LABEL)
            .color(theme.text)
            .truncate(),
    );
    if let Some(sec) = secondary {
        label_col = label_col.child(
            text(sec.to_owned())
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .truncate(),
        );
    }
    row_el.child(label_col)
}

pub(crate) fn nav_leaf_row(
    key: &str,
    label: &str,
    depth: u32,
    icon: &'static Icon,
    theme: tok::Theme,
) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .h(NAV_ROW_H)
        .pl(8.0 + depth as f32 * NAV_INDENT + NAV_CHEVRON + 6.0)
        .pr(8.0)
        .gap(8.0)
        .center_y()
        .radius(NAV_RADIUS)
        .hover(theme.hover)
        .child(rune::icon(icon, NAV_ICON).color(theme.text_secondary))
        .child(
            text(label.to_owned())
                .font(type_::LABEL)
                .color(theme.text)
                .truncate(),
        )
}

pub(crate) fn nav_toolbar_button(
    key: &str,
    icon: &'static Icon,
    label: &str,
    theme: tok::Theme,
    on_press: impl Fn(&mut AppState) + 'static,
) -> El {
    icon_button_action(key, icon, label, theme, on_press)
}

// (intentionally empty: helpers are composed directly by views)