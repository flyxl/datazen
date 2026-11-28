//! The Editor Settings Add dialog/action backed by Native's name/SQL snippet store.
//! The React Description, import/export, and rename controls are not projected
//! into this store: Native has no place to save their data.
use rune::{col, icon, row, text, text_input, El, TextStyle};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::ICON_PLUS;
use crate::state_def::AppState;
use crate::ui::{dialog::Dialog, field::TextField, kit::kit};

const CAPTION: TextStyle = TextStyle::new(12.0, 400.0).lh(16.0);

pub(super) fn section_header(k: &Kit) -> El {
    // The original card follows the remaining Editor preferences. Do not
    // paint that card above its proper position or reserve space for controls
    // Native does not yet mount. Keep the live Add action beside the heading.
    let add = t("query.snippets.add");
    col()
        .key("settings-snippets")
        .w_full()
        .gap(6.0)
        .child(
            row().w_full().gap(8.0).center_y().justify_between().flex_wrap()
                .child(
                    text(t("query.snippets"))
                        .key("settings-snippets-heading")
                        .font(rune::tok::type_::TITLE)
                        .color(k.c.text_base),
                )
                .child(
                    button(k, "snippet-add", add, true)
                        .child(icon(&ICON_PLUS, 14.0).color(k.c.submit_fg))
                        .child(text(add).font(CAPTION).color(k.c.submit_fg))
                        .on_press(|state: &mut AppState| state.open_native_snippet_add()),
                ),
        )
}

pub(in crate::views) fn mount(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    let draft = &state.native_snippet_add;
    if !state.on_editor_settings() || !draft.open {
        return;
    }
    let epoch = draft.epoch;
    let k = kit(dark);
    let mut panel = k.clone();
    panel.c.bg_base = k.c.surface_weak; // React dialog's bg-surface-alt
    let radius = if dark { 10.0 } else { 9.0 };
    let name = t("settings.nativeSnippet.name");
    let sql = t("query.snippets.template");
    let field = |label: &str, editor: El| {
        col().w_full().gap(6.0)
            .child(text(format!("{label} *")).font(CAPTION).color(k.c.text_muted))
            .child(editor)
    };
    let mut body = col().gap(16.0).w_full();
    if let Some(error) = &draft.error {
        body = body.child(
            text(error.clone()).key("snippet-add-error").w_full().wrap()
                .font(CAPTION).color(k.c.danger.fg),
        );
    }
    body = body
        .child(field(
            name,
            TextField::new("snippet-name-input", &draft.name)
                .height(36.0).label(name).placeholder(name).pad_x(12.0)
                .text_style(TextStyle::new(14.0, 400.0).lh(20.0))
                .radius(radius).fill(k.c.surface_base).border(1.0, k.c.border_base)
                .on_edit(|s: &mut AppState| &mut s.native_snippet_add.name)
                .build(dark),
        ))
        .child(field(
            sql,
            text_input(&draft.sql, CAPTION.mono())
                .key("snippet-sql-input").label(sql).w_full().h(118.0)
                .px(12.0).py(8.0).radius(radius).fill(k.c.surface_base)
                .border(1.0, k.c.border_base).color(k.c.text_base)
                .caret(k.c.text_base, k.selection())
                .on_edit(|s: &mut AppState| &mut s.native_snippet_add.sql)
                .on_submit(move |s: &mut AppState, _| {
                    if s.on_editor_settings() && s.native_snippet_add.open && s.native_snippet_add.epoch == epoch {
                        s.native_snippet_add.sql.insert("\n");
                    }
                }),
        ));
    let cancel = t("common.cancel");
    let save = t("common.save");
    Dialog::new("snippet-edit-dialog", t("query.snippets.add"))
        .open(true).open_epoch(epoch).width(576.0).fit().body_max_height(360.0)
        .header_pad((21.0, 16.0)).body_pad((21.0, 16.0)).footer_pad((21.0, 12.0))
        .on_close(move |s: &mut AppState| s.close_native_snippet_add(epoch))
        .child(body)
        .footer(
            button(k, "snippet-cancel", cancel, false)
                .child(text(cancel).font(CAPTION).color(k.c.text_muted))
                .on_press(move |s: &mut AppState| s.close_native_snippet_add(epoch)),
        )
        .footer(
            button(k, "snippet-save", save, true)
                .child(text(save).font(CAPTION).color(k.c.submit_fg))
                .on_press(move |s: &mut AppState| s.submit_native_snippet_add(epoch)),
        )
        .build(&panel, layer);
}

fn button(k: &Kit, key: &str, label: &str, primary: bool) -> El {
    let mut button = row().key(key).role("button").label(label).focusable(true)
        .h(28.0).px(8.0).gap(4.0).center().radius(9.0).focus_ring(2.0, 2.0, k.c.border_focus);
    if primary {
        button = button.fill(k.c.bg_accent).hover(k.c.submit_hover);
    } else {
        button = button.hover(k.c.surface_hover);
    }
    button
}
