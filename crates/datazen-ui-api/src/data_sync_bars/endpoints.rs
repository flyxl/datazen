use super::TranslateFn;

pub const ENDPOINTS_TEST_ID_PREFIX: &str = "data-sync";
pub const ENDPOINTS_I18N_PREFIX: &str = "sync";
pub const COMPARE_LABEL_KEY: &str = "sync.compare";
pub const PATH_TEST_ID: &str = "data-sync-path";
pub const ACTION_NOTE_CLASS: &str = "text-[10px] font-medium uppercase tracking-wide";
pub const SUPPORTED_CLASS: &str = "text-fg-muted";
pub const UNSUPPORTED_CLASS: &str = "text-amber-600 dark:text-amber-400";
pub const EXPERIMENTAL_HINT_KEY: &str = "common.experimentalPairHint";
pub const EXPERIMENTAL_PAIR_KEY: &str = "common.experimentalPair";
pub const PATH_DIRECT_KEY: &str = "sync.pathDirect";
pub const PATH_IR_KEY: &str = "sync.pathIr";
pub const USE_TRANSFER_HINT_KEY: &str = "sync.useTransferHint";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncPairingPath {
    Direct,
    Ir,
    Unsupported,
}

impl SyncPairingPath {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncPairingPath::Direct => "direct",
            SyncPairingPath::Ir => "ir",
            SyncPairingPath::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivePairing {
    pub supported: bool,
    pub path: SyncPairingPath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionNote {
    pub test_id: String,
    pub title: Option<String>,
    pub class: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointsLayout {
    Bar,
    Grid,
}

impl EndpointsLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            EndpointsLayout::Bar => "bar",
            EndpointsLayout::Grid => "grid",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointsBarConfig {
    pub test_id_prefix: String,
    pub i18n_prefix: String,
    pub layout: EndpointsLayout,
    pub compare_label: String,
    pub action_note: Option<ActionNote>,
}

pub fn resolve_action_note(
    active: Option<&ActivePairing>,
    experimental: bool,
    t: &TranslateFn<'_>,
) -> Option<ActionNote> {
    let active = active?;
    let title = if experimental {
        Some(t(EXPERIMENTAL_HINT_KEY, &[]))
    } else {
        None
    };
    let class = if active.supported {
        SUPPORTED_CLASS
    } else {
        UNSUPPORTED_CLASS
    };
    let text = if active.supported && experimental {
        t(EXPERIMENTAL_PAIR_KEY, &[])
    } else if active.supported && active.path == SyncPairingPath::Direct {
        t(PATH_DIRECT_KEY, &[])
    } else if active.supported {
        t(PATH_IR_KEY, &[])
    } else {
        t(USE_TRANSFER_HINT_KEY, &[])
    };
    Some(ActionNote {
        test_id: PATH_TEST_ID.to_owned(),
        title,
        class: class.to_owned(),
        text,
    })
}

pub fn build_endpoints_bar_config(
    layout: Option<EndpointsLayout>,
    active: Option<&ActivePairing>,
    experimental: bool,
    t: &TranslateFn<'_>,
) -> EndpointsBarConfig {
    EndpointsBarConfig {
        test_id_prefix: ENDPOINTS_TEST_ID_PREFIX.to_owned(),
        i18n_prefix: ENDPOINTS_I18N_PREFIX.to_owned(),
        layout: layout.unwrap_or(EndpointsLayout::Bar),
        compare_label: t(COMPARE_LABEL_KEY, &[]),
        action_note: resolve_action_note(active, experimental, t),
    }
}
