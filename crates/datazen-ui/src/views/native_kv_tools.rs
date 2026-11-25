//! Redis utility views; all network activity stays in typed host jobs.
use super::{control, AppState, KvTool, KvWorkbench, TextEdit, TextField};
use datazen_ui_api::native_kv::RedisValueCodec;
use rune::{col, row, text, tok::Theme, El};

#[path = "native_kv_console.rs"]
mod console_completions;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Section {
    #[default]
    All,
    Console,
    Monitor,
    PubSub,
}

impl Section {
    fn shows(self, section: Self) -> bool {
        self == Self::All || self == section
    }
}

#[derive(Default)]
pub(super) struct ToolPanel {
    pub open: bool,
    pub section: Section,
    pub request: Option<u64>,
    pub output: String,
    command: TextEdit,
    console_history: Vec<String>,
    console_queue: Vec<String>,
    pub(super) console_set_review: Option<(String, String)>,
    console_index: Option<usize>,
    console_unsent: String,
    channel: TextEdit,
    patterns: TextEdit,
    stream_group: TextEdit,
    stream_entry: TextEdit,
    codec: RedisValueCodec,
    export_keys: std::collections::BTreeSet<String>,
    delete_review: Option<Vec<String>>,
    delete_confirmation: TextEdit,
    message: TextEdit,
    key_input: TextEdit,
    search: TextEdit,
    value_input: TextEdit,
}

impl ToolPanel {
    pub(super) fn clear_marked(&mut self) {
        self.export_keys.clear();
        self.delete_review = None;
        self.delete_confirmation = TextEdit::new("");
    }

    fn recall(&mut self, older: bool) {
        let len = self.console_history.len();
        if len == 0 {
            return;
        }
        let index = match (older, self.console_index) {
            (true, None) => {
                self.console_unsent = self.command.text.clone();
                Some(len - 1)
            }
            (true, Some(i)) => Some(i.saturating_sub(1)),
            (false, Some(i)) if i + 1 < len => Some(i + 1),
            (false, Some(_)) => None,
            (false, None) => return,
        };
        self.console_index = index;
        self.command =
            TextEdit::new(index.map_or(&self.console_unsent, |i| &self.console_history[i]));
    }

    pub(super) fn record_console(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() || text.len() > 4096 {
            return;
        }
        if self.console_history.last().is_none_or(|last| last != text) {
            if self.console_history.len() == 30 {
                self.console_history.remove(0);
            }
            self.console_history.push(text.to_owned());
        }
        self.console_index = None;
        self.console_unsent.clear();
    }
}

fn event(s: &mut AppState, op: impl FnOnce(&ToolPanel) -> KvTool) {
    if let Some(vm) = &s.native_kv {
        let tool = op(&vm.tools);
        s.kv_tool(tool);
    }
}

fn terminal_submit(s: &mut AppState) {
    let Some(vm) = s.native_kv.as_mut() else {
        return;
    };
    let text = vm.tools.command.text.clone();
    vm.tools.console_set_review = if vm.writable {
        console_completions::simple_set(&text)
    } else {
        None
    };
    if vm.tools.console_set_review.is_none() {
        s.kv_tool(KvTool::Console(text));
    }
}

pub(super) fn view(state: &KvWorkbench, theme: Theme, dark: bool) -> El {
    let panel = &state.tools;
    let button = |key, label, op: KvTool| {
        control(key, label, theme).on_press(move |s: &mut AppState| s.kv_tool(op.clone()))
    };
    let command = TextField::new("native-kv-console-input", &panel.command)
        .height(28.0)
        .width(380.0)
        .on_edit(|s: &mut AppState| {
            s.native_kv
                .as_mut()
                .map(|vm| &mut vm.tools.command)
                .unwrap_or(&mut s.native_kv_unused_input)
        })
        .on_submit(|s: &mut AppState, _| terminal_submit(s))
        .on_key(|s: &mut AppState, event: &rune::KeyEvent, _| {
            if event.mods.shift || event.mods.alt || event.mods.cmd || event.mods.ctrl {
                return false;
            }
            let Some(vm) = s.native_kv.as_mut() else {
                return false;
            };
            match event.key {
                rune::Key::Tab => {
                    let first = console_completions::completions(&vm.tools.command.text, &vm.keys)
                        .into_iter()
                        .next();
                    if let Some(first) = first {
                        vm.tools.command.replace_text(first.text);
                        true
                    } else {
                        false
                    }
                }
                rune::Key::Up | rune::Key::Down if !vm.tools.console_history.is_empty() => {
                    vm.tools.recall(event.key == rune::Key::Up);
                    true
                }
                _ => false,
            }
        })
        .build(dark);
    let mut suggestions = row()
        .gap(4.0)
        .flex_wrap()
        .key("native-kv-completion-options");
    for candidate in console_completions::completions(&panel.command.text, &state.keys) {
        suggestions = suggestions.child(control(&candidate.id, &candidate.label, theme).on_press(
            move |s: &mut AppState| {
                if let Some(vm) = s.native_kv.as_mut() {
                    vm.tools.command.replace_text(candidate.text.clone());
                }
            },
        ));
    }
    let mut console = row()
        .gap(6.0)
        .flex_wrap()
        .child(
            text(format!("db{}>", state.db_index.unwrap_or(0)))
                .key("native-kv-console-prompt")
                .color(theme.text),
        )
        .child(command)
        .child(
            control("native-kv-console-run", "Run", theme).on_press(|s: &mut AppState| {
                event(s, |p| KvTool::Console(p.command.text.clone()));
            }),
        );
    if !panel.console_history.is_empty() {
        for (key, label, older) in [
            ("native-kv-console-older", "Older", true),
            ("native-kv-console-newer", "Newer", false),
        ] {
            console = console.child(control(key, label, theme).on_press(
                move |s: &mut AppState| {
                    if let Some(vm) = s.native_kv.as_mut() {
                        vm.tools.recall(older);
                    }
                },
            ));
        }
    }
    console = console.child(control("native-kv-console-queue", "Queue", theme).on_press(
        |s: &mut AppState| {
            if let Some(vm) = s.native_kv.as_mut() {
                let command = vm.tools.command.text.trim();
                if !command.is_empty() && command.len() <= 4096 && vm.tools.console_queue.len() < 8
                {
                    vm.tools.console_queue.push(command.to_owned());
                    vm.tools.command = TextEdit::new("");
                }
            }
        },
    ));
    let mut queued = row().gap(6.0).flex_wrap();
    if !panel.console_queue.is_empty() {
        queued = queued
            .child(
                text(format!("{} queued", panel.console_queue.len()))
                    .key("native-kv-console-count"),
            )
            .child(
                control("native-kv-console-run-batch", "Run queued commands", theme).on_press(
                    |s: &mut AppState| event(s, |p| KvTool::ConsoleBatch(p.console_queue.clone())),
                ),
            )
            .child(
                control("native-kv-console-clear-queue", "Clear queue", theme).on_press(
                    |s: &mut AppState| {
                        if let Some(vm) = s.native_kv.as_mut() {
                            vm.tools.console_queue.clear();
                        }
                    },
                ),
            );
    }
    let channel = TextField::new("native-kv-channel", &panel.channel)
        .height(28.0)
        .width(200.0)
        .on_edit(|s: &mut AppState| {
            s.native_kv
                .as_mut()
                .map(|vm| &mut vm.tools.channel)
                .unwrap_or(&mut s.native_kv_unused_input)
        })
        .build(dark);
    let patterns = TextField::new("native-kv-channel-patterns", &panel.patterns)
        .height(28.0)
        .width(160.0)
        .on_edit(|s: &mut AppState| {
            s.native_kv
                .as_mut()
                .map(|vm| &mut vm.tools.patterns)
                .unwrap_or(&mut s.native_kv_unused_input)
        })
        .build(dark);
    let message = TextField::new("native-kv-message", &panel.message)
        .height(28.0)
        .width(300.0)
        .on_edit(|s: &mut AppState| {
            s.native_kv
                .as_mut()
                .map(|vm| &mut vm.tools.message)
                .unwrap_or(&mut s.native_kv_unused_input)
        })
        .build(dark);
    let mut pubsub = row()
        .gap(6.0)
        .flex_wrap()
        .child(text("Channels").color(theme.text))
        .child(channel)
        .child(text("Patterns").color(theme.text))
        .child(patterns)
        .child(
            control("native-kv-subscribe", "Subscribe", theme).on_press(|s: &mut AppState| {
                event(s, |p| KvTool::Subscribe {
                    channels: p.channel.text.clone(),
                    patterns: p.patterns.text.clone(),
                });
            }),
        )
        .child(button(
            "native-kv-read-messages",
            "Read messages",
            KvTool::Messages,
        ))
        .child(button(
            "native-kv-unsubscribe",
            "Unsubscribe",
            KvTool::Unsubscribe,
        ));
    if state.writable {
        pubsub =
            pubsub
                .child(message)
                .child(control("native-kv-publish", "Publish", theme).on_press(
                    |s: &mut AppState| {
                        event(s, |p| KvTool::Publish {
                            channel: p.channel.text.clone(),
                            message: p.message.text.clone(),
                        })
                    },
                ));
    }
    let search = TextField::new("native-kv-value-search", &panel.search)
        .height(28.0)
        .width(260.0)
        .on_edit(|s: &mut AppState| {
            s.native_kv
                .as_mut()
                .map(|vm| &mut vm.tools.search)
                .unwrap_or(&mut s.native_kv_unused_input)
        })
        .build(dark);
    let value_search = row()
        .gap(6.0)
        .flex_wrap()
        .child(search)
        .child(
            control(
                "native-kv-value-search-start",
                "Search String values",
                theme,
            )
            .on_press(|s: &mut AppState| {
                if let Some(vm) = &s.native_kv {
                    let op = KvTool::SearchStart {
                        query: vm.tools.search.text.clone(),
                        pattern: vm.pattern.clone(),
                    };
                    s.kv_tool(op);
                }
            }),
        )
        .child(button(
            "native-kv-value-search-next",
            "Next search batch",
            KvTool::SearchNext,
        ))
        .child(button(
            "native-kv-value-search-stop",
            "Stop search",
            KvTool::SearchStop,
        ));
    let management = if state.writable {
        let key = TextField::new("native-kv-manage-key", &panel.key_input)
            .height(28.0)
            .width(220.0)
            .on_edit(|s: &mut AppState| {
                s.native_kv
                    .as_mut()
                    .map(|vm| &mut vm.tools.key_input)
                    .unwrap_or(&mut s.native_kv_unused_input)
            })
            .build(dark);
        let value = TextField::new("native-kv-manage-value", &panel.value_input)
            .height(28.0)
            .width(220.0)
            .on_edit(|s: &mut AppState| {
                s.native_kv
                    .as_mut()
                    .map(|vm| &mut vm.tools.value_input)
                    .unwrap_or(&mut s.native_kv_unused_input)
            })
            .build(dark);
        let mut controls = row().gap(6.0).flex_wrap().child(key).child(value).child(
            control("native-kv-create", "Create String", theme).on_press(|s: &mut AppState| {
                event(s, |p| KvTool::CreateString {
                    key: p.key_input.text.clone(),
                    value: p.value_input.text.clone(),
                });
            }),
        );
        if let Some(selected) = &state.selected {
            let selected_rename = selected.clone();
            let selected_delete = selected.clone();
            controls = controls
                .child(
                    control("native-kv-rename", "Rename selected", theme).on_press(
                        move |s: &mut AppState| {
                            event(s, |p| KvTool::RenameKey {
                                key: selected_rename.clone(),
                                target: p.key_input.text.clone(),
                            });
                        },
                    ),
                )
                .child(
                    control("native-kv-delete", "Delete exact selected key", theme).on_press(
                        move |s: &mut AppState| {
                            event(s, |p| KvTool::DeleteKey {
                                key: selected_delete.clone(),
                                confirmation: p.key_input.text.clone(),
                            });
                        },
                    ),
                );
        }
        col()
            .gap(4.0)
            .child(
                text("Key management: name / value. Type the selected name to confirm deletion.")
                    .color(theme.text),
            )
            .child(controls)
    } else {
        col()
    };
    let selected = state.selected.clone();
    let mut files = row().gap(6.0).flex_wrap();
    if let Some(key) = selected.as_ref() {
        files = files.child(button(
            "native-kv-key-properties",
            "Key properties",
            KvTool::KeyProperties(key.clone()),
        ));
        if state
            .detail
            .as_ref()
            .is_some_and(|detail| detail.key_type == "string")
        {
            files = files
                .child(
                    control(
                        "native-kv-codec",
                        &format!("Codec: {}", panel.codec.token()),
                        theme,
                    )
                    .on_press(|s: &mut AppState| {
                        if let Some(vm) = s.native_kv.as_mut() {
                            vm.tools.codec = vm.tools.codec.next();
                        }
                    }),
                )
                .child(button(
                    "native-kv-decode-string",
                    "Decode selected String",
                    KvTool::DecodeString {
                        key: key.clone(),
                        codec: panel.codec,
                    },
                ));
        }
        if state
            .detail
            .as_ref()
            .is_some_and(|detail| detail.key_type == "stream")
        {
            let group = TextField::new("native-kv-stream-group", &panel.stream_group)
                .height(28.0)
                .width(160.0)
                .on_edit(|s: &mut AppState| {
                    s.native_kv
                        .as_mut()
                        .map(|vm| &mut vm.tools.stream_group)
                        .unwrap_or(&mut s.native_kv_unused_input)
                })
                .build(dark);
            files = files
                .child(button(
                    "native-kv-stream-groups",
                    "Stream groups",
                    KvTool::StreamGroups(key.clone()),
                ))
                .child(text("Group").color(theme.text))
                .child(group);
            for (control_key, label, mode) in [
                ("native-kv-stream-consumers", "Consumers", 0),
                ("native-kv-stream-pending", "Pending entries", 1),
                (
                    "native-kv-stream-create",
                    "Create group from first entry",
                    2,
                ),
            ] {
                if mode == 2 && !state.writable {
                    continue;
                }
                let stream = key.clone();
                files = files.child(control(control_key, label, theme).on_press(
                    move |s: &mut AppState| {
                        event(s, |p| match mode {
                            0 => KvTool::StreamConsumers {
                                key: stream.clone(),
                                group: p.stream_group.text.clone(),
                            },
                            1 => KvTool::StreamPending {
                                key: stream.clone(),
                                group: p.stream_group.text.clone(),
                            },
                            _ => KvTool::StreamCreateGroup {
                                key: stream.clone(),
                                group: p.stream_group.text.clone(),
                            },
                        });
                    },
                ));
            }
            if state.writable {
                let id = TextField::new("native-kv-stream-entry", &panel.stream_entry)
                    .height(28.0)
                    .width(180.0)
                    .on_edit(|s: &mut AppState| {
                        s.native_kv
                            .as_mut()
                            .map(|vm| &mut vm.tools.stream_entry)
                            .unwrap_or(&mut s.native_kv_unused_input)
                    })
                    .build(dark);
                let stream = key.clone();
                files = files
                    .child(text("Pending ID").color(theme.text))
                    .child(id)
                    .child(
                        control("native-kv-stream-ack", "Acknowledge exact ID", theme).on_press(
                            move |s: &mut AppState| {
                                event(s, |p| KvTool::StreamAck {
                                    key: stream.clone(),
                                    group: p.stream_group.text.clone(),
                                    entry: p.stream_entry.text.clone(),
                                });
                            },
                        ),
                    );
            }
        }
    }
    if let Some(key) = selected {
        files = files.child(
            control(
                "native-kv-export",
                if panel.export_keys.is_empty() {
                    "Export selected ZIP"
                } else {
                    "Export marked keys ZIP"
                },
                theme,
            )
            .on_press_tx(move |s: &mut AppState, tx: &mut rune::Tx| {
                let Some(vm) = s.native_kv.as_ref() else {
                    return;
                };
                let keys: Vec<String> = if vm.tools.export_keys.is_empty() {
                    vec![key.clone()]
                } else {
                    vm.tools.export_keys.iter().cloned().collect()
                };
                let route = s
                    .native_kv
                    .as_ref()
                    .map(|vm| (vm.connection_id.clone(), vm.session_generation, vm.db_index));
                tx.save_file(
                    rune::FileDialogOptions {
                        title: "Export selected Redis keys".into(),
                        file_name: Some("redis-keys.zip".into()),
                        ..Default::default()
                    }
                    .filter("Redis ZIP", &["zip"]),
                    move |s: &mut AppState, picked, _| {
                        if same_route(s, route.as_ref()) {
                            if let Ok(Some(path)) = picked {
                                s.kv_tool(KvTool::Export {
                                    keys: keys.clone(),
                                    path,
                                });
                            }
                        }
                    },
                );
            }),
        );
    }
    if let Some(key) = state.selected.as_ref() {
        let key = key.clone();
        let marked = panel.export_keys.contains(&key);
        if marked || panel.export_keys.len() < 128 {
            files = files.child(
                control(
                    "native-kv-mark-export",
                    if marked { "Unmark key" } else { "Mark key" },
                    theme,
                )
                .on_press(move |s: &mut AppState| {
                    if let Some(vm) = s.native_kv.as_mut() {
                        vm.tools.delete_review = None;
                        vm.tools.delete_confirmation = TextEdit::new("");
                        if !vm.tools.export_keys.remove(&key) && vm.tools.export_keys.len() < 128 {
                            vm.tools.export_keys.insert(key.clone());
                        }
                    }
                }),
            );
        }
    }
    if !panel.export_keys.is_empty() {
        files = files.child(
            control(
                "native-kv-clear-export",
                &format!("Clear {} marked", panel.export_keys.len()),
                theme,
            )
            .on_press(|s: &mut AppState| {
                if let Some(vm) = s.native_kv.as_mut() {
                    vm.tools.clear_marked();
                }
            }),
        );
    }
    if state.writable {
        files = files.child(
            control("native-kv-import", "Import ZIP without overwriting", theme).on_press_tx(
                |s: &mut AppState, tx: &mut rune::Tx| {
                    let route = s
                        .native_kv
                        .as_ref()
                        .map(|vm| (vm.connection_id.clone(), vm.session_generation, vm.db_index));
                    tx.open_file(
                        rune::FileDialogOptions {
                            title: "Import trusted Redis archive without overwriting existing keys"
                                .into(),
                            ..Default::default()
                        }
                        .filter("Redis ZIP", &["zip"]),
                        move |s: &mut AppState, picked, _| {
                            if same_route(s, route.as_ref()) {
                                if let Ok(Some(path)) = picked {
                                    s.kv_tool(KvTool::Import { path });
                                }
                            }
                        },
                    );
                },
            ),
        );
    }
    let mut review = col();
    if state.writable && panel.export_keys.len() > 1 {
        if let Some(keys) = panel.delete_review.as_ref() {
            let confirmation = format!("DELETE {}", keys.len());
            let label = format!(
                "To delete these keys from DB {}, type {confirmation}.",
                state.db_index.unwrap_or(0)
            );
            let names = keys
                .iter()
                .map(|k| format!("{k:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            review = review
                .child(text(label).color(theme.text))
                .child(
                    col()
                        .h(80.0)
                        .scroll_xy()
                        .child(text(names).key("native-kv-delete-preview")),
                )
                .child(
                    row()
                        .gap(5.0)
                        .child(
                            TextField::new("native-kv-delete-confirm", &panel.delete_confirmation)
                                .height(28.0)
                                .width(170.0)
                                .on_edit(|s: &mut AppState| {
                                    s.native_kv
                                        .as_mut()
                                        .map(|vm| &mut vm.tools.delete_confirmation)
                                        .unwrap_or(&mut s.native_kv_unused_input)
                                })
                                .build(dark),
                        )
                        .child(
                            control("native-kv-delete-marked", "Delete reviewed keys", theme)
                                .on_press(|s: &mut AppState| {
                                    let Some(p) = s.native_kv.as_ref().map(|vm| &vm.tools) else {
                                        return;
                                    };
                                    if let Some(keys) = p.delete_review.clone() {
                                        let confirmation = p.delete_confirmation.text.clone();
                                        s.kv_tool(KvTool::DeleteMarked { keys, confirmation });
                                    }
                                }),
                        )
                        .child(
                            control("native-kv-cancel-marked", "Cancel", theme).on_press(
                                |s: &mut AppState| {
                                    if let Some(vm) = s.native_kv.as_mut() {
                                        vm.tools.delete_review = None;
                                        vm.tools.delete_confirmation = TextEdit::new("");
                                    }
                                },
                            ),
                        ),
                );
        } else {
            review = review.child(
                control(
                    "native-kv-review-marked",
                    "Review marked keys for deletion",
                    theme,
                )
                .on_press(|s: &mut AppState| {
                    if let Some(vm) = s.native_kv.as_mut() {
                        vm.tools.delete_review =
                            Some(vm.tools.export_keys.iter().cloned().collect());
                        vm.tools.delete_confirmation = TextEdit::new("");
                    }
                }),
            );
        }
    }
    let mut root = col()
        .key("native-kv-tools")
        .min_w(0.0)
        .grow(1.0)
        .min_h(0.0)
        .gap(8.0);
    if panel.section.shows(Section::Console) {
        root = root
            .child(text("Read-only console").color(theme.text))
            .child(console)
            .child(suggestions)
            .child(queued)
            .child(console_completions::set_review(state, theme));
    }
    if panel.section.shows(Section::Monitor) {
        root = root
            .child(text("Server and monitoring").color(theme.text))
            .child(
                row()
                    .gap(6.0)
                    .flex_wrap()
                    .child(button("native-kv-overview", "Overview", KvTool::Overview))
                    .child(button(
                        "native-kv-memory-sample",
                        "Sample large keys",
                        KvTool::MemorySample,
                    ))
                    .child(button("native-kv-info", "Server INFO", KvTool::Info))
                    .child(button("native-kv-slowlog", "Slow log", KvTool::SlowLog))
                    .child(button(
                        "native-kv-cluster-nodes",
                        "Cluster nodes",
                        KvTool::ClusterNodes,
                    ))
                    .child(button(
                        "native-kv-monitor-start",
                        "Start monitor",
                        KvTool::MonitorStart,
                    ))
                    .child(button(
                        "native-kv-monitor-read",
                        "Read monitor",
                        KvTool::MonitorRead,
                    ))
                    .child(button(
                        "native-kv-monitor-stop",
                        "Stop monitor",
                        KvTool::MonitorStop,
                    )),
            );
    }
    if panel.section.shows(Section::PubSub) {
        root = root
            .child(text("Pub/Sub: comma-separated channels or patterns").color(theme.text))
            .child(pubsub);
    }
    if panel.section == Section::All {
        root = root
            .child(value_search)
            .child(management)
            .child(files)
            .child(review);
    }
    root.child(
        col().grow(1.0).min_h(0.0).w_full().scroll_xy().child(
            text(if panel.request.is_some() {
                "Working…".into()
            } else {
                panel.output.clone()
            })
            .key("native-kv-tool-output")
            .color(theme.text),
        ),
    )
}

fn same_route(state: &AppState, route: Option<&(String, u32, Option<u32>)>) -> bool {
    state
        .native_kv
        .as_ref()
        .zip(route)
        .is_some_and(|(vm, (id, session, db))| {
            &vm.connection_id == id && vm.session_generation == *session && vm.db_index == *db
        })
}
