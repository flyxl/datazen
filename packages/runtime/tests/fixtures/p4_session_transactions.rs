use super::*;
use datazen_runtime::application::{IdlePolicy, SessionTransactionOperation as Op};
#[tokio::test]
async fn concurrent_begin_is_one_actor_transaction_and_commit_rollback_retire_handles() {
    let (service, backend, _, _) = setup();
    let c = ctx("user", "client");
    let opened = open(&service, &c, "transaction").await;
    let id = opened.session.handle.db_session_id.as_str();
    let (a, b) = tokio::join!(
        service.session_transaction(&c, id, Op::Begin),
        service.session_transaction(&c, id, Op::Begin)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(lock(&backend.ids).len(), 1);
    assert!(service.session_transaction_status(&c, id).await.unwrap());
    assert!(service
        .close_session(
            &c,
            CloseSessionRequest::new(
                opened.session.handle.clone(),
                CloseMode::RequireNoTransaction
            )
        )
        .await
        .is_err());
    service
        .session_transaction(&c, id, Op::Commit)
        .await
        .unwrap();
    assert!(!service.session_transaction_status(&c, id).await.unwrap());
    assert!(lock(&backend.transactions).is_empty());
    service
        .session_transaction(&c, id, Op::Begin)
        .await
        .unwrap();
    service
        .session_transaction(&c, id, Op::Rollback)
        .await
        .unwrap();
    assert!(!service.session_transaction_status(&c, id).await.unwrap());
    assert!(lock(&backend.transactions).is_empty());
}
#[tokio::test]
async fn idle_eviction_clears_transaction_authority_and_old_controls_never_dispatch() {
    let (service, backend, _, _) = setup();
    service.with_idle_policy(IdlePolicy::with_deadline(
        std::time::Duration::ZERO,
        std::time::Duration::from_millis(1),
    ));
    let c = ctx("user", "client");
    let opened = open(&service, &c, "expired-transaction").await;
    let id = opened.session.handle.db_session_id.as_str();
    service
        .session_transaction(&c, id, Op::Begin)
        .await
        .unwrap();
    let before = lock(&backend.ids).len();
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
    assert!(!service.session_transaction_status(&c, id).await.unwrap());
    assert!(service
        .session_transaction(&c, id, Op::Commit)
        .await
        .is_err());
    assert!(service
        .session_transaction(&c, id, Op::Rollback)
        .await
        .is_err());
    assert_eq!(lock(&backend.ids).len(), before);
    assert_eq!(backend.closed.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn unknown_commit_is_not_replayed_and_foreign_owner_cannot_control() {
    let (service, backend, _, _) = setup();
    let c = ctx("user", "client");
    let opened = open(&service, &c, "unknown-transaction").await;
    let id = opened.session.handle.db_session_id.as_str();
    assert!(service
        .session_transaction(&ctx("other", "client"), id, Op::Begin)
        .await
        .is_err());
    assert!(lock(&backend.ids).is_empty());
    service
        .session_transaction(&c, id, Op::Begin)
        .await
        .unwrap();
    backend.unknown_commit.store(true, Ordering::SeqCst);
    assert!(service
        .session_transaction(&c, id, Op::Commit)
        .await
        .is_err());
    let after = lock(&backend.ids).len();
    assert!(service.session_transaction_status(&c, id).await.is_err());
    assert!(service
        .session_transaction(&c, id, Op::Commit)
        .await
        .is_err());
    assert!(service
        .session_transaction(&c, id, Op::Begin)
        .await
        .is_err());
    assert_eq!(lock(&backend.ids).len(), after);
}
