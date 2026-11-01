//! `ServerKnownHosts` 行为钉子：端口契约的两条特殊语义（`load` 不失败、
//! `save` 全表替换）+ 每组织实例隔离。

use std::collections::HashMap;
use std::sync::Arc;

use datazen::tunnel::domain::{KnownHostEntry, KnownHostsRepository};

use super::known_hosts::ServerKnownHosts;
use super::AdminStore;

fn migrated_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

fn entry(fingerprint: &str) -> KnownHostEntry {
    KnownHostEntry {
        fingerprint: fingerprint.into(),
        algorithm: "ssh-ed25519".into(),
        public_key: format!("ssh-ed25519 AAAA{fingerprint}"),
    }
}

#[test]
fn empty_store_loads_empty_by_default() {
    let store = migrated_store();
    let hosts = ServerKnownHosts::new(store, "org-a");
    assert!(hosts.load().is_empty(), "TOFU: no records means empty, not error");
}

#[test]
fn save_then_load_round_trips() {
    let store = migrated_store();
    let hosts = ServerKnownHosts::new(store, "org-a");

    let mut entries = HashMap::new();
    entries.insert("bastion.example.com:22".to_string(), entry("SHA256:one"));
    entries.insert("db.internal:2222".to_string(), entry("SHA256:two"));
    hosts.save(&entries).expect("save succeeds");

    let loaded = hosts.load();
    assert_eq!(loaded, entries);
}

#[test]
fn save_is_a_full_table_replace() {
    let store = migrated_store();
    let hosts = ServerKnownHosts::new(store, "org-a");

    let mut first = HashMap::new();
    first.insert("a.example.com:22".to_string(), entry("SHA256:a"));
    first.insert("b.example.com:22".to_string(), entry("SHA256:b"));
    hosts.save(&first).expect("first save");

    // 全表替换：不在第二份里的条目必须消失（whole-table write 是端口契约）。
    let mut second = HashMap::new();
    second.insert("c.example.com:22".to_string(), entry("SHA256:c"));
    hosts.save(&second).expect("second save");

    let loaded = hosts.load();
    assert_eq!(loaded.len(), 1, "stale entries must be gone: {loaded:?}");
    assert!(loaded.contains_key("c.example.com:22"));
}

#[test]
fn per_organization_instances_are_isolated() {
    let store = migrated_store();
    let org_a = ServerKnownHosts::new(store.clone(), "org-a");
    let org_b = ServerKnownHosts::new(store.clone(), "org-b");

    let mut entries = HashMap::new();
    entries.insert("shared.example.com:22".to_string(), entry("SHA256:same"));
    org_a.save(&entries).expect("save in org-a");

    assert!(
        org_b.load().is_empty(),
        "organization isolation: org-b must not see org-a's entries"
    );

    let mut other = HashMap::new();
    other.insert("b-only.example.com:22".to_string(), entry("SHA256:other"));
    org_b.save(&other).expect("save in org-b");
    assert_eq!(org_a.load().len(), 1, "org-a unaffected by org-b's save");
    assert_eq!(org_b.load().len(), 1);
}
