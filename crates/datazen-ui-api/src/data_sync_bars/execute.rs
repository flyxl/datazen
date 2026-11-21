use super::TranslateFn;

pub const EXECUTE_TEST_ID: &str = "data-sync-execute";
pub const CANCEL_TEST_ID: &str = "data-sync-cancel";
pub const START_TEST_ID: &str = "data-sync-start";
pub const START_DISABLED_TEST_ID: &str = "data-sync-start-disabled";
pub const KEY_SELECTED_ROWS: &str = "sync.selectedRows";
pub const KEY_TARGET_READ_ONLY: &str = "sync.targetReadOnly";
pub const KEY_DELETE_WILL_APPLY: &str = "sync.deleteWillApply";
pub const KEY_CANCEL: &str = "common.cancel";
pub const KEY_EXECUTE_UNAVAILABLE: &str = "sync.executeUnavailable";
pub const KEY_EXECUTING: &str = "sync.executing";
pub const KEY_EXECUTE: &str = "sync.execute";
pub const CONTAINER_CLASS: &str =
    "flex shrink-0 items-center gap-3 border-t border-edge px-6 py-3";
pub const READ_ONLY_CLASS: &str = "text-xs text-amber-600 dark:text-amber-400";
pub const DELETE_CLASS: &str = "text-xs text-red-600 dark:text-red-400";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelVariant {
    Ghost,
}

impl CancelVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            CancelVariant::Ghost => "ghost",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunVariant {
    Run,
}

impl RunVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            RunVariant::Run => "run",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecuteBarEvent {
    Execute,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecuteBarInput {
    pub selected_rows: u64,
    pub has_deletes: bool,
    pub target_read_only: bool,
    pub executing: bool,
    pub can_execute: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecuteBarView {
    pub enabled: bool,
    pub container_test_id: Option<&'static str>,
    pub container_class: String,
    pub selected_rows_text: String,
    pub show_read_only_warning: bool,
    pub read_only_text: String,
    pub show_delete_warning: bool,
    pub delete_text: String,
    pub show_cancel: bool,
    pub cancel_text: String,
    pub cancel_variant: CancelVariant,
    pub start_test_id: &'static str,
    pub start_disabled: bool,
    pub start_title: Option<String>,
    pub start_text: String,
    pub run_variant: RunVariant,
    pub executing: bool,
}

pub fn build_execute_bar_view(input: ExecuteBarInput, t: &TranslateFn<'_>) -> ExecuteBarView {
    let enabled = input.can_execute && !input.target_read_only && input.selected_rows > 0;
    let read_only_text = t(KEY_TARGET_READ_ONLY, &[]);
    let start_title = if input.target_read_only {
        Some(read_only_text.clone())
    } else if !enabled {
        Some(t(KEY_EXECUTE_UNAVAILABLE, &[]))
    } else {
        None
    };
    ExecuteBarView {
        enabled,
        container_test_id: if enabled {
            Some(EXECUTE_TEST_ID)
        } else {
            None
        },
        container_class: CONTAINER_CLASS.to_owned(),
        selected_rows_text: t(
            KEY_SELECTED_ROWS,
            &[("count", input.selected_rows.to_string())],
        ),
        show_read_only_warning: input.target_read_only,
        read_only_text,
        show_delete_warning: input.has_deletes && !input.target_read_only,
        delete_text: t(KEY_DELETE_WILL_APPLY, &[]),
        show_cancel: input.executing,
        cancel_text: t(KEY_CANCEL, &[]),
        cancel_variant: CancelVariant::Ghost,
        start_test_id: if enabled {
            START_TEST_ID
        } else {
            START_DISABLED_TEST_ID
        },
        start_disabled: !enabled || input.executing,
        start_title,
        start_text: if input.executing {
            t(KEY_EXECUTING, &[])
        } else {
            t(KEY_EXECUTE, &[])
        },
        run_variant: RunVariant::Run,
        executing: input.executing,
    }
}

pub fn can_fire(event: ExecuteBarEvent, view: &ExecuteBarView) -> bool {
    match event {
        ExecuteBarEvent::Execute => !view.start_disabled,
        ExecuteBarEvent::Cancel => view.show_cancel,
    }
}
