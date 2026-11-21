use super::*;
use crate::{SslMode, TunnelKind};
use std::cell::RefCell;

fn conn(id: &str, tunnel_id: Option<&str>, kind: Option<TunnelKind>) -> ConnectionConfig {
    ConnectionConfig {
        id: id.to_owned(),
        name: id.to_owned(),
        database_type: "postgresql".into(),
        host: None,
        port: None,
        database: None,
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Disable,
        connection_timeout: None,
        max_pool_size: None,
        ssh_tunnel: None,
        tunnel_kind: kind,
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

#[derive(Default)]
struct Recorder {
    saved: RefCell<Vec<ConnectionConfig>>,
    deleted: RefCell<Vec<String>>,
    save_calls: RefCell<usize>,
    save_fail_calls: RefCell<Vec<usize>>,
}

fn translator(key: &str, _params: &[(&str, String)]) -> String {
    format!("T:{key}")
}

#[test]
fn i18n_keys_are_pinned() {
    assert_eq!(KEY_DELETE_FAILED, "settings.tunnels.delete.failed");
    assert_eq!(KEY_UNBIND_FAILED, "settings.tunnels.delete.unbindFailed");
    assert_eq!(translator(KEY_DELETE_FAILED, &[]), "T:settings.tunnels.delete.failed");
}

#[test]
fn the_patch_clears_both_the_reference_and_the_hint() {
    let original = conn("a", Some("t1"), Some(TunnelKind::Ssh));
    let patched = unbind_connection_patch(&original);
    assert_eq!(patched.id, "a");
    assert_eq!(patched.name, "a");
    assert_eq!(patched.tunnel_id, None);
    assert_eq!(patched.tunnel_kind, None);
    assert_eq!(original.tunnel_id.as_deref(), Some("t1"));
    assert_eq!(original.tunnel_kind, Some(TunnelKind::Ssh));
}

#[test]
fn an_unreferenced_tunnel_is_deleted_without_any_unbind() {
    let recorder = Recorder::default();
    let connections = vec![conn("a", Some("other"), None), conn("b", None, None)];
    let get = || Ok::<_, String>(connections.clone());
    let save = |config: &ConnectionConfig| -> Result<(), String> {
        recorder.saved.borrow_mut().push(config.clone());
        Ok(())
    };
    let delete = |id: &str| -> Result<(), String> {
        recorder.deleted.borrow_mut().push(id.to_owned());
        Ok(())
    };
    let deps = DeleteTunnelDeps {
        get_connections: &get,
        save_connection: &save,
        delete_tunnel: &delete,
    };
    let outcome = delete_tunnel_and_unbind("t1", &deps, &translator as &dyn Translate);
    assert_eq!(
        outcome,
        DeleteTunnelOutcome::Deleted {
            unbound_connection_ids: Vec::new()
        }
    );
    assert!(recorder.saved.borrow().is_empty());
    assert_eq!(recorder.deleted.borrow().as_slice(), ["t1"]);
}

#[test]
fn referencing_connections_are_unbound_then_the_tunnel_is_deleted() {
    let recorder = Recorder::default();
    let connections = vec![
        conn("a", Some("t1"), Some(TunnelKind::Ssh)),
        conn("keep", Some("other"), None),
        conn("b", Some("t1"), Some(TunnelKind::Ssh)),
    ];
    let get = || Ok::<_, String>(connections.clone());
    let save = |config: &ConnectionConfig| -> Result<(), String> {
        recorder.saved.borrow_mut().push(config.clone());
        Ok(())
    };
    let delete = |id: &str| -> Result<(), String> {
        recorder.deleted.borrow_mut().push(id.to_owned());
        Ok(())
    };
    let deps = DeleteTunnelDeps {
        get_connections: &get,
        save_connection: &save,
        delete_tunnel: &delete,
    };
    let outcome = delete_tunnel_and_unbind("t1", &deps, &translator as &dyn Translate);
    assert_eq!(
        outcome,
        DeleteTunnelOutcome::Deleted {
            unbound_connection_ids: vec!["a".to_owned(), "b".to_owned()]
        }
    );
    let saved = recorder.saved.borrow();
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0].id, "a");
    assert_eq!(saved[1].id, "b");
    assert!(saved.iter().all(|config| config.tunnel_id.is_none()));
    assert!(saved.iter().all(|config| config.tunnel_kind.is_none()));
    assert_eq!(recorder.deleted.borrow().as_slice(), ["t1"]);
}

#[test]
fn a_failed_unbind_rolls_back_previous_writes_in_reverse_order() {
    let recorder = Recorder::default();
    *recorder.save_fail_calls.borrow_mut() = vec![2];
    let connections = vec![
        conn("a", Some("t1"), None),
        conn("b", Some("t1"), None),
        conn("c", Some("t1"), None),
    ];
    let get = || Ok::<_, String>(connections.clone());
    let save = |config: &ConnectionConfig| -> Result<(), String> {
        *recorder.save_calls.borrow_mut() += 1;
        let call = *recorder.save_calls.borrow();
        if recorder.save_fail_calls.borrow().contains(&call) {
            return Err("save failed".to_owned());
        }
        recorder.saved.borrow_mut().push(config.clone());
        Ok(())
    };
    let delete = |id: &str| -> Result<(), String> {
        recorder.deleted.borrow_mut().push(id.to_owned());
        Ok(())
    };
    let deps = DeleteTunnelDeps {
        get_connections: &get,
        save_connection: &save,
        delete_tunnel: &delete,
    };
    let outcome = delete_tunnel_and_unbind("t1", &deps, &translator as &dyn Translate);
    assert_eq!(
        outcome,
        DeleteTunnelOutcome::UnbindFailed {
            message: "save failed".to_owned(),
            rolled_back: vec!["a".to_owned()],
            rollback_failed: Vec::new(),
        }
    );
    assert!(recorder.deleted.borrow().is_empty());
    let saved = recorder.saved.borrow();
    assert_eq!(saved[0].id, "a");
    assert_eq!(saved[0].tunnel_id, None);
    assert_eq!(saved[1].id, "a");
    assert_eq!(saved[1].tunnel_id.as_deref(), Some("t1"));
}

#[test]
fn rollback_failures_are_reported_separately() {
    let recorder = Recorder::default();
    *recorder.save_fail_calls.borrow_mut() = vec![2, 3];
    let connections = vec![conn("a", Some("t1"), None), conn("b", Some("t1"), None)];
    let get = || Ok::<_, String>(connections.clone());
    let save = |config: &ConnectionConfig| -> Result<(), String> {
        *recorder.save_calls.borrow_mut() += 1;
        let call = *recorder.save_calls.borrow();
        if recorder.save_fail_calls.borrow().contains(&call) {
            return Err("failed".to_owned());
        }
        recorder.saved.borrow_mut().push(config.clone());
        Ok(())
    };
    let delete = |id: &str| -> Result<(), String> {
        recorder.deleted.borrow_mut().push(id.to_owned());
        Ok(())
    };
    let deps = DeleteTunnelDeps {
        get_connections: &get,
        save_connection: &save,
        delete_tunnel: &delete,
    };
    let outcome = delete_tunnel_and_unbind("t1", &deps, &translator as &dyn Translate);
    match outcome {
        DeleteTunnelOutcome::UnbindFailed {
            rolled_back,
            rollback_failed,
            ..
        } => {
            assert!(rolled_back.is_empty());
            assert_eq!(rollback_failed, vec!["a".to_owned()]);
        }
        other => panic!("expected an unbind failure, got {other:?}"),
    }
    assert!(recorder.deleted.borrow().is_empty());
}

#[test]
fn a_read_failure_aborts_without_saving_or_deleting() {
    let recorder = Recorder::default();
    let get = || Err::<Vec<ConnectionConfig>, String>("read failed".to_owned());
    let save = |config: &ConnectionConfig| -> Result<(), String> {
        recorder.saved.borrow_mut().push(config.clone());
        Ok(())
    };
    let delete = |id: &str| -> Result<(), String> {
        recorder.deleted.borrow_mut().push(id.to_owned());
        Ok(())
    };
    let deps = DeleteTunnelDeps {
        get_connections: &get,
        save_connection: &save,
        delete_tunnel: &delete,
    };
    let outcome = delete_tunnel_and_unbind("t1", &deps, &translator as &dyn Translate);
    assert_eq!(
        outcome,
        DeleteTunnelOutcome::UnbindFailed {
            message: "read failed".to_owned(),
            rolled_back: Vec::new(),
            rollback_failed: Vec::new(),
        }
    );
    assert!(recorder.saved.borrow().is_empty());
    assert!(recorder.deleted.borrow().is_empty());
}

#[test]
fn a_failed_delete_keeps_the_unbound_connections_and_reports_the_ids() {
    let recorder = Recorder::default();
    let connections = vec![conn("a", Some("t1"), None)];
    let get = || Ok::<_, String>(connections.clone());
    let save = |config: &ConnectionConfig| -> Result<(), String> {
        recorder.saved.borrow_mut().push(config.clone());
        Ok(())
    };
    let delete = |id: &str| -> Result<(), String> {
        recorder.deleted.borrow_mut().push(id.to_owned());
        Err("delete failed".to_owned())
    };
    let deps = DeleteTunnelDeps {
        get_connections: &get,
        save_connection: &save,
        delete_tunnel: &delete,
    };
    let outcome = delete_tunnel_and_unbind("t1", &deps, &translator as &dyn Translate);
    assert_eq!(
        outcome,
        DeleteTunnelOutcome::DeleteFailed {
            message: "delete failed".to_owned(),
            unbound_connection_ids: vec!["a".to_owned()],
        }
    );
    assert_eq!(recorder.saved.borrow().len(), 1);
}

#[test]
fn an_empty_error_message_falls_back_to_the_translated_key() {
    let connections: Vec<ConnectionConfig> = Vec::new();
    let get = || Ok::<_, String>(connections.clone());
    let save = |_config: &ConnectionConfig| -> Result<(), String> { Ok(()) };
    let delete = |_id: &str| -> Result<(), String> { Err(String::new()) };
    let deps = DeleteTunnelDeps {
        get_connections: &get,
        save_connection: &save,
        delete_tunnel: &delete,
    };
    let outcome = delete_tunnel_and_unbind("t1", &deps, &translator as &dyn Translate);
    assert_eq!(
        outcome,
        DeleteTunnelOutcome::DeleteFailed {
            message: "T:settings.tunnels.delete.failed".to_owned(),
            unbound_connection_ids: Vec::new(),
        }
    );
}

#[test]
fn crate_root_reexports_the_tunnel_deletion_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::DeleteTunnelDeps>();
    resolves::<crate::DeleteTunnelOutcome>();
    resolves::<crate::GetConnections<'static>>();
    resolves::<crate::SaveConnection<'static>>();
    resolves::<crate::DeleteTunnel<'static>>();
    assert_eq!(crate::KEY_DELETE_FAILED, "settings.tunnels.delete.failed");
    assert_eq!(crate::unbind_connection_patch(&conn("a", None, None)).tunnel_id, None);
}
