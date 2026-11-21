use super::*;

#[test]
fn timeout_only_responds_for_the_current_generation() {
    let request = parse(&envelope("command.invoke", "slow", None));
    let mut host = RecordingHost::default();
    let mut b = bridge(&[]);
    let effects = b.on_timeout(&request, 0);
    let post = last_post(&effects);
    assert_eq!(error_code(&post).as_deref(), Some("E_TIMEOUT"));
    assert_eq!(post["type"], "command.invoke.err");
    assert_eq!(
        post["payload"]["message"],
        "handler timed out after 30000ms"
    );

    b.on_frame_load(&mut host);
    assert!(b.on_timeout(&request, 0).is_empty());
    assert_eq!(
        timeout_disposition(true, false, 0, 0),
        TimeoutDisposition::Ignore
    );
    let mut detached = bridge(&[]);
    detached.state.detached = true;
    assert!(detached.on_timeout(&request, 0).is_empty());
}

#[test]
fn sessions_fail_closed_without_both_permissions_and_nonce() {
    for permissions in [
        vec![],
        vec![WappPermission::SessionStateful],
        vec![WappPermission::CommandInvoke],
    ] {
        let mut host = RecordingHost::default();
        let mut b = bridge(&permissions);
        let effects = b.on_message(
            &mut host,
            true,
            &envelope("session.open", "deny", Some(json!({"connectionId": "c"}))),
        );
        assert_eq!(
            error_code(&last_post(&effects)).as_deref(),
            Some("E_PERMISSION")
        );
        assert!(host.session_calls.borrow().is_empty());
    }
    let mut host = RecordingHost::default();
    let mut b = bridge(&[
        WappPermission::CommandInvoke,
        WappPermission::SessionStateful,
    ]);
    let mut input = envelope("session.open", "stale", Some(json!({"connectionId": "c"})));
    input["instanceNonce"] = json!("old");
    let effects = b.on_message(&mut host, true, &input);
    assert_eq!(
        error_code(&last_post(&effects)).as_deref(),
        Some("E_PERMISSION")
    );
    assert!(host.session_calls.borrow().is_empty());
}

#[test]
fn sessions_deduplicate_by_reqid_and_end_on_detach() {
    let mut host = RecordingHost::default();
    let mut b = bridge(&[
        WappPermission::CommandInvoke,
        WappPermission::SessionStateful,
    ]);
    let target = json!({"connectionId": "c", "database": "app"});
    let mut input = envelope("session.open", "open", Some(target));
    input["instanceNonce"] = json!("nonce-1");
    b.on_message(&mut host, true, &input);
    b.on_message(&mut host, true, &input);
    assert_eq!(host.session_calls.borrow().len(), 1);
    assert_eq!(host.next_instance, 1);
    let calls = host.session_calls.borrow();
    assert_eq!(calls[0]["action"], "open");
    assert_eq!(calls[0]["instanceId"], "instance-1");
    drop(calls);

    let mut replay = envelope(
        "session.open",
        "open",
        Some(json!({"connectionId": "c", "database": "other"})),
    );
    replay["instanceNonce"] = json!("nonce-1");
    let effects = b.on_message(&mut host, true, &replay);
    assert_eq!(
        error_code(&last_post(&effects)).as_deref(),
        Some("E_BAD_REQUEST")
    );

    let effects = b.detach(&mut host);
    assert_eq!(
        host.session_ends.borrow().as_slice(),
        &["instance-1".to_owned()]
    );
    assert!(effects.contains(&BridgeEffect::RemoveMessageListener));
    assert!(b.on_message(&mut host, true, &input).is_empty());
}

#[test]
fn frame_load_rotates_nonce_and_ends_the_old_instance() {
    let mut host = RecordingHost::default();
    host.nonce_seq = 1;
    let mut b = bridge(&[
        WappPermission::CommandInvoke,
        WappPermission::SessionStateful,
    ]);
    let mut input = envelope("session.open", "open", Some(json!({"connectionId": "c"})));
    input["instanceNonce"] = json!("nonce-1");
    b.on_message(&mut host, true, &input);
    b.on_frame_load(&mut host);
    assert_eq!(b.state.generation, 1);
    assert_eq!(b.state.instance_nonce, "nonce-2");
    assert_eq!(b.state.instance_id, None);
    assert!(b.state.session_requests.is_empty());
    assert_eq!(
        host.session_ends.borrow().as_slice(),
        &["instance-1".to_owned()]
    );

    let old = b.on_message(&mut host, true, &input);
    assert_eq!(
        error_code(&last_post(&old)).as_deref(),
        Some("E_PERMISSION")
    );

    let mut fresh = envelope("session.open", "fresh", Some(json!({"connectionId": "c"})));
    fresh["instanceNonce"] = json!("nonce-2");
    let post = last_post(&b.on_message(&mut host, true, &fresh));
    assert_eq!(post["type"], "session.open.ok");
    assert_eq!(host.session_calls.borrow().len(), 2);
    assert_eq!(host.session_calls.borrow()[1]["instanceId"], "instance-2");
}

#[test]
fn push_theme_snapshot_posts_the_current_theme() {
    let mut host = RecordingHost::default();
    host.dark = true;
    let b = bridge(&[]);
    match b.push_theme_snapshot(&host) {
        BridgeEffect::PostMessage(value) => {
            assert_eq!(value["type"], "theme.apply");
            assert_eq!(value["payload"]["dark"], true);
            assert_eq!(value["payload"]["tokens"]["--c-surface"], "#0b0e14");
        }
        other => panic!("expected a post message, got {other:?}"),
    }
}

#[test]
fn gap_sentinels_and_root_surface_are_pinned() {
    assert_eq!(WAPP_BRIDGE_SURFACE.len(), 9);
    assert!(WAPP_BRIDGE_SURFACE.contains(&"attachBridge"));
    assert_eq!(WAPP_BRIDGE_DOM_GAP.len(), 5);
    assert_eq!(WAPP_BRIDGE_IPC_GAP.len(), 7);
    assert_eq!(WAPP_BRIDGE_EVENT_GAP.len(), 4);
    assert_eq!(WAPP_BRIDGE_HANDLER_GAP.len(), 4);
    assert_eq!(crate::WAPP_BRIDGE_SURFACE.len(), 9);
    assert_eq!(crate::BRIDGE_CHANNEL, BRIDGE_CHANNEL);
    assert_eq!(crate::WAPP_BRIDGE_DOM_GAP.len(), 5);
}
