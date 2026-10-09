//! 端点重叠与多端预算门禁旅程（CM-31 / CM-65）。
//!
//! 从 `job_kernel.rs` 拆出，理由是单文件 ≤800 行纪律（AGENTS.md「单文件规模与模块拆分」）。
//! 拆分只搬运代码：夹具与导入经 `use super::*;` 全部来自父模块，断言与被测调用逐字未改。

use super::*;

// ------------------------------------------------------ 预算与重叠

#[test]
fn cm31_overlap_is_rejected_before_any_permit_is_held() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "svc-1".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: conn(),
            service_key: "svc-1".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("overlap");
    assert!(
        matches!(err, datazen_runtime::job::JobError::EndpointOverlap(_)),
        "{err:?}"
    );

    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    let mut guard = ledger.lock().expect("lock");
    guard.ensure_service(&conn());
    drop(guard);
    let permits = datazen_runtime::job::MultiEndpointPermits::reserve(
        ledger.clone(),
        &endpoints,
        ResourceClass::Job,
        &org(),
        &PrincipalId::new("user-a"),
        0,
    );
    assert!(matches!(
        permits,
        Err(datazen_runtime::job::JobError::EndpointOverlap(_))
    ));
}

// CM-31 另有重叠检测用例见同目录 `job_endpoint_identity.rs`（本文件规模已到上限，
// 端点身份的成组断言在那里集中维护）。

#[test]
fn cm65_multi_endpoint_reserve_is_all_or_nothing() {
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    {
        let mut guard = ledger.lock().expect("lock");
        guard.ensure_service(&conn());
        guard.ensure_service(&ConnectionId::new("conn-2"));
    }
    // EndpointRef has no service_id field; use service_key:
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "svc-a".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "svc-b".into(),
            objects: vec!["orders".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let permits = datazen_runtime::job::MultiEndpointPermits::reserve(
        ledger.clone(),
        &endpoints,
        ResourceClass::Job,
        &org(),
        &PrincipalId::new("user-a"),
        0,
    )
    .expect("reserve-all");
    assert_eq!(permits.permits().len(), 2, "全组预留");
    let held = ledger
        .lock()
        .expect("lock")
        .permits_of(&PrincipalId::new("user-a"));
    assert_eq!(held, 2);
    let _ = permits.release(true);
}

/// 反向：预算不足/服务未注册时 try_admit_many 整组回滚，一个许可都不持有。
#[test]
fn cm65_insufficient_budget_admits_no_permits_at_all() {
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(config(8, [1, 1, 1, 0]))));
    {
        let mut guard = ledger.lock().expect("lock");
        guard.ensure_service(&conn());
        // conn-2 故意不注册：该组里任何一个端点不可准入都必须整组回滚
    }
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "svc-a".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "svc-b".into(),
            objects: vec!["orders".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let result = datazen_runtime::job::MultiEndpointPermits::reserve(
        ledger.clone(),
        &endpoints,
        ResourceClass::Job,
        &org(),
        &PrincipalId::new("user-a"),
        0,
    );
    assert!(matches!(
        result,
        Err(datazen_runtime::job::JobError::BudgetDenied(_))
    ));
    let held = ledger
        .lock()
        .expect("lock")
        .permits_of(&PrincipalId::new("user-a"));
    assert_eq!(held, 0, "整组回滚后不得持有任何许可");
}
