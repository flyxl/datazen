//! Queue and deliver CLI `datazen open` requests.
//!
//! A request is acknowledged only after the desktop has successfully emitted
//! the frontend event. When the first run is still in onboarding there is no
//! main window yet, so the request is queued and replayed by
//! `onboarding_complete` instead of being dropped.

use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use super::open_request::OpenConnectionRequest;

/// At most one CLI open request belongs to a process launch. Keeping the
/// latest is correct because a second argv request in the same process cannot
/// exist; replay consumes it exactly once.
#[derive(Default)]
pub(crate) struct PendingOpenRequest(Mutex<Option<OpenConnectionRequest>>);

impl PendingOpenRequest {
    pub(crate) fn queue(&self, request: OpenConnectionRequest) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(request);
    }

    pub(crate) fn take(&self) -> Option<OpenConnectionRequest> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}

pub(crate) fn queue_pending_open_request(app: &AppHandle, request: OpenConnectionRequest) {
    if let Some(pending) = app.try_state::<PendingOpenRequest>() {
        pending.queue(request);
    } else {
        tracing::warn!("pending open request state is not managed; dropping request");
    }
}

pub(crate) fn replay_pending_open_request(app: &AppHandle, target: WebviewWindow) {
    let Some(pending) = app.try_state::<PendingOpenRequest>() else {
        return;
    };
    let Some(request) = pending.take() else {
        return;
    };
    tracing::info!("replaying queued CLI open request after onboarding");
    dispatch(request, target);
}

pub(crate) fn dispatch(request: OpenConnectionRequest, target: WebviewWindow) {
    let request_id = format!(
        "cli:{}:{}:{}",
        request.connection_id,
        request.query.as_deref().map_or(0, str::len),
        nonce(),
    );
    let payload = serde_json::json!({
        "connectionId": request.connection_id,
        "sql": request.query,
        "requestId": request_id,
        "source": "cli",
    });
    std::thread::spawn(move || {
        // The frontend listener mounts with the main window; retry with the
        // same request id so the first emit cannot be lost.
        let mut acknowledged = false;
        for delay in [600_u64, 1_400, 2_800] {
            std::thread::sleep(Duration::from_millis(delay));
            match target.emit("datazen:open-connection", payload.clone()) {
                Ok(()) => {
                    if !acknowledged {
                        acknowledge(&request, &request_id);
                        acknowledged = true;
                    }
                }
                Err(error) => {
                    tracing::warn!(error = %error, "failed to emit CLI open request");
                }
            }
        }
    });
}

fn nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn acknowledge(request: &OpenConnectionRequest, request_id: &str) {
    let Some(path) = request.ack_path.as_deref() else {
        return;
    };
    if let Err(error) = write_ack(Path::new(path), request_id) {
        tracing::warn!(
            kind = ?error.kind(),
            "failed to write the CLI open acknowledgement"
        );
    }
}

fn write_ack(path: &Path, request_id: &str) -> io::Result<()> {
    let payload = serde_json::json!({ "ok": true, "requestId": request_id });
    std::fs::write(path, format!("{payload}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(ack_path: Option<&str>) -> OpenConnectionRequest {
        OpenConnectionRequest {
            connection_id: "conn-1".into(),
            query: Some("SELECT 1".into()),
            data_dir: None,
            ack_path: ack_path.map(str::to_string),
        }
    }

    #[test]
    fn pending_request_is_replayed_exactly_once() {
        let pending = PendingOpenRequest::default();
        pending.queue(request(None));
        assert_eq!(pending.take(), Some(request(None)));
        assert_eq!(pending.take(), None);
    }

    #[test]
    fn ack_file_records_the_request_id() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ack");
        write_ack(&path, "cli:conn-1:8:42").unwrap();
        let body = std::fs::read_to_string(path).unwrap();
        assert!(body.contains("\"ok\":true"), "{body}");
        assert!(body.contains("cli:conn-1:8:42"), "{body}");
    }
}
