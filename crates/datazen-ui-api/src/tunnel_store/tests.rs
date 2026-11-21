use super::*;
use crate::types::SavedTunnelSummary;
use crate::SavedTunnelKind;

fn summary(id: &str) -> SavedTunnelSummary {
    SavedTunnelSummary {
        id: id.to_owned(),
        name: id.to_owned(),
        kind: SavedTunnelKind::Ssh,
    }
}

fn tunnel_input(id: Option<String>) -> TunnelInput {
    TunnelInput {
        id,
        name: "prod".to_owned(),
        kind: SavedTunnelKind::Ssh,
        ssh: None,
        http_proxy: None,
        websocket: None,
    }
}

#[test]
fn surface_gap_and_id_grammar_are_pinned() {
    assert_eq!(TUNNEL_STORE_SURFACE.len(), 8);
    assert_eq!(TUNNEL_STORE_GAP.len(), 5);
    assert_eq!(new_tunnel_id("0.abc12345xyz"), "tun_abc12345");
    assert_eq!(new_tunnel_id("0.short"), "tun_");
    assert_eq!(new_tunnel_id(""), "tun_");
}

#[test]
fn error_message_prefers_a_nonempty_error() {
    assert_eq!(error_message(Some("offline"), "fallback"), "offline");
    assert_eq!(error_message(Some(""), "fallback"), "fallback");
    assert_eq!(error_message(None, "fallback"), "fallback");
}

#[test]
fn load_short_circuits_unless_forced() {
    let fresh = TunnelStoreState::default();
    let begun = match begin_load(&fresh, false) {
        Some(state) => state,
        None => panic!("a fresh store must load"),
    };
    assert!(begun.loading);
    assert!(begun.error.is_none());
    assert!(begun.summaries.is_empty());

    let loaded = TunnelStoreState {
        summaries: vec![summary("tun_x")],
        loaded: true,
        loading: false,
        error: None,
    };
    assert_eq!(begin_load(&loaded, false), None);
    let forced = match begin_load(&loaded, true) {
        Some(state) => state,
        None => panic!("force must reload"),
    };
    assert!(forced.loading);
    assert!(forced.error.is_none());
    assert_eq!(forced.summaries, loaded.summaries);
}

#[test]
fn settling_a_load_replaces_the_collection_and_flags() {
    let begun = TunnelStoreState {
        loading: true,
        error: None,
        ..Default::default()
    };
    let ok = settle_load(
        &begun,
        Ok(vec![summary("tun_1"), summary("tun_2")]),
        "T:tunnelStore.loadFailed",
    );
    assert_eq!(ok.summaries.len(), 2);
    assert!(ok.loaded);
    assert!(!ok.loading);
    assert_eq!(ok.error, None);

    let failed = settle_load(
        &begun,
        Err("permission denied".to_owned()),
        "T:tunnelStore.loadFailed",
    );
    assert!(failed.summaries.is_empty());
    assert!(failed.loaded);
    assert!(!failed.loading);
    assert_eq!(failed.error.as_deref(), Some("permission denied"));

    let fallback = settle_load(&begun, Err(String::new()), "T:tunnelStore.loadFailed");
    assert_eq!(fallback.error.as_deref(), Some("T:tunnelStore.loadFailed"));
}

#[test]
fn create_generates_an_id_then_saves_and_force_loads() {
    let plan = plan_create(&tunnel_input(None), "0.abc12345xyz");
    assert_eq!(plan.tunnel.id, "tun_abc12345");
    assert_eq!(plan.tunnel.name, "prod");
    assert_eq!(plan.tunnel.kind, SavedTunnelKind::Ssh);
    assert_eq!(
        plan.effects,
        vec![
            TunnelStoreEffect::SaveTunnel(plan.tunnel.clone()),
            TunnelStoreEffect::ForceLoad,
        ]
    );

    let explicit = plan_create(&tunnel_input(Some("given".to_owned())), "0.abc12345xyz");
    assert_eq!(explicit.tunnel.id, "given");
    let blank = plan_create(&tunnel_input(Some(String::new())), "0.abc12345xyz");
    assert_eq!(blank.tunnel.id, "tun_abc12345");
}

#[test]
fn update_remove_and_usage_have_fixed_effect_sequences() {
    let saved = plan_create(&tunnel_input(None), "0.abc12345xyz").tunnel;
    assert_eq!(
        plan_update(&saved),
        vec![
            TunnelStoreEffect::SaveTunnel(saved.clone()),
            TunnelStoreEffect::ForceLoad,
        ]
    );
    assert_eq!(
        plan_remove("tun_1"),
        vec![
            TunnelStoreEffect::DeleteTunnel("tun_1".to_owned()),
            TunnelStoreEffect::ForceLoad,
        ]
    );
    assert_eq!(
        plan_usage("tun_1"),
        vec![TunnelStoreEffect::GetUsage("tun_1".to_owned())]
    );
}

#[test]
fn usage_keeps_both_parallel_lists() {
    let usage = usage_from_lists(
        vec!["c1".to_owned(), "c2".to_owned()],
        vec!["One".to_owned(), "Two".to_owned()],
    );
    assert_eq!(usage.connection_ids, vec!["c1", "c2"]);
    assert_eq!(usage.connection_names, vec!["One", "Two"]);
}

#[test]
fn crate_root_reexports_the_tunnel_store_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::TunnelStoreState>();
    resolves::<crate::TunnelInput>();
    resolves::<crate::TunnelStoreEffect>();
    assert_eq!(crate::new_tunnel_id("0.abcdefghij"), "tun_abcdefgh");
    assert_eq!(crate::TUNNEL_STORE_GAP.len(), 5);
}
