//! Port of `packages/frontend/src/lib/tunnelDeletion.ts` — absorbed
//! by T441 (the delete dialog imports it directly; the tunnel
//! settings section T453 reuses it).
//!
//! Reference-integrity cleanup for `delete_tunnel`.
//!
//! The backend `delete_tunnel` only removes the entity (`retain`):
//! it neither checks nor cleans up referencing connections, so
//! deleting a referenced tunnel silently leaves dangling
//! `tunnelId`s that hard-fail at connect time with an internal
//! error (G3). The UI therefore unbinds every referencing
//! connection *before* deleting, and aborts the delete when an
//! unbind fails.
//!
//! Ordering and failure semantics:
//! 1. read the current connections and select the ones referencing
//!    `tunnelId`;
//! 2. save each one without the reference — on the first failure,
//!    restore the already-unbound ones and abort without deleting
//!    anything;
//! 3. only once every reference is gone, delete the tunnel.
//!
//! A failure in step 3 leaves no dangling reference (the entity
//! simply remains, unreferenced), so it is reported as a distinct
//! outcome rather than a rollback.
//!
//! Known deltas: the async deps become a synchronous trait (the
//! host's transport); `messageOf`'s `Error` branch is the message
//! string the transport reports.

// No production consumer yet (T441's dialog and T453's section wire
// it); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use datazen_ui_api::ConnectionConfig;

use crate::i18n::t;

/// `DeleteTunnelOutcome`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeleteTunnelOutcome {
    Deleted { unbound_connection_ids: Vec<String> },
    UnbindFailed {
        message: String,
        rolled_back: Vec<String>,
        rollback_failed: Vec<String>,
    },
    DeleteFailed {
        message: String,
        unbound_connection_ids: Vec<String>,
    },
}

/// `DeleteTunnelDeps` — the transport the cleanup drives.
pub trait TunnelDeletionDeps {
    /// `getConnections()`.
    fn get_connections(&self) -> Result<Vec<ConnectionConfig>, String>;
    /// `saveConnection(config)`.
    fn save_connection(&self, config: &ConnectionConfig) -> Result<(), String>;
    /// `deleteTunnel(id)`.
    fn delete_tunnel(&self, id: &str) -> Result<(), String>;
}

/// `messageOf`: the transport's own words, or the fallback key's
/// translated text.
fn message_of(error: &str, fallback_key: &str) -> String {
    if !error.is_empty() {
        return error.to_owned();
    }
    t(fallback_key).to_owned()
}

/// The patch applied to a referencing connection: the reference
/// itself, plus the now-meaningless `tunnelKind` hint the form
/// persists alongside it (leaving it behind makes the connection
/// form hydrate as an empty inline tunnel of that kind instead of a
/// plain direct connection).
pub fn unbind_connection_patch(connection: &ConnectionConfig) -> ConnectionConfig {
    let mut next = connection.clone();
    // `tunnelId: undefined, tunnelKind: undefined` — absent on the
    // wire (skip_serializing_if), exactly the React undefined.
    next.tunnel_id = None;
    next.tunnel_kind = None;
    next
}

/// `deleteTunnelAndUnbind`.
pub fn delete_tunnel_and_unbind(
    tunnel_id: &str,
    deps: &dyn TunnelDeletionDeps,
) -> DeleteTunnelOutcome {
    let connections = match deps.get_connections() {
        Ok(connections) => connections,
        Err(e) => {
            return DeleteTunnelOutcome::UnbindFailed {
                message: message_of(&e, "settings.tunnels.delete.failed"),
                rolled_back: Vec::new(),
                rollback_failed: Vec::new(),
            }
        }
    };

    let referencing: Vec<&ConnectionConfig> =
        connections.iter().filter(|c| c.tunnel_id.as_deref() == Some(tunnel_id)).collect();
    let mut unbound: Vec<ConnectionConfig> = Vec::new();

    for connection in referencing {
        if let Err(e) = deps.save_connection(&unbind_connection_patch(connection)) {
            let mut rolled_back: Vec<String> = Vec::new();
            let mut rollback_failed: Vec<String> = Vec::new();
            // Reverse order: undo the most recent write first.
            for done in unbound.iter().rev() {
                match deps.save_connection(done) {
                    Ok(()) => rolled_back.push(done.id.clone()),
                    Err(_) => rollback_failed.push(done.id.clone()),
                }
            }
            return DeleteTunnelOutcome::UnbindFailed {
                message: message_of(&e, "settings.tunnels.delete.unbindFailed"),
                rolled_back,
                rollback_failed,
            };
        }
        unbound.push(connection.clone());
    }

    if let Err(e) = deps.delete_tunnel(tunnel_id) {
        return DeleteTunnelOutcome::DeleteFailed {
            message: message_of(&e, "settings.tunnels.delete.failed"),
            unbound_connection_ids: unbound.iter().map(|c| c.id.clone()).collect(),
        };
    }

    DeleteTunnelOutcome::Deleted {
        unbound_connection_ids: unbound.iter().map(|c| c.id.clone()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ui_api::{SslMode, TunnelKind};
    use std::cell::RefCell;

    fn connection(id: &str, tunnel_id: Option<&str>) -> ConnectionConfig {
        ConnectionConfig {
            id: id.to_owned(),
            name: format!("Connection {id}"),
            database_type: "postgres".to_owned(),
            host: Some("127.0.0.1".to_owned()),
            port: Some(5432),
            database: Some("db".to_owned()),
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Prefer,
            connection_timeout: None,
            max_pool_size: None,
            ssh_tunnel: None,
            tunnel_kind: tunnel_id.map(|_| TunnelKind::Ssh),
            tunnel_id: tunnel_id.map(str::to_owned),
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: None,
            pinned: None,
        }
    }

    /// A scripted transport: the saves may fail for selected ids
    /// (optionally only their n-th occurrence), the delete may fail.
    struct Scripted {
        connections: Vec<ConnectionConfig>,
        /// `(id, zero-based occurrence)` — that particular save
        /// fails (an id in `fail_saves_for` fails every save).
        fail_saves_for: Vec<String>,
        fail_nth_save: Vec<(String, usize)>,
        fail_delete: bool,
        saves: RefCell<Vec<ConnectionConfig>>,
        deletes: RefCell<Vec<String>>,
    }

    impl Scripted {
        fn new(connections: Vec<ConnectionConfig>) -> Self {
            Self {
                connections,
                fail_saves_for: Vec::new(),
                fail_nth_save: Vec::new(),
                fail_delete: false,
                saves: RefCell::new(Vec::new()),
                deletes: RefCell::new(Vec::new()),
            }
        }
    }

    impl TunnelDeletionDeps for Scripted {
        fn get_connections(&self) -> Result<Vec<ConnectionConfig>, String> {
            Ok(self.connections.clone())
        }
        fn save_connection(&self, config: &ConnectionConfig) -> Result<(), String> {
            let occurrence =
                self.saves.borrow().iter().filter(|c| c.id == config.id).count();
            if self.fail_saves_for.iter().any(|id| *id == config.id)
                || self
                    .fail_nth_save
                    .iter()
                    .any(|(id, n)| *id == config.id && *n == occurrence)
            {
                return Err(format!("cannot save {}", config.id));
            }
            self.saves.borrow_mut().push(config.clone());
            Ok(())
        }
        fn delete_tunnel(&self, id: &str) -> Result<(), String> {
            self.deletes.borrow_mut().push(id.to_owned());
            if self.fail_delete {
                return Err("cannot delete".to_owned());
            }
            Ok(())
        }
    }

    #[test]
    fn the_patch_strips_the_reference_and_the_kind_hint() {
        let connection = connection("c1", Some("t-1"));
        let patched = unbind_connection_patch(&connection);
        assert_eq!(patched.tunnel_id, None, "the reference is gone");
        assert_eq!(patched.tunnel_kind, None, "the hint is gone too");
        // The wire spells both absent (undefined), not null.
        let wire = serde_json::to_value(&patched).unwrap();
        assert!(wire.get("tunnelId").is_none());
        assert!(wire.get("tunnelKind").is_none());
        // Everything else survives.
        assert_eq!(patched.id, "c1");
        assert_eq!(patched.name, "Connection c1");
    }

    #[test]
    fn an_unreferenced_tunnel_deletes_cleanly() {
        let deps = Scripted::new(vec![connection("c1", None)]);
        let outcome = delete_tunnel_and_unbind("t-1", &deps);
        assert_eq!(
            outcome,
            DeleteTunnelOutcome::Deleted { unbound_connection_ids: Vec::new() }
        );
        assert_eq!(*deps.deletes.borrow(), vec!["t-1".to_owned()], "the delete ran");
        assert!(deps.saves.borrow().is_empty(), "no unbind happened");
    }

    #[test]
    fn a_referenced_tunnel_unbinds_first_then_deletes() {
        let deps = Scripted::new(vec![
            connection("c1", Some("t-1")),
            connection("c2", Some("t-other")),
            connection("c3", Some("t-1")),
        ]);
        let outcome = delete_tunnel_and_unbind("t-1", &deps);
        assert_eq!(
            outcome,
            DeleteTunnelOutcome::Deleted {
                unbound_connection_ids: vec!["c1".to_owned(), "c3".to_owned()]
            }
        );
        // The unbind saves happen before the delete (the ordering
        // the G3 rule exists for).
        let saves = deps.saves.borrow();
        assert_eq!(saves.len(), 2, "only the referencing connections");
        assert!(saves.iter().all(|c| c.tunnel_id.is_none()));
        assert_eq!(*deps.deletes.borrow(), vec!["t-1".to_owned()]);
    }

    #[test]
    fn an_unbind_failure_rolls_back_in_reverse_and_never_deletes() {
        let mut deps = Scripted::new(vec![
            connection("c1", Some("t-1")),
            connection("c2", Some("t-1")),
            connection("c3", Some("t-1")),
        ]);
        // c3's unbind save fails; c1 and c2 already unbound.
        deps.fail_saves_for = vec!["c3".to_owned()];
        let outcome = delete_tunnel_and_unbind("t-1", &deps);
        match outcome {
            DeleteTunnelOutcome::UnbindFailed { message, rolled_back, rollback_failed } => {
                assert_eq!(message, "cannot save c3");
                assert_eq!(rolled_back, vec!["c2".to_owned(), "c1".to_owned()], "reverse order");
                assert!(rollback_failed.is_empty());
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert!(deps.deletes.borrow().is_empty(), "nothing was deleted");
        // The saves: c1, c2 unbound, then c2, c1 restored.
        let saves = deps.saves.borrow();
        let order: Vec<&str> = saves.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(order, vec!["c1", "c2", "c2", "c1"]);
    }

    #[test]
    fn a_rollback_failure_is_reported_separately() {
        let mut deps = Scripted::new(vec![
            connection("c1", Some("t-1")),
            connection("c2", Some("t-1")),
        ]);
        // c2's unbind save fails, and c1's restore (its second
        // save) fails too.
        deps.fail_nth_save = vec![("c2".to_owned(), 0), ("c1".to_owned(), 1)];
        let outcome = delete_tunnel_and_unbind("t-1", &deps);
        match outcome {
            DeleteTunnelOutcome::UnbindFailed { rolled_back, rollback_failed, .. } => {
                assert!(rolled_back.is_empty());
                assert_eq!(rollback_failed, vec!["c1".to_owned()], "the stuck reference");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }

    #[test]
    fn a_delete_failure_leaves_no_dangling_reference() {
        let mut deps = Scripted::new(vec![connection("c1", Some("t-1"))]);
        deps.fail_delete = true;
        let outcome = delete_tunnel_and_unbind("t-1", &deps);
        assert_eq!(
            outcome,
            DeleteTunnelOutcome::DeleteFailed {
                message: "cannot delete".to_owned(),
                unbound_connection_ids: vec!["c1".to_owned()],
            }
        );
    }

    #[test]
    fn a_read_failure_uses_the_translated_fallback() {
        struct Failing;
        impl TunnelDeletionDeps for Failing {
            fn get_connections(&self) -> Result<Vec<ConnectionConfig>, String> {
                Err(String::new())
            }
            fn save_connection(&self, _config: &ConnectionConfig) -> Result<(), String> {
                Ok(())
            }
            fn delete_tunnel(&self, _id: &str) -> Result<(), String> {
                Ok(())
            }
        }
        let outcome = delete_tunnel_and_unbind("t-1", &Failing);
        match outcome {
            DeleteTunnelOutcome::UnbindFailed { message, .. } => {
                assert_eq!(message, t("settings.tunnels.delete.failed"), "the fallback key");
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
    }
}
