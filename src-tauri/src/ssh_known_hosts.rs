//! SSH known-hosts store (TOFU) persisted under the app data directory.

use russh::keys::ssh_key::{HashAlg, PublicKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnownHostEntry {
    pub fingerprint: String,
    pub algorithm: String,
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyDecision {
    AcceptFirstUse { fingerprint: String },
    AcceptMatch,
    RejectMismatch { expected: String, received: String },
}

pub fn host_key_id(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

pub fn format_public_key_fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub fn known_host_entry_from_key(key: &PublicKey) -> Result<KnownHostEntry, String> {
    Ok(KnownHostEntry {
        fingerprint: format_public_key_fingerprint(key),
        algorithm: key.key_data().algorithm().as_str().to_string(),
        public_key: key
            .to_openssh()
            .map_err(|e| format!("encode SSH public key: {e}"))?,
    })
}

pub fn verify_host_key(
    stored: Option<&KnownHostEntry>,
    observed: &KnownHostEntry,
) -> HostKeyDecision {
    match stored {
        None => HostKeyDecision::AcceptFirstUse {
            fingerprint: observed.fingerprint.clone(),
        },
        Some(stored) if stored.fingerprint == observed.fingerprint => HostKeyDecision::AcceptMatch,
        Some(stored) => HostKeyDecision::RejectMismatch {
            expected: stored.fingerprint.clone(),
            received: observed.fingerprint.clone(),
        },
    }
}

pub fn load_known_hosts(path: &Path) -> HashMap<String, KnownHostEntry> {
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(map) => map,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Ignoring corrupt ssh_known_hosts.json");
                HashMap::new()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "Failed to read ssh_known_hosts.json");
            HashMap::new()
        }
    }
}

pub fn save_known_hosts(
    path: &Path,
    map: &HashMap<String, KnownHostEntry>,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create known hosts dir {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(map)
        .map_err(|e| format!("serialize ssh_known_hosts.json: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("write {}: {e}", path.display()))
}

pub fn mismatch_error_message(host_id: &str, expected: &str, received: &str) -> String {
    format!(
        "SSH host key changed for {host_id} (possible MITM). Expected {expected}, got {received}"
    )
}
