use std::sync::Arc;

use chrono::{TimeZone, Utc};

use datazen::ai::memory::{MemoryKind, MemorySensitivity, OrgMemoryDecision, OrgMemoryDraft};

use super::{AdminStore, ServerOrgMemories};

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("store");
    store.migrate().expect("migrate");
    Arc::new(store)
}

fn draft(subject: &str, content: &str) -> OrgMemoryDraft {
    OrgMemoryDraft {
        memory_id: None,
        kind: MemoryKind::Glossary,
        subject: subject.into(),
        content: content.into(),
        confidence: 0.9,
        sensitivity: MemorySensitivity::Normal,
    }
}

#[test]
fn publish_versions_list_and_revoke_are_organization_scoped() {
    let store = store();
    let org_a = ServerOrgMemories::scoped("org-a", store.clone());
    let org_b = ServerOrgMemories::scoped("org-b", store.clone());
    let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();

    let first = org_a
        .publish(&draft("Revenue", "Use paid_at"), "admin-a", now)
        .unwrap();
    assert_eq!(first.version, 1);
    assert_eq!(first.approved_by, "admin-a");
    assert_eq!(org_a.list_active().unwrap().len(), 1);
    assert!(org_b.list_active().unwrap().is_empty(), "cross-tenant leak");

    let second = org_a
        .publish(
            &OrgMemoryDraft {
                memory_id: Some(first.id.clone()),
                subject: "Revenue v2".into(),
                content: "Use paid_at month".into(),
                ..draft("Revenue", "Use paid_at")
            },
            "admin-a",
            now,
        )
        .unwrap();
    assert_eq!(second.version, 2);
    let active = org_a.list_active().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].version, 2);
    assert_eq!(active[0].subject, "Revenue v2");

    assert_eq!(org_a.revoke(&first.id, "admin-a", now).unwrap(), 1);
    assert!(org_a.list_active().unwrap().is_empty());
    assert_eq!(
        org_b.revoke(&first.id, "admin-b", now),
        Err(datazen::ai::memory::OrgMemoryError::NotFound)
    );
}

#[test]
fn audit_records_who_org_memory_version_and_decision_without_content() {
    let store = store();
    let org = ServerOrgMemories::scoped("org-a", store);
    let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    let entry = org
        .publish(
            &draft("Revenue", "super-secret-policy-text"),
            "admin-a",
            now,
        )
        .unwrap();
    org.record_injection(
        &entry.id,
        entry.version,
        "member-a",
        OrgMemoryDecision::Injected,
        now,
    )
    .unwrap();
    org.revoke(&entry.id, "admin-a", now).unwrap();
    org.record_injection(
        &entry.id,
        entry.version,
        "member-a",
        OrgMemoryDecision::Injected,
        now,
    )
    .unwrap();

    let audit = org.list_audit().unwrap();
    assert_eq!(audit.len(), 4);
    assert!(audit.iter().all(|row| row.organization_id == "org-a"));
    assert!(audit.iter().all(|row| row.memory_id == entry.id));
    assert!(audit.iter().all(|row| row.version == 1));
    assert!(audit.iter().any(|row| row.action.as_str() == "publish"
        && row.actor_principal_id == "admin-a"
        && row.decision == OrgMemoryDecision::Approved));
    assert!(audit
        .iter()
        .any(|row| row.action.as_str() == "revoke" && row.decision == OrgMemoryDecision::Revoked));
    assert!(audit
        .iter()
        .any(|row| row.action.as_str() == "inject" && row.decision == OrgMemoryDecision::Injected));
    assert!(audit.iter().any(
        |row| row.action.as_str() == "inject" && row.decision == OrgMemoryDecision::Suppressed
    ));
    let serialized = serde_json::to_string(&audit).unwrap();
    assert!(!serialized.contains("super-secret-policy-text"));
}

#[test]
fn injection_audit_cannot_claim_a_revoked_version_was_injected() {
    let store = store();
    let org = ServerOrgMemories::scoped("org-a", store);
    let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    let entry = org
        .publish(&draft("Revenue", "Use paid_at"), "admin-a", now)
        .unwrap();
    org.revoke(&entry.id, "admin-a", now).unwrap();
    org.record_injection(
        &entry.id,
        entry.version,
        "member-a",
        OrgMemoryDecision::Injected,
        now,
    )
    .unwrap();
    let audit = org.list_audit().unwrap();
    assert!(audit.iter().any(
        |row| row.action.as_str() == "inject" && row.decision == OrgMemoryDecision::Suppressed
    ));
}

#[test]
fn secret_or_invalid_drafts_are_rejected_before_storage() {
    let store = store();
    let org = ServerOrgMemories::scoped("org-a", store);
    let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    let mut secret = draft("Revenue", "Use paid_at");
    secret.sensitivity = MemorySensitivity::Secret;
    assert!(org.publish(&secret, "admin-a", now).is_err());
    assert!(org.list_active().unwrap().is_empty());
}
