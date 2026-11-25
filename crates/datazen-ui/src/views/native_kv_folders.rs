//! Server-paged namespace browser. Local Flat/Tree layouts remain available.
use std::collections::BTreeSet;

use rune::{col, row, text, tok::Theme, El};

use super::{control, AppState, KvFolderChild, KvFolderPage, KvReadKind, KvReply, MAX_KEYS};

#[derive(Default)]
pub(super) struct State {
    path: Vec<String>,
    entries: Vec<KvFolderChild>,
    names: BTreeSet<(bool, String)>,
    cursor: u64,
    started: bool,
    request: Option<u64>,
    error: Option<String>,
}

impl State {
    fn prefix(&self) -> &str {
        self.path.last().map(String::as_str).unwrap_or("")
    }

    fn reset_page(&mut self) {
        self.entries.clear();
        self.names.clear();
        self.cursor = 0;
        self.started = false;
        self.request = None;
        self.error = None;
    }

    pub(super) fn receive(&mut self, id: u64, prefix: &str, result: Result<KvReply, String>) {
        if self.request != Some(id) || self.prefix() != prefix {
            return;
        }
        self.request = None;
        match result {
            Ok(KvReply::Folder(KvFolderPage { children, cursor })) => {
                self.cursor = cursor;
                self.started = true;
                for row in children.into_iter().take(MAX_KEYS - self.entries.len()) {
                    let (folder, name) = match &row {
                        KvFolderChild::Folder { prefix } => (true, prefix),
                        KvFolderChild::Key { key } => (false, key),
                    };
                    if self.names.insert((folder, name.clone())) {
                        self.entries.push(row);
                    }
                }
            }
            Err(error) => self.error = Some(error),
            _ => self.error = Some("Unexpected Redis folder response.".into()),
        }
    }
}

impl AppState {
    pub(super) fn kv_toggle_server_folders(&mut self) {
        if !self.kv_can_leave() {
            return;
        }
        if let Some(vm) = self.native_kv.as_mut() {
            if vm.server_folders.take().is_none() {
                vm.server_folders = Some(State::default());
                self.kv_next_folder_page();
            }
        }
    }

    fn kv_move_folder(&mut self, child: Option<String>) {
        if !self.kv_can_leave() {
            return;
        }
        if let Some(folder) = self
            .native_kv
            .as_mut()
            .and_then(|vm| vm.server_folders.as_mut())
        {
            match child {
                Some(child) if folder.path.len() < 16 => folder.path.push(child),
                Some(_) => return,
                None => {
                    folder.path.pop();
                }
            }
            folder.reset_page();
            self.kv_next_folder_page();
        }
    }

    fn kv_next_folder_page(&mut self) {
        let Some(vm) = self.native_kv.as_mut() else {
            return;
        };
        let Some(folder) = vm.server_folders.as_ref() else {
            return;
        };
        if folder.request.is_some()
            || (folder.started && folder.cursor == 0)
            || folder.entries.len() >= MAX_KEYS
        {
            return;
        }
        let request = vm.request(KvReadKind::Folder {
            prefix: folder.prefix().into(),
            cursor: folder.cursor,
            key_type: vm.key_type,
        });
        if let Some(folder) = vm.server_folders.as_mut() {
            folder.request = Some(request.request_id);
            folder.error = None;
        }
        self.dispatch_kv(request);
    }
}

pub(super) fn view(state: &State, theme: Theme) -> El {
    let prefix = state.prefix();
    let mut out = col()
        .key("native-kv-server-folder-grid")
        .w_full()
        .gap(5.0)
        .child(
            row()
                .w_full()
                .gap(4.0)
                .flex_wrap()
                .child(
                    text(if prefix.is_empty() {
                        "Folder: root".into()
                    } else {
                        format!("Folder: {prefix}")
                    })
                    .key("native-kv-current-folder")
                    .color(theme.text),
                )
                .child(if prefix.is_empty() {
                    col()
                } else {
                    control("native-kv-server-folder-up", "Up", theme)
                        .on_press(|s: &mut AppState| s.kv_move_folder(None))
                }),
        );
    for (i, entry) in state.entries.iter().enumerate() {
        match entry {
            KvFolderChild::Folder { prefix } => {
                let prefix = prefix.clone();
                out = out.child(
                    control(&format!("native-kv-server-folder-{i}"), &prefix, theme)
                        .on_press(move |s: &mut AppState| s.kv_move_folder(Some(prefix.clone()))),
                );
            }
            KvFolderChild::Key { key } => {
                let key = key.clone();
                out = out.child(
                    control(&format!("native-kv-server-leaf-{i}"), &key, theme)
                        .on_press(move |s: &mut AppState| s.kv_select(key.clone())),
                );
            }
        }
    }
    if let Some(error) = &state.error {
        out = out.child(
            text(error.clone())
                .key("native-kv-server-folder-error")
                .wrap(),
        );
    }
    if state.request.is_some() {
        out = out.child(text("Loading folder…"));
    } else if (!state.started || state.cursor != 0) && state.entries.len() < MAX_KEYS {
        out = out.child(
            control("native-kv-server-folder-more", "Load more children", theme)
                .on_press(AppState::kv_next_folder_page),
        );
    }
    out
}
