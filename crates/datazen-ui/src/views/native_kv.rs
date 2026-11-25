//! Native KV workbench contribution. The frame renders cached state only.

use std::collections::BTreeSet;

use datazen_ui_api::{
    native_kv::{
        KvChange, KvCollectionEdit, KvDetail, KvFolderChild, KvFolderPage, KvKey, KvRead,
        KvReadKind, KvReply, KvTool, NativeKvDriver, RedisKeyType, KV_EDIT_MAX_BYTES,
        KV_TTL_MAX_SECONDS,
    },
    AdapterJob,
};
use rune::{
    row, text,
    tok::{self, type_},
    El, TextEdit,
};

use crate::state_def::AppState;
use crate::ui::field::TextField;

#[path = "native_kv_folders.rs"]
mod folders;
#[path = "native_kv_render.rs"]
mod render;
#[path = "native_kv_tools.rs"]
mod tools;
#[path = "native_kv_tree.rs"]
mod tree;

const MAX_KEYS: usize = 1024;

#[derive(Clone)]
enum EditMode {
    String,
    Hash(String),
    Ttl(String),
    Collection(KvCollectionEdit, String),
}

struct Draft {
    mode: EditMode,
    original: String,
    value: TextEdit,
}

impl Draft {
    fn change(&self) -> Result<KvChange, &'static str> {
        let value = self.value.text.clone();
        if matches!(self.mode, EditMode::Ttl(_)) {
            let seconds = value
                .trim()
                .parse::<i64>()
                .map_err(|_| "Enter a positive number of seconds, or -1 for no expiry.")?;
            if seconds != -1 && !(1..=KV_TTL_MAX_SECONDS).contains(&seconds) {
                return Err("TTL must be 1 to 31536000 seconds, or -1 for no expiry.");
            }
            if let EditMode::Ttl(key_type) = &self.mode {
                return Ok(KvChange::Ttl {
                    expected_type: key_type.clone(),
                    seconds,
                });
            }
        }
        if value.len() > KV_EDIT_MAX_BYTES || value.contains(['\n', '\r', '\0']) {
            return Err("Use a single line of text (at most 4096 bytes).");
        }
        Ok(match &self.mode {
            EditMode::String => KvChange::String {
                expected: self.original.clone(),
                value,
            },
            EditMode::Hash(field) => KvChange::Hash {
                field: field.clone(),
                expected: self.original.clone(),
                value,
            },
            EditMode::Collection(target, _) => KvChange::Collection {
                target: target.clone(),
                expected: self.original.clone(),
                value,
            },
            EditMode::Ttl(_) => return Err("Invalid TTL."),
        })
    }
}

pub(crate) struct KvWorkbench {
    db_index: Option<u32>,
    filter: TextEdit,
    pattern: String,
    key_type: RedisKeyType,
    persistent: bool,
    cluster: bool,
    tree_mode: bool,
    server_folders: Option<folders::State>,
    collapsed_folders: BTreeSet<String>,
    databases: Vec<u32>,
    db_picker_open: bool,
    tools: tools::ToolPanel,
    writable: bool,
    draft: Option<Draft>,
    edit_request: Option<u64>,
    edit_notice: Option<String>,
    driver: NativeKvDriver,
    connection_id: String,
    session_generation: u32,
    sequence: u64,
    cursor: u64,
    started: bool,
    keys: Vec<KvKey>,
    seen: BTreeSet<String>,
    scan_request: Option<u64>,
    detail_request: Option<u64>,
    selected: Option<String>,
    detail: Option<KvDetail>,
    scan_error: Option<String>,
    detail_error: Option<String>,
}

impl KvWorkbench {
    pub(crate) fn new(
        driver: NativeKvDriver,
        connection_id: String,
        session_generation: u32,
    ) -> Self {
        Self {
            db_index: None,
            filter: TextEdit::new(""),
            pattern: "*".into(),
            key_type: RedisKeyType::Any,
            persistent: false,
            cluster: false,
            tree_mode: false,
            server_folders: None,
            collapsed_folders: BTreeSet::new(),
            databases: Vec::new(),
            db_picker_open: false,
            tools: tools::ToolPanel::default(),
            writable: false,
            draft: None,
            edit_request: None,
            edit_notice: None,
            driver,
            connection_id,
            session_generation,
            sequence: 0,
            cursor: 0,
            started: false,
            keys: Vec::new(),
            seen: BTreeSet::new(),
            scan_request: None,
            detail_request: None,
            selected: None,
            detail: None,
            scan_error: None,
            detail_error: None,
        }
    }

    fn request(&mut self, kind: KvReadKind) -> KvRead {
        self.sequence += 1;
        KvRead {
            connection_id: self.connection_id.clone(),
            session_generation: self.session_generation,
            request_id: self.sequence,
            db_index: self.db_index,
            kind,
        }
    }

    fn next_page(&mut self) -> Option<KvRead> {
        if self.scan_request.is_some()
            || (self.started && self.cursor == 0)
            || self.keys.len() == MAX_KEYS
        {
            return None;
        }
        self.scan_error = None;
        let request = self.request(KvReadKind::Scan {
            cursor: self.cursor,
            pattern: self.pattern.clone(),
            key_type: self.key_type,
            persistent: self.persistent,
        });
        self.scan_request = Some(request.request_id);
        Some(request)
    }

    fn select(&mut self, key: String) -> KvRead {
        self.selected = Some(key.clone());
        self.detail = None;
        self.detail_error = None;
        let request = self.request(KvReadKind::Detail { key });
        self.detail_request = Some(request.request_id);
        request
    }

    fn reset_scan(&mut self) {
        self.started = false;
        self.cursor = 0;
        self.keys.clear();
        self.seen.clear();
        self.collapsed_folders.clear();
        self.server_folders = None;
        self.tools.clear_marked();
        self.selected = None;
        self.scan_request = None;
        self.detail_request = None;
        self.detail = None;
        self.detail_error = None;
    }

    fn can_leave(&mut self) -> bool {
        if self.edit_request.is_some() {
            self.edit_notice = Some("Wait for Redis to confirm the save.".into());
            return false;
        }
        if self
            .draft
            .as_ref()
            .is_some_and(|draft| draft.value.text != draft.original)
        {
            self.edit_notice = Some("Unsaved changes. Save or discard them before leaving.".into());
            return false;
        }
        self.draft = None;
        self.edit_notice = None;
        true
    }

    fn begin_edit(&mut self, mode: EditMode, original: String) {
        if self.writable && self.edit_request.is_none() && self.can_leave() {
            self.draft = Some(Draft {
                mode,
                value: TextEdit::new(original.clone()),
                original,
            });
        }
    }

    fn submit(&mut self) -> Option<KvRead> {
        if !self.writable || self.edit_request.is_some() {
            return None;
        }
        let draft = self.draft.as_ref()?;
        if draft.value.text == draft.original {
            return None;
        }
        let key = self.selected.clone()?;
        let change = match draft.change() {
            Ok(change) => change,
            Err(error) => {
                self.edit_notice = Some(error.into());
                return None;
            }
        };
        let request = self.request(KvReadKind::Edit { key, change });
        self.edit_request = Some(request.request_id);
        self.edit_notice = None;
        Some(request)
    }

    fn receive(&mut self, request: KvRead, result: Result<KvReply, String>) -> Option<KvRead> {
        if request.connection_id != self.connection_id
            || request.session_generation != self.session_generation
            || (request.db_index.is_some() && request.db_index != self.db_index)
        {
            return None;
        }
        match request.kind {
            KvReadKind::Scan { .. } if self.scan_request == Some(request.request_id) => {
                self.scan_request = None;
                match result {
                    Ok(KvReply::Page(page)) => {
                        self.started = true;
                        self.writable = page.writable;
                        self.db_index = Some(page.db_index);
                        self.databases = page.databases;
                        self.cluster = page.cluster;
                        self.cursor = page.cursor;
                        for key in page.keys {
                            if self.keys.len() == MAX_KEYS {
                                break;
                            }
                            if self.seen.insert(key.key.clone()) {
                                self.keys.push(key);
                            }
                        }
                    }
                    Err(message) => self.scan_error = Some(message),
                    _ => self.scan_error = Some("Unexpected key list response.".into()),
                }
            }
            KvReadKind::Folder { prefix, .. } => {
                if let Some(folder) = self.server_folders.as_mut() {
                    folder.receive(request.request_id, &prefix, result);
                }
            }
            KvReadKind::Detail { key }
                if self.detail_request == Some(request.request_id)
                    && self.selected.as_ref() == Some(&key) =>
            {
                self.detail_request = None;
                match result {
                    Ok(KvReply::Detail(detail)) => self.detail = Some(detail),
                    Err(message) => self.detail_error = Some(message),
                    _ => self.detail_error = Some("Unexpected key detail response.".into()),
                }
            }
            KvReadKind::Edit { key, .. }
                if self.edit_request == Some(request.request_id)
                    && self.selected.as_ref() == Some(&key) =>
            {
                self.edit_request = None;
                match result {
                    Ok(KvReply::Edited) => {
                        self.draft = None;
                        let refresh = self.select(key);
                        self.edit_notice = Some("Saved to Redis.".into());
                        return Some(refresh);
                    }
                    Err(error) => self.edit_notice = Some(error),
                    _ => self.edit_notice = Some("Redis did not confirm the save.".into()),
                }
            }
            KvReadKind::Tool(op) if self.tools.request == Some(request.request_id) => {
                self.tools.request = None;
                let refresh = result.is_ok()
                    && matches!(
                        op,
                        KvTool::CreateString { .. }
                            | KvTool::RenameKey { .. }
                            | KvTool::DeleteKey { .. }
                            | KvTool::DeleteMarked { .. }
                            | KvTool::Import { .. }
                    );
                self.tools.output = match result {
                    Ok(KvReply::Tool(output)) => output,
                    Err(error) => error,
                    _ => "Unexpected Redis tool response.".into(),
                };
                if refresh {
                    self.reset_scan();
                    return self.next_page();
                }
            }
            _ => {} // Replies to an older key, scan, or connection never update the workbench.
        }
        None
    }
}

impl AppState {
    pub(crate) fn kv_can_leave(&mut self) -> bool {
        self.native_kv.as_mut().is_none_or(KvWorkbench::can_leave)
    }

    pub(crate) fn close_kv_workbench(&mut self) {
        if let Some(workbench) = self.native_kv.take() {
            self.desktop.begin_job(AdapterJob::CloseKv {
                connection_id: workbench.connection_id,
                session_generation: workbench.session_generation,
            });
        }
    }

    pub(crate) fn kv_next_page(&mut self) {
        if let Some(request) = self.native_kv.as_mut().and_then(KvWorkbench::next_page) {
            self.dispatch_kv(request);
        }
    }

    fn kv_switch_db(&mut self, offset: isize) {
        let Some(vm) = self.native_kv.as_ref() else {
            return;
        };
        let Some(index) = vm.databases.iter().position(|n| Some(*n) == vm.db_index) else {
            return;
        };
        if let Some(new) = index
            .checked_add_signed(offset)
            .and_then(|idx| vm.databases.get(idx))
            .copied()
        {
            self.kv_choose_db(new);
        }
    }

    fn kv_choose_db(&mut self, db: u32) {
        if !self.kv_can_leave() || self.connect_loading.is_some() {
            return;
        }
        let Some(vm) = self.native_kv.as_mut() else {
            return;
        };
        if vm.tools.request.is_some() || !vm.databases.contains(&db) {
            return;
        }
        vm.db_picker_open = false;
        if vm.db_index == Some(db) {
            return;
        }
        vm.db_index = Some(db);
        vm.reset_scan();
        vm.tools.output.clear();
        vm.tools.console_set_review = None;
        self.kv_next_page();
    }

    fn kv_apply_pattern(&mut self) {
        if !self.kv_can_leave() || self.connect_loading.is_some() {
            return;
        }
        let Some(vm) = &mut self.native_kv else {
            return;
        };
        let pattern = vm.filter.text.trim();
        vm.pattern = if pattern.is_empty() {
            "*".into()
        } else {
            pattern.into()
        };
        vm.reset_scan();
        self.kv_next_page();
    }

    fn kv_lookup_exact_key(&mut self) {
        if !self.kv_can_leave() || self.connect_loading.is_some() {
            return;
        }
        let Some(vm) = self.native_kv.as_mut() else {
            return;
        };
        let key = vm.filter.text.clone();
        if key.is_empty() || key.len() > 1024 || key.chars().any(char::is_control) {
            vm.scan_error = Some("Enter an exact key name between 1 and 1024 bytes.".into());
            return;
        }
        vm.scan_error = None;
        let request = vm.select(key);
        self.dispatch_kv(request);
    }

    fn kv_adjust_scan(&mut self, change_type: bool) {
        if !self.kv_can_leave() || self.connect_loading.is_some() {
            return;
        }
        if let Some(vm) = self.native_kv.as_mut() {
            if change_type {
                vm.key_type = vm.key_type.next();
            } else {
                vm.persistent = !vm.persistent;
            }
            vm.reset_scan();
            self.kv_next_page();
        }
    }

    fn kv_tool(&mut self, tool: KvTool) {
        let Some(vm) = &mut self.native_kv else {
            return;
        };
        if vm.tools.request.is_some() || self.connect_loading.is_some() {
            return;
        }
        if let KvTool::Console(command) = &tool {
            vm.tools.record_console(command);
        }
        let request = vm.request(KvReadKind::Tool(tool));
        vm.tools.request = Some(request.request_id);
        self.dispatch_kv(request);
    }

    fn kv_toggle_tools(&mut self) {
        let already_open = self.native_kv.as_ref().is_some_and(|vm| vm.tools.open);
        self.kv_show_tools((!already_open).then_some(tools::Section::All));
    }

    fn kv_show_tools(&mut self, section: Option<tools::Section>) -> bool {
        if !self.kv_can_leave() {
            return false;
        }
        let Some(vm) = self.native_kv.as_mut() else {
            return false;
        };
        if vm.tools.request.is_some() {
            return false;
        }
        vm.tools.open = section.is_some();
        if let Some(section) = section {
            if vm.tools.section != section {
                vm.tools.output.clear();
                vm.tools.console_set_review = None;
            }
            vm.tools.section = section;
        }
        true
    }

    fn kv_select(&mut self, key: String) {
        if !self.kv_can_leave() {
            return;
        }
        if let Some(workbench) = &mut self.native_kv {
            let request = workbench.select(key);
            self.dispatch_kv(request);
        }
    }

    fn dispatch_kv(&mut self, request: KvRead) {
        if !self.desktop.begin_job(AdapterJob::ReadKv(request.clone())) {
            self.receive_kv(
                request,
                Err("The native key browser is unavailable.".into()),
            );
        }
    }

    pub(crate) fn receive_kv(&mut self, request: KvRead, result: Result<KvReply, String>) {
        if let Some(workbench) = &mut self.native_kv {
            if let Some(refresh) = workbench.receive(request, result) {
                self.dispatch_kv(refresh);
            }
        }
    }

    fn kv_begin_edit(&mut self, mode: EditMode, original: String) {
        if let Some(vm) = &mut self.native_kv {
            if self.connect_loading.is_some() {
                vm.edit_notice = Some("Wait for the pending connection to open.".into());
                return;
            }
            vm.begin_edit(mode, original);
        }
    }

    fn native_kv_edit_input(&mut self) -> &mut TextEdit {
        self.native_kv
            .as_mut()
            .and_then(|vm| vm.draft.as_mut())
            .map(|draft| &mut draft.value)
            .unwrap_or(&mut self.native_kv_unused_input)
    }

    fn kv_save(&mut self) {
        if let Some(request) = self.native_kv.as_mut().and_then(KvWorkbench::submit) {
            self.dispatch_kv(request);
        }
    }

    fn kv_discard(&mut self) {
        if let Some(vm) = &mut self.native_kv {
            if vm.edit_request.is_some() {
                return;
            }
            vm.draft = None;
            vm.edit_notice = None;
        }
    }
}

pub(crate) fn workbench_view(state: &KvWorkbench, theme: tok::Theme, dark: bool) -> El {
    match state.driver {
        NativeKvDriver::Redis => render::redis_workbench(state, theme, dark),
    }
}

fn control(key: &str, label: &str, theme: tok::Theme) -> El {
    row()
        .key(key)
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .label(label)
        .role("button")
        .focusable(true)
        .child(text(label.to_owned()).font(type_::LABEL).color(theme.text))
}

#[cfg(test)]
#[path = "native_kv_tests.rs"]
mod tests;
