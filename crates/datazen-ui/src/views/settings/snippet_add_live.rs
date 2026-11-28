//! The Editor Settings custom snippet list and dialog, backed by Native's name/SQL store.
//! Existing names are identity keys. Description, Rename, Builtin and Import/Export
//! do not mount here until the Native-backed capabilities are available.
use rune::{col, icon, row, text, text_input, El, TextStyle};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{ICON_CODE_2, ICON_PENCIL, ICON_PLUS, ICON_TRASH};
use crate::state_def::AppState;
use crate::ui::{dialog::Dialog, field::TextField, kit::kit};

const CAPTION: TextStyle = TextStyle::new(12.0, 400.0).lh(16.0);

pub(super) fn section(state: &AppState, k: &Kit) -> El {
    let add = t("query.snippets.add");
    let saved = state.host_snippets();
    let custom = t("query.snippets.custom");
    let caption = TextStyle::new(11.0, 500.0).lh(16.5).tracking(0.55);
    let mut card = col().key("settings-snippets").w_full().padding(15.0).gap(16.0)
        .radius(k.radius.lg).fill(k.c.surface_weak).border(1.0, k.c.border_base)
        .child(
            row().key("settings-snippets-header").w_full().gap(8.0).center_y().justify_between().flex_wrap()
                .child(
                    row().gap(8.0).center_y()
                        .child(icon(&ICON_CODE_2, 16.0).color(k.c.text_accent))
                        .child(text(t("query.snippets").to_uppercase()).key("settings-snippets-heading")
                            .font(TextStyle::new(12.0, 600.0).lh(16.0).tracking(0.6))
                            .color(k.c.text_base)),
                )
                .child(
                    button(k, "snippet-add", add, true)
                        .child(icon(&ICON_PLUS, 14.0).color(k.c.submit_fg))
                        .child(text(add).font(CAPTION).color(k.c.submit_fg))
                        .on_press(|state: &mut AppState| state.open_native_snippet_add()),
                ),
        );
    let mut group = col().key("settings-snippets-custom").w_full().gap(8.0)
        .child(text(format!("{} ({})", custom.to_uppercase(), saved.len()))
            .key("settings-snippets-custom-heading").font(caption).color(k.c.text_faint));
    if saved.is_empty() {
        group = group.child(
            col().key("settings-snippets-empty").w_full().padding(16.0).items_center()
                .radius(k.radius.sm).border(1.0, k.c.border_base)
                .child(text(t("query.snippets.emptyCustom")).font(CAPTION).color(k.c.text_faint).wrap()),
        );
    } else {
        let mut list = col().key("settings-snippets-custom-list").w_full()
            .radius(k.radius.sm).border(1.0, k.c.border_base).fill(k.c.surface_base);
        for (index, snippet) in saved.into_iter().enumerate() {
            if index != 0 {
                list = list.child(col().w_full().h(1.0).fill(k.c.border_base));
            }
            let edit_target = snippet.clone();
            let edit = t("query.snippets.edit");
            let delete = t("common.delete");
            let summary = snippet.sql.replace('\n', " ");
            list = list.child(
                row().key(format!("snippet-row-{index}")).w_full().px(11.0).py(10.0)
                    .gap(12.0).center_y().justify_between().hover(k.c.surface_hover)
                    .child(
                        col().grow(1.0).min_w(0.0).gap(2.0)
                            .child(row().gap(6.0).center_y().min_w(0.0)
                                .child(text(snippet.name.clone()).font(CAPTION.mono()).color(k.c.text_base)
                                    .min_w(0.0).shrink().truncate())
                                .child(row().radius(k.radius.sm).px(6.0).fill(k.alpha(k.c.bg_accent, 0.12))
                                    .child(text(custom).font(TextStyle::new(10.0, 500.0).lh(15.0)).color(k.c.text_accent))))
                            .child(
                                text(summary).font(caption.mono()).color(k.c.text_faint)
                                    .key(format!("snippet-summary-{index}")).w_full().min_w(0.0).truncate(),
                            ),
                    )
                    .child(
                        row().gap(4.0).min_w(60.0).center_y()
                            .child(
                                button(k, &format!("snippet-edit-{index}"), edit, false)
                                    .w(28.0).px(0.0).help(edit)
                                    .child(icon(&ICON_PENCIL, 14.0).color(k.c.text_muted))
                                    .on_press(move |s: &mut AppState| s.open_native_snippet_edit(&edit_target)),
                            )
                            .child(
                                button(k, &format!("snippet-delete-{index}"), delete, false)
                                    .w(28.0).px(0.0).help(delete)
                                    .child(icon(&ICON_TRASH, 14.0).color(k.c.danger.fg))
                                    .on_press(move |s: &mut AppState| s.request_snippet_delete(&snippet)),
                            ),
                    ),
            );
        }
        group = group.child(list);
    }
    card = card.child(group);
    if let Some(error) = &state.snippet_error {
        card = card.child(
            text(error.clone()).key("snippet-error").w_full().wrap().font(CAPTION).color(k.c.danger.fg),
        );
    }
    card
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
    let name_field = if draft.edit_target.is_some() {
        // This is an identity label, not a disabled/misleading Rename input.
        col().w_full().gap(6.0)
            .child(text(name).font(CAPTION).color(k.c.text_muted))
            .child(text(draft.name.text.clone()).key("snippet-edit-identity")
                .w_full().wrap().font(TextStyle::new(14.0, 400.0).lh(20.0).mono())
                .color(k.c.text_base))
    } else {
        field(
            name,
            TextField::new("snippet-name-input", &draft.name)
                .height(36.0).label(name).placeholder(name).pad_x(12.0)
                .text_style(TextStyle::new(14.0, 400.0).lh(20.0))
                .radius(radius).fill(k.c.surface_base).border(1.0, k.c.border_base)
                .on_edit(|s: &mut AppState| &mut s.native_snippet_add.name)
                .build(dark),
        )
    };
    body = body.child(name_field)
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
    let title = if draft.edit_target.is_some() { t("query.snippets.edit") } else { t("query.snippets.add") };
    Dialog::new("snippet-edit-dialog", title)
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
