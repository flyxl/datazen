mod endpoints;
mod execute;

#[cfg(test)]
mod tests;

pub use endpoints::{
    build_endpoints_bar_config, resolve_action_note, ActionNote, ActivePairing, EndpointsBarConfig, EndpointsLayout,
    SyncPairingPath, ACTION_NOTE_CLASS, COMPARE_LABEL_KEY, ENDPOINTS_I18N_PREFIX,
    ENDPOINTS_TEST_ID_PREFIX, EXPERIMENTAL_HINT_KEY, EXPERIMENTAL_PAIR_KEY, PATH_DIRECT_KEY,
    PATH_IR_KEY, PATH_TEST_ID, SUPPORTED_CLASS, UNSUPPORTED_CLASS, USE_TRANSFER_HINT_KEY,
};
pub use execute::{
    build_execute_bar_view, can_fire, CancelVariant, ExecuteBarEvent, ExecuteBarInput,
    ExecuteBarView, RunVariant, CANCEL_TEST_ID, CONTAINER_CLASS, DELETE_CLASS, EXECUTE_TEST_ID,
    KEY_CANCEL, KEY_DELETE_WILL_APPLY, KEY_EXECUTE, KEY_EXECUTING, KEY_EXECUTE_UNAVAILABLE,
    KEY_SELECTED_ROWS, KEY_TARGET_READ_ONLY, READ_ONLY_CLASS, START_DISABLED_TEST_ID, START_TEST_ID,
};

pub type TranslateFn<'a> = dyn Fn(&str, &[(&str, String)]) -> String + 'a;
