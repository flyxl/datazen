//! P5 端点身份集成测试（CM-31）：端点重叠检测按「service_key + connectionId」键并集
//! 判定物理端点身份——不同物理端点放行、同一物理端点自覆盖拒绝、身份不可证明时
//! fail-closed。本文件由 `job_kernel.rs` 拆出，保持单文件规模纪律。

mod support;

use datazen_platform_api::id::ConnectionId;

use datazen_runtime::job::{detect_endpoint_overlap, EndpointRef, EndpointRole};

use support::conn;

/// 同名对象落在**两个不同**的物理端点上必须放行——这是数据迁移最常见的一步
/// （A 库的 public.users 拷到 B 库的 public.users）。
#[test]
fn cm31_same_object_on_two_different_endpoints_is_allowed() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["public.users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-bbbb".into(),
            objects: vec!["public.users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    assert!(
        detect_endpoint_overlap(&endpoints).is_ok(),
        "不同物理端点之间拷贝同名对象必须允许"
    );
}

/// 两个不同的连接配置指向同一台服务器（别名）时仍必须拒绝：service_key 是共享的，
/// 而按配置 id 分桶看不见自覆盖。
#[test]
fn cm31_alias_configs_sharing_one_service_are_still_rejected() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("别名自覆盖必须拒绝");
    let datazen_runtime::job::JobError::EndpointOverlap(message) = err else {
        panic!("应当是 EndpointOverlap");
    };
    // 错误必须点名两端与对象，否则用户无从判断该改哪一侧
    assert!(message.contains("`users`"), "{message}");
    assert!(message.contains(conn().as_str()), "{message}");
    assert!(message.contains("conn-2"), "{message}");
}

/// 同一 connectionId 即同一端点：即使位置摘要不同（配置被改过、或摘要不可证明）
/// 也必须拒绝。只看 service_key 的实现会漏掉这一条。
#[test]
fn cm31_one_connection_id_is_one_endpoint_regardless_of_service_key() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-bbbb".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    assert!(
        detect_endpoint_overlap(&endpoints).is_err(),
        "同一个 connectionId 读写同一对象必须拒绝"
    );
}

/// fail-closed：端点给不出任何身份键（既无 service_key 也无 connectionId）且同时
/// 存在 reader 与 writer 时拒绝，不开放危险自覆盖。
#[test]
fn cm31_unprovable_identity_with_writer_is_rejected() {
    let endpoints = vec![
        EndpointRef {
            connection_id: ConnectionId::new("  "),
            service_key: "   ".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-bbbb".into(),
            objects: vec!["orders".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("身份不可证明必须拒绝");
    assert!(
        matches!(err, datazen_runtime::job::JobError::EndpointOverlap(_)),
        "{err:?}"
    );
}
