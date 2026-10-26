//! `platform::adapter` 的结构性用例。
//!
//! 由 `adapter.rs` 以 `#[path]` 在 `#[cfg(test)]` 下装载：`super::` 因此仍指向 `adapter` 模块，
//! 且本文件与 `adapter.rs` 同目录，`include_str!("adapter.rs")` 的解析基准与拆出前逐字节一致。
use super::*;
use datazen_platform_api::id::{ClientInstanceId, EditorSessionId, JobId, StageId};

fn adapter() -> PlatformAdapter {
    PlatformAdapter::with_identity(
        DesktopIdentity::new_launch("datazen-desktop-local", "alice", "client-1")
            .expect("固定输入应当组装成功"),
    )
}

#[test]
fn assembly_state_is_derived_from_injection_not_hand_kept() {
    let adapter = adapter();
    // 这条断言读的是 OnceLock 推导结果，不是常量，因此注入逻辑写错时会失败。
    assert_eq!(adapter.assembly_state(), AssemblyState::IdentityReady);
    let missing: Vec<&str> = adapter
        .assembly_state()
        .missing_steps()
        .iter()
        .map(|(name, _)| *name)
        .collect();
    assert_eq!(
        missing,
        vec!["§6.2-6 Runtime", "§6.2-7 ApplicationServices"],
        "缺哪两步必须可枚举，否则调用方无法定位"
    );
    // 每条缺步记录的目标状态必须比当前状态靠后，否则"补齐到哪"是错的。
    for (_, after) in AssemblyState::IdentityReady.missing_steps() {
        assert_ne!(*after, AssemblyState::IdentityReady);
    }
    assert!(AssemblyState::FullyWired.missing_steps().is_empty());
    // 组装状态与注入事实必须一致：此刻未注入，就取不到应用服务。
    assert!(adapter.application_services().is_none());
    assert_eq!(adapter.identity().principal_id().as_str(), "alice");
    assert!(adapter.handles().is_empty());
}

/// 「补上这一步之后」必须真的**往前走一档**。
///
/// 这条断言盯的是一个真实缺陷：缺步表把 §6.2-7 的目标状态写成
/// `AwaitingApplicationServices`——与自己同一档。于是第 7 步落地之后，
/// 表仍然说它没落地，「补完就前进」这句话是假的。调用方读这张表是为了
/// 回答「还差哪一步、补完会到哪」，两者必须是同一个答案。
#[test]
fn completing_step_seven_advances_the_machine_to_fully_wired() {
    for state in [
        AssemblyState::IdentityReady,
        AssemblyState::AwaitingApplicationServices,
    ] {
        let steps = state.missing_steps();
        let (name, after) = steps.last().expect("尚未 FullyWired 的状态必须列出缺步");
        assert_eq!(
            *name, "§6.2-7 ApplicationServices",
            "{state:?} 的最后一条缺步必须是第 7 步"
        );
        assert_eq!(
            *after,
            AssemblyState::FullyWired,
            "{state:?}：补完 §6.2-7 之后必须到 FullyWired，不能停在原地"
        );
        // 目标状态必须与当前状态不同——停在原地就是「补了也没变」。
        assert_ne!(*after, state);
    }
    // 已经补完的档位不再列缺步。
    assert!(AssemblyState::FullyWired.missing_steps().is_empty());
}

/// 组装完成后：状态机报第 7 步已落地，且**注入的存储端口就在应用服务里**。
///
/// 这条是本轨的端到端证据——§6.2 第 7 步不是「有个 `ApplicationServices` 对象」，
/// 而是「应用服务**持有**了形态中立的存储端口」。端口实例必须与宿主注入的
/// **同一个**（`Arc::ptr_eq` 比地址）：另造一份就是两份事实。
#[tokio::test]
async fn injection_wires_the_storage_ports_into_the_application_services() {
    use crate::store::{DesktopRepository, KeyBackend};
    use datazen_modules::tunnel::TunnelRepository;

    let temp = tempfile::tempdir().expect("临时目录");
    let store = Arc::new(
        DesktopRepository::init_with_path(temp.path(), KeyBackend::File)
            .await
            .expect("临时存储"),
    );
    let tunnels: Arc<dyn TunnelRepository> = store.clone();

    let adapter = adapter();
    adapter
        .inject(
            store.clone(),
            Arc::new(datazen_runtime::driver_registry::DriverRegistry::new()),
            tunnels.clone(),
            crate::platform::known_hosts::desktop_known_hosts_repository(temp.path()),
        )
        .expect("装配必须成功");

    // 状态机如实反映第 7 步已落地。
    assert_eq!(adapter.assembly_state(), AssemblyState::FullyWired);
    assert!(adapter.assembly_state().missing_steps().is_empty());

    let services = adapter
        .require_application_services()
        .expect("注入后必须取得到应用服务");
    // 注入的端口就是宿主创建的那一个实例。
    assert!(
        Arc::ptr_eq(&services.tunnels, &tunnels),
        "应用服务持有的隧道仓储必须是宿主注入的那一个实例"
    );
    // D1 收尾后：设置用例由 datazen-modules 拥有，经 adapter 直接暴露。
    // 同一条端到端证据仍然成立——模块服务读到的必须是同一个 DesktopRepository
    // 事实，而不是另一份缓存或默认实现。
    let shared = adapter
        .require_settings_service()
        .expect("注入后必须取得到模块设置服务")
        .read()
        .await
        .expect("模块设置服务必须可读");
    assert_eq!(
        shared,
        store.get_settings().await.shared_settings(),
        "模块设置服务必须读取同一个 DesktopRepository 事实"
    );
    // 全部接线成功时没有任何降级项：降级表不是「永远非空」的摆设。
    assert!(
        adapter.degraded_capabilities().is_empty(),
        "顺利装配后不得报降级：{:?}",
        adapter.degraded_capabilities()
    );
    assert!(
        adapter.identity_resolver().is_ok(),
        "临时目录 + File keyring 下执行身份解析器必须装配成功"
    );
}

/// 降级装配：**可选接线**失败只关闭它自己，内核照常可用。
///
/// 失败半径的实测证据：`{dataDir}/result-artifacts` 被同名文件占住时（真实的
/// 「目录不可用」，不是 mock 出来的错误），`inject` 不得返回 `Err`；
/// `require_application_services()` 必须仍然可用——设置、连接列表、建连全走它——
/// 只有 `artifact_store()` 报错，且缺席项在 `degraded_capabilities()` 里可枚举。
///
/// 旧形状（`?`）下这里会让**整个**平台装配失败：`profiles` / `runtime` / `services`
/// 一个都装不上，于是所有依赖平台的 IPC 统一变成 `NotConfigured`——把「制品存不了」
/// 放大成「什么都用不了」。
#[tokio::test]
async fn optional_wiring_failure_degrades_only_its_own_capability() {
    use crate::store::{DesktopRepository, KeyBackend};
    use datazen_modules::tunnel::TunnelRepository;

    let temp = tempfile::tempdir().expect("临时目录");
    let store = Arc::new(
        DesktopRepository::init_with_path(temp.path(), KeyBackend::File)
            .await
            .expect("临时存储"),
    );
    let tunnels: Arc<dyn TunnelRepository> = store.clone();
    // `create_dir_all` 撞上同名文件必然失败。
    std::fs::write(temp.path().join("result-artifacts"), b"not a directory").expect("占位文件");

    let adapter = adapter();
    adapter
        .inject(
            store,
            Arc::new(datazen_runtime::driver_registry::DriverRegistry::new()),
            tunnels.clone(),
            crate::platform::known_hosts::desktop_known_hosts_repository(temp.path()),
        )
        .expect("可选接线失败不得让 inject 返回 Err：那是降级，不是装配冲突");

    // 内核在：第 7 步如实落地，应用服务可取。
    assert_eq!(adapter.assembly_state(), AssemblyState::FullyWired);
    let services = adapter
        .require_application_services()
        .expect("内核必须可用：设置与连接列表都走这里");
    assert!(Arc::ptr_eq(&services.tunnels, &tunnels));

    // 缺席项可枚举，且只列真正缺席的那一项（身份解析器在场）。
    assert_eq!(
        adapter.degraded_capabilities(),
        vec!["§6.2-8 结果制品存储（result-artifacts 不可打开）"],
        "降级项必须精确到缺席的那一项，不能把在场项也算进去"
    );
    assert!(
        adapter.identity_resolver().is_ok(),
        "身份解析器不该被制品存储的失败牵连"
    );

    // 用哪一项报哪一项的错：内部码不变，IPC 表面仍收敛到 NotConfigured。
    let err = adapter.artifact_store().err().expect("制品存储必须缺席");
    assert_eq!(err.code, ApiErrorCode::ServiceUnavailable);
    let mapped = crate::platform::error::into_command_error(err);
    assert!(
        matches!(mapped, crate::commands::CommandError::NotConfigured(_)),
        "制品存储缺席在 IPC 表面必须收敛到 NotConfigured，实际 {mapped:?}"
    );
}

#[test]
fn launch_without_a_deterministic_desktop_user_fails_closed() {
    // `new_launch` 依赖运行环境变量；这里只断言它要么成功、要么明确失败，
    // 绝不返回占位主体。
    match PlatformAdapter::new_launch("client-x") {
        Ok(adapter) => assert!(!adapter.identity().principal_id().as_str().is_empty()),
        Err(err) => assert_eq!(err.code, ApiErrorCode::Unauthenticated),
    }
}

#[test]
fn failed_entry_returns_the_original_error_not_a_placeholder_adapter() {
    // 组装失败时，依赖身份的调用必须拿到**原始**原因。
    // 如果这里退化成一个占位 adapter，调用方会以为身份已就绪，
    // 然后把未知主体当成已知主体写进日志/审计 —— 那是 :84 禁止的伪映射。
    let original = ApiError::new(
        ApiErrorCode::Unauthenticated,
        "test: 桌面用户不可解析，身份未组装",
    );
    let entry = PlatformEntry::failed(original.clone());
    assert!(!entry.is_ready());
    let err = entry.require().expect_err("失败的 entry 必须报错");
    assert_eq!(err.code, ApiErrorCode::Unauthenticated);
    assert_eq!(err.message, original.message);
    // 失败时不得凭空造出一个 adapter。
    assert!(PlatformEntry::failed(original.clone()).require().is_err());
}

#[test]
fn ready_entry_hands_back_the_same_adapter() {
    let adapter = adapter();
    let entry = PlatformEntry::ready(adapter);
    assert!(entry.is_ready());
    let recovered = entry.require().expect("成功的 entry 必须给出 adapter");
    assert_eq!(recovered.identity().principal_id().as_str(), "alice");
}

#[test]
fn each_call_gets_a_fresh_request_id_and_a_stable_principal() {
    let adapter = adapter();
    let a = adapter.begin_call();
    let b = adapter.begin_call();
    assert_ne!(a.request_id(), b.request_id());
    assert_eq!(a.context().principal_id.as_str(), "alice");
    assert_eq!(b.context().principal_id.as_str(), "alice");
    assert_eq!(a.context().client_instance_id.as_str(), "client-1");
}

#[test]
fn owned_call_rejects_a_foreign_editor_owner_before_any_use_case_runs() {
    let adapter = adapter();
    let err = adapter
        .begin_owned_call(&OwnerIntent::Editor {
            claimed_client_instance_id: ClientInstanceId::new("client-other"),
            editor_session_id: EditorSessionId::new("tab-1"),
        })
        .err()
        .expect("跨实例 editor owner 必须被拒");
    assert_eq!(err.code, ApiErrorCode::PermissionDenied);
}

#[test]
fn owned_call_accepts_the_current_instance_editor_owner() {
    let adapter = adapter();
    let (scope, owner) = adapter
        .begin_owned_call(&OwnerIntent::Editor {
            claimed_client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("tab-1"),
        })
        .expect("同实例 editor owner 应通过");
    assert!(matches!(
        owner,
        datazen_platform_api::context::OwnerRef::Editor { .. }
    ));
    assert_eq!(scope.context().client_instance_id.as_str(), "client-1");
}

#[test]
fn owned_call_fails_closed_for_job_owners() {
    let adapter = adapter();
    let err = adapter
        .begin_owned_call(&OwnerIntent::Job {
            job_id: JobId::new("job-1"),
            stage_id: StageId::new("stage-1"),
        })
        .err()
        .expect("job owner 必须失败关闭");
    assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
}

#[test]
fn application_services_is_absent_and_the_failure_explains_why() {
    let adapter = adapter();
    assert!(
        adapter.application_services().is_none(),
        "inject() 之前不得凭空造出 ApplicationServices"
    );
    let err = adapter
        .require_application_services()
        .err()
        .expect("未注入必须报错");
    assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
    // 报错必须说明未注入这件事，否则调用方无法定位是启动装配漏了哪一步。
    for needle in ["§6.2", "未注入", "inject", "不得改走旧共享 session"] {
        assert!(
            err.message.contains(needle),
            "报错缺少「{needle}」：{}",
            err.message
        );
    }
}

/// 窄适配的核心约束：**不得回落**。`require_*` 失败时不能偷偷给旧路径开门。
#[test]
fn failing_requirements_never_fall_back_to_the_legacy_path() {
    let adapter = adapter();
    // 三个 require_* 在未注入 / 未接线时的行为都必须是 Err，而不是"尽力而为"。
    assert!(adapter.require_application_services().is_err());
    // 桥在测试进程里可能被其它用例注入，两种结果都必须成功，
    // 关键是**没有第三条"回落"分支**。
    match adapter.require_bridge() {
        Ok(_) => {}
        Err(err) => assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported),
    }
}

/// 注入点常量必须指向真实符号，否则"可定位"的承诺是假的。
///
/// 这条断言随注入缝隙一起搬到了 `packages/application/src/frontend_bridge.rs`，
/// 但它校验的是**宿主侧**符号，所以留在 adapter 的用例里：常量在用例层，
/// 被指名的 `pub fn set_bridge_injection` 在组装层，两端必须同时可读。
#[test]
fn injection_point_names_a_real_symbol() {
    assert_eq!(
        frontend_bridge::INJECTION_POINT,
        "crate::platform::adapter::PlatformAdapter::set_bridge_injection"
    );
    let adapter_source = crate::platform::production_source(include_str!("adapter.rs"));
    assert!(
        adapter_source.contains("pub fn set_bridge_injection"),
        "INJECTION_POINT 指向的符号必须真实存在于 adapter.rs"
    );
}

/// 结构性证据：本模块不含任何 `ConnectionUseCases` 实现，也不出现 `todo!` /
/// `unimplemented!` / `unreachable!` 之类会把半成品伪装成完成的写法。
#[test]
fn no_shell_implementation_of_the_thirteen_use_cases_exists() {
    let source = include_str!("adapter.rs");
    let production = source.split("#[cfg(test)]").next().unwrap_or_default();
    assert!(
        !production.contains("impl ConnectionUseCases"),
        "本 track 不实现 13 个用例：实现就得编造 revision / epoch / 幂等值"
    );
    for forbidden in ["todo!", "unimplemented!", "TODO", "FIXME"] {
        assert!(
            !production.contains(forbidden),
            "adapter.rs 生产路径不得出现 {forbidden}"
        );
    }
}

/// :84 条款一的**语义**证据：`ProfileView` 的默认 revision 就是常量 0。
/// 这正是"伪映射"会落进去的洞：把常量 0 当成真实版本号填进去，
/// `ConfigRevisionMismatch` 判定会永久失效。本测试把这个事实钉住。
#[test]
fn profile_view_defaults_are_placeholders_and_must_not_be_used_on_the_desktop_path() {
    use datazen_platform_api::dto::profile::ProfileView;
    use datazen_platform_api::id::{ConnectionId, Counter};

    let view = ProfileView::new(ConnectionId::new("c-1"), "本地", "postgres");
    // 没有任何真实版本源时，唯一能拿到的就是 ZERO —— 一个编造值。
    assert_eq!(view.config_revision, Counter::ZERO);
    assert_eq!(view.credential_revision, Counter::ZERO);
    // 两个 revision 因此永远相等 ⇒ 版本不一致判定不可用。
    assert_eq!(view.config_revision, view.credential_revision);
    assert!(view.enabled);
    let real = view
        .with_config_revision(Counter::new(7))
        .with_credential_revision(Counter::new(3));
    assert_ne!(real.config_revision, real.credential_revision);
}

/// 各访问器在**未装配**状态下的错误码，钉成可执行断言。
///
/// `adapter.rs` 的访问器逐个报错：`runtime()` / `profiles()` / `live_source()` /
/// `artifact_store()` / `identity_resolver()` 缺项时是 `ServiceUnavailable`；
/// `require_application_services()` 与 `native_tools()` 是 `CapabilityUnsupported`。
/// 两者在 IPC 表面都收敛成 `CommandError::NotConfigured`（`platform/error.rs` 的窄适配）。
///
/// 这条把错误码从**注释**变成**断言**：改任何一个访问器的错误码、或把窄适配的
/// 收敛档位改掉，本条都会红。注释本身不被校验，行为才是。
///
/// **降级**那一格（同样的 `ServiceUnavailable`，但内核仍在）由
/// `optional_wiring_failure_degrades_only_its_own_capability` 覆盖——两者是
/// 不同状态，不能用一个断言代替另一个。
#[test]
fn 未注入时各访问器的错误码与注释写的一致() {
    let adapter = adapter();

    let service_unavailable: Vec<(&str, Result<(), ApiError>)> = vec![
        ("artifact_store", adapter.artifact_store().map(|_| ())),
        ("live_source", adapter.live_source().map(|_| ())),
        ("runtime", adapter.runtime().map(|_| ())),
        ("profiles", adapter.profiles().map(|_| ())),
        ("identity_resolver", adapter.identity_resolver().map(|_| ())),
    ];
    for (name, result) in service_unavailable {
        let err = result
            .err()
            .unwrap_or_else(|| panic!("{name} 未注入必须报错"));
        assert_eq!(
            err.code,
            ApiErrorCode::ServiceUnavailable,
            "{name} 未注入时的 ApiError 码必须是 ServiceUnavailable"
        );
    }

    let capability_unsupported: Vec<(&str, Result<(), ApiError>)> = vec![
        (
            "require_application_services",
            adapter.require_application_services().map(|_| ()),
        ),
        ("native_tools", adapter.native_tools().map(|_| ())),
    ];
    for (name, result) in capability_unsupported {
        let err = result
            .err()
            .unwrap_or_else(|| panic!("{name} 未注入必须报错"));
        assert_eq!(
            err.code,
            ApiErrorCode::CapabilityUnsupported,
            "{name} 未注入时的 ApiError 码必须是 CapabilityUnsupported"
        );
    }

    // IPC 表面：两个码都收敛到 `NotConfigured`，前端拿到的分类一致。
    for code in [
        ApiErrorCode::ServiceUnavailable,
        ApiErrorCode::CapabilityUnsupported,
    ] {
        let mapped = crate::platform::error::into_command_error(ApiError::new(code, "boom"));
        assert!(
            matches!(mapped, crate::commands::CommandError::NotConfigured(_)),
            "{code:?} 在 IPC 表面必须收敛到 NotConfigured，实际 {mapped:?}"
        );
    }
}
