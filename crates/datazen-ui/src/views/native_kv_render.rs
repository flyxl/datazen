//! Native Redis workbench layout and cached detail editor.
use super::{control, folders, tools, tree, AppState, EditMode, KvWorkbench, TextField, MAX_KEYS};
use rune::{
    col, row, text,
    tok::{self, type_},
    El,
};

fn ttl(value: i64) -> String {
    match value {
        -2 => "Key expired or missing".into(),
        -1 => "No expiry".into(),
        n if n >= 0 => format!("TTL: {n}s"),
        _ => "TTL unavailable".into(),
    }
}

pub(super) fn redis_workbench(state: &KvWorkbench, theme: tok::Theme, dark: bool) -> El {
    let keys = match &state.server_folders {
        Some(folder) => folders::view(folder, theme),
        None => tree::key_list(state, theme),
    };
    let mut left = col()
        .key("native-kv-list")
        .w(310.0)
        .h_full()
        .gap(8.0)
        .child(
            row()
                .gap(5.0)
                .child(
                    TextField::new("native-kv-pattern", &state.filter)
                        .height(28.0)
                        .width(210.0)
                        .on_edit(|s: &mut AppState| {
                            s.native_kv
                                .as_mut()
                                .map(|vm| &mut vm.filter)
                                .unwrap_or(&mut s.native_kv_unused_input)
                        })
                        .build(dark),
                )
                .child(
                    control("native-kv-pattern-apply", "Filter", theme)
                        .on_press(AppState::kv_apply_pattern),
                ),
        )
        .child(
            row()
                .gap(6.0)
                .child(text(format!("{} keys loaded", state.keys.len())).key("native-kv-loaded"))
                .child(
                    control(
                        "native-kv-tree-toggle",
                        if state.tree_mode {
                            "Flat list"
                        } else {
                            "Key tree"
                        },
                        theme,
                    )
                    .on_press(|s: &mut AppState| {
                        if let Some(vm) = &mut s.native_kv {
                            vm.tree_mode = !vm.tree_mode;
                        }
                    }),
                ),
        );

    left = left.child(
        row()
            .gap(6.0)
            .flex_wrap()
            .child(
                control(
                    "native-kv-type-filter",
                    &format!("Type: {}", state.key_type.token().unwrap_or("all")),
                    theme,
                )
                .on_press(|s: &mut AppState| s.kv_adjust_scan(true)),
            )
            .child(
                control(
                    "native-kv-persistent-filter",
                    if state.persistent {
                        "No expiry only"
                    } else {
                        "Any TTL"
                    },
                    theme,
                )
                .on_press(|s: &mut AppState| s.kv_adjust_scan(false)),
            ),
    );
    left = left.child(
        control("native-kv-exact", "Open exact key", theme).on_press(AppState::kv_lookup_exact_key),
    );
    left = left.child(
        control(
            "native-kv-server-folders",
            if state.server_folders.is_some() {
                "Back to scanned list"
            } else {
                "Browse server folders"
            },
            theme,
        )
        .on_press(AppState::kv_toggle_server_folders),
    );
    if state.cluster {
        left = left.child(
            text("Cluster browsing covers one shard. Exact names search all shards.")
                .key("native-kv-cluster-scope")
                .font(type_::LABEL)
                .wrap(),
        );
    }
    if state.server_folders.is_none() && state.scan_request.is_some() {
        left = left.child(text("Loading keys…").key("native-kv-scan-loading"));
    } else if state.server_folders.is_none() && state.keys.len() == MAX_KEYS {
        left = left.child(text("Key list limit reached.").key("native-kv-limit"));
    } else if state.server_folders.is_none() && (!state.started || state.cursor != 0) {
        let label = if state.scan_error.is_some() {
            "Retry key scan"
        } else {
            "Load more keys"
        };
        left = left.child(control("native-kv-more", label, theme).on_press(AppState::kv_next_page));
    } else if state.server_folders.is_none() && state.keys.is_empty() {
        left = left.child(
            text(format!("No keys in DB {}.", state.db_index.unwrap_or(0))).key("native-kv-empty"),
        );
    }
    if let Some(error) = &state.scan_error {
        left = left.child(text(error.clone()).key("native-kv-scan-error").w_full());
    }
    left = left.child(
        col()
            .grow(1.0)
            .min_h(0.0)
            .w_full()
            .scroll(rune::ScrollSpec {
                stick_bottom: false,
            })
            .child(keys),
    );

    let mut detail = col()
        .key("native-kv-detail")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .gap(8.0);
    if let Some(key) = &state.selected {
        detail = detail.child(
            text(key.clone())
                .key("native-kv-selected")
                .font(type_::TITLE),
        );
    }
    if state.detail_request.is_some() {
        detail = detail.child(text("Loading value…").key("native-kv-detail-loading"));
    } else if let Some(error) = &state.detail_error {
        detail = detail.child(text(error.clone()).key("native-kv-detail-error"));
    } else if let Some(value) = &state.detail {
        detail = detail
            .child(text(value.key_type.clone()).key("native-kv-type"))
            .child(text(ttl(value.ttl)).key("native-kv-ttl"))
            .child(
                col().grow(1.0).min_h(0.0).w_full().scroll_xy().child(
                    text(value.preview.clone())
                        .key("native-kv-value")
                        .color(theme.text),
                ),
            );
    } else {
        detail = detail.child(text("Select a key to preview its value."));
    }

    if let Some(message) = &state.edit_notice {
        detail = detail.child(text(message.clone()).key("native-kv-edit-notice").w_full());
    }
    if state.writable {
        detail = detail.child(edit_panel(state, theme, dark));
    }

    let mut picker = row()
        .w_full()
        .gap(6.0)
        .flex_wrap()
        .key("native-kv-db-options");
    if state.db_picker_open {
        for &db in &state.databases {
            picker = picker.child(
                control(
                    &format!("native-kv-choose-db-{db}"),
                    &format!(
                        "DB {db}{}",
                        if state.db_index == Some(db) {
                            " (current)"
                        } else {
                            ""
                        }
                    ),
                    theme,
                )
                .on_press(move |s: &mut AppState| s.kv_choose_db(db)),
            );
        }
    }
    let mut nav = row()
        .w_full()
        .gap(6.0)
        .flex_wrap()
        .key("native-kv-section-tabs");
    for (key, label, section) in [
        ("native-kv-tab-details", "Key details", None),
        (
            "native-kv-tab-console",
            "Console",
            Some(tools::Section::Console),
        ),
        (
            "native-kv-tab-monitor",
            "Monitor",
            Some(tools::Section::Monitor),
        ),
        (
            "native-kv-tab-pubsub",
            "Pub/Sub",
            Some(tools::Section::PubSub),
        ),
    ] {
        let current = if let Some(section) = section {
            state.tools.open && state.tools.section == section
        } else {
            !state.tools.open
        };
        let tab = control(key, label, theme).on_press(move |s: &mut AppState| {
            s.kv_show_tools(section);
        });
        nav = nav.child(if current { tab.fill(theme.hover) } else { tab });
    }
    nav = nav.child(
        control("native-kv-quick-overview", "Open overview", theme).on_press(|s: &mut AppState| {
            if s.kv_show_tools(Some(tools::Section::Monitor)) {
                s.kv_tool(super::KvTool::Overview);
            }
        }),
    );
    col()
        .key("native-redis-workbench")
        .w_full()
        .h_full()
        .px(16.0)
        .py(12.0)
        .gap(10.0)
        .color(theme.text)
        .child(
            row()
                .w_full()
                .gap(12.0)
                .center_y()
                .child(
                    text(format!(
                        "Redis · DB {}{}",
                        state.db_index.unwrap_or(0),
                        if state.writable { "" } else { " · Read only" }
                    ))
                    .key("native-kv-context")
                    .font(type_::TITLE),
                )
                .child(control("native-kv-db-picker", "Choose DB", theme).on_press(
                    |s: &mut AppState| {
                        if let Some(vm) = s.native_kv.as_mut() {
                            vm.db_picker_open = !vm.db_picker_open;
                        }
                    },
                ))
                .child(
                    control("native-kv-db-prev", "Previous DB", theme)
                        .on_press(|s: &mut AppState| s.kv_switch_db(-1)),
                )
                .child(
                    control("native-kv-db-next", "Next DB", theme)
                        .on_press(|s: &mut AppState| s.kv_switch_db(1)),
                )
                .child(
                    control(
                        "native-kv-tool-toggle",
                        if state.tools.open {
                            "Key details"
                        } else {
                            "Redis tools"
                        },
                        theme,
                    )
                    .on_press(AppState::kv_toggle_tools),
                )
                .child(
                    control("native-kv-disconnect", "Disconnect", theme).on_press(
                        |s: &mut AppState| {
                            if !s.kv_can_leave() {
                                return;
                            }
                            s.disconnect();
                            s.active = crate::types::Section::Connections;
                        },
                    ),
                ),
        )
        .child(nav)
        .child(picker)
        .child(
            row()
                .w_full()
                .min_h(0.0)
                .grow(1.0)
                .gap(16.0)
                .child(left)
                .child(if state.tools.open {
                    tools::view(state, theme, dark)
                } else {
                    detail
                }),
        )
}

fn edit_panel(state: &KvWorkbench, theme: tok::Theme, dark: bool) -> El {
    if let Some(draft) = &state.draft {
        let mut panel = col().key("native-kv-editor").w_full().gap(5.0);
        if state.edit_request.is_some() {
            return panel.child(text("Saving to Redis…").key("native-kv-edit-loading"));
        }
        let label = match &draft.mode {
            EditMode::String => "String value".to_owned(),
            EditMode::Hash(field) => format!("Hash field: {field}"),
            EditMode::Ttl(_) => "TTL in seconds (-1 removes expiry)".to_owned(),
            EditMode::Collection(_, label) => label.clone(),
        };
        panel = panel.child(text(label).key("native-kv-edit-label")).child(
            TextField::new("native-kv-edit-input", &draft.value)
                .height(28.0)
                .width(320.0)
                .on_edit(|s: &mut AppState| s.native_kv_edit_input())
                .build(dark),
        );
        return panel.child(
            row()
                .gap(6.0)
                .child(control("native-kv-save", "Save", theme).on_press(AppState::kv_save))
                .child(
                    control("native-kv-discard", "Discard draft", theme)
                        .on_press(AppState::kv_discard),
                ),
        );
    }
    let Some(value) = &state.detail else {
        return col();
    };
    if value.ttl == -2 {
        return col();
    }
    let mut buttons = row().key("native-kv-edit-actions").gap(6.0).flex_wrap();
    let key_type = value.key_type.clone();
    let ttl = value.ttl.to_string();
    buttons = buttons.child(control("native-kv-edit-ttl", "Change TTL", theme).on_press(
        move |s: &mut AppState| {
            s.kv_begin_edit(EditMode::Ttl(key_type.clone()), ttl.clone());
        },
    ));
    if let Some(original) = &value.string_edit {
        let original = original.clone();
        buttons = buttons.child(
            control("native-kv-edit-string", "Edit String", theme).on_press(
                move |s: &mut AppState| {
                    s.kv_begin_edit(EditMode::String, original.clone());
                },
            ),
        );
    }
    for (index, (field, original)) in value.hash_fields.iter().cloned().enumerate() {
        buttons = buttons.child(
            control(
                &format!("native-kv-edit-hash-{index}"),
                &format!("Edit {field}"),
                theme,
            )
            .on_press(move |s: &mut AppState| {
                s.kv_begin_edit(EditMode::Hash(field.clone()), original.clone());
            }),
        );
    }
    for (index, (label, target, original)) in value.collection_edits.iter().cloned().enumerate() {
        buttons = buttons.child(
            control(&format!("native-kv-edit-collection-{index}"), &label, theme).on_press(
                move |s: &mut AppState| {
                    s.kv_begin_edit(
                        EditMode::Collection(target.clone(), label.clone()),
                        original.clone(),
                    )
                },
            ),
        );
    }
    buttons
}
