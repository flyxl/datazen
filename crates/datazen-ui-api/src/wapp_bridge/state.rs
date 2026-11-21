use std::collections::BTreeMap;

use serde_json::Value;

use super::protocol::{
    MAX_INFLIGHT_REQUESTS, MAX_REQUEST_ID_CHARS, NOTIFY_MIN_INTERVAL_MS, REQUEST_TIMEOUT_MS,
};
use crate::types::wapp::{WappPermission, WAPP_API_VERSION};

#[derive(Clone, Debug, PartialEq)]
pub struct AttachBridgeOptions {
    pub wapp_id: String,
    pub permissions: Vec<WappPermission>,
    pub locale: String,
    pub api_version: u32,
    pub timeout_ms: u64,
    pub max_inflight: u32,
    pub notify_cooldown_ms: i64,
}

impl Default for AttachBridgeOptions {
    fn default() -> Self {
        Self {
            wapp_id: String::new(),
            permissions: Vec::new(),
            locale: "en".to_owned(),
            api_version: WAPP_API_VERSION,
            timeout_ms: REQUEST_TIMEOUT_MS,
            max_inflight: MAX_INFLIGHT_REQUESTS,
            notify_cooldown_ms: NOTIFY_MIN_INTERVAL_MS,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionRequestRecord {
    pub digest: String,
    pub result: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BridgeState {
    pub detached: bool,
    pub generation: u64,
    pub instance_nonce: String,
    pub instance_id: Option<String>,
    pub session_requests: BTreeMap<String, SessionRequestRecord>,
    pub inflight: u32,
    pub last_notify_ms: Option<i64>,
    pub timers: BTreeMap<u64, u64>,
}

impl BridgeState {
    pub fn new(initial_nonce: String) -> Self {
        Self {
            detached: false,
            generation: 0,
            instance_nonce: initial_nonce,
            instance_id: None,
            session_requests: BTreeMap::new(),
            inflight: 0,
            last_notify_ms: None,
            timers: BTreeMap::new(),
        }
    }

    pub fn request_id_valid(req_id: &Option<String>) -> bool {
        match req_id {
            Some(id) => !id.is_empty() && id.chars().count() <= MAX_REQUEST_ID_CHARS,
            None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BridgeEffect {
    AddFrameLoadListener,
    RemoveFrameLoadListener,
    AddMessageListener,
    RemoveMessageListener,
    PostMessage(Value),
    SetTimer {
        token: u64,
        generation: u64,
        duration_ms: u64,
    },
    ClearTimer {
        token: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeoutDisposition {
    Respond,
    Ignore,
}

pub fn timeout_disposition(
    settled: bool,
    detached: bool,
    generation: u64,
    request_generation: u64,
) -> TimeoutDisposition {
    if settled || detached || generation != request_generation {
        TimeoutDisposition::Ignore
    } else {
        TimeoutDisposition::Respond
    }
}
