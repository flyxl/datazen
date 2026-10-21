//! 桌面组织作用域的**来源**与**可用性**（验证轨道 D）。
//!
//! 这组用例盯的不是「跨组织读不到」——那由
//! [`crate::dashboard::store_tests`] / [`crate::dashboard::runs`] /
//! [`crate::workflow::registry`] 各自的闸门用例承担。这里盯的是另一件事：
//!
//! * 桌面自己那一个组织的**权威来源**是不是安装级常量
//!   （[`super::identity::LOCAL_ORGANIZATION_ID`]）；
//! * IPC 路径取作用域的方式，与后台路径（monitor / MCP / 调度器）取作用域的方式，
//!   是不是指向**同一个**组织；
//! * IPC 路径取作用域时，对 platform adapter 的**可用性**有没有额外依赖。

use super::super::identity::LOCAL_ORGANIZATION_ID;
use super::super::{DesktopIdentity, PlatformAdapter, PlatformEntry};
use super::{is_local_scope, local_organization_scope};
use datazen_application::error::{ApiError, ApiErrorCode};
use datazen_feature_core::dashboard::store::DashboardStore;
use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;

/// 别的组织的作用域。
fn foreign_scope() -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new("another-organization"))
}

/// 一个**组装成功**的 adapter：身份写死，不依赖运行环境的 OS 用户名。
fn ready_entry() -> PlatformEntry {
    PlatformEntry::ready(PlatformAdapter::with_identity(
        DesktopIdentity::new_launch(LOCAL_ORGANIZATION_ID, "alice", "client-1")
            .expect("固定身份应当组装成功"),
    ))
}

/// 一个**组装失败**的 adapter：模拟「取不到当前桌面用户」。
fn failed_entry() -> PlatformEntry {
    PlatformEntry::failed(ApiError::new(
        ApiErrorCode::Unauthenticated,
        "无法确定当前桌面登录用户，拒绝以占位主体继续",
    ))
}

/// 桌面自己的组织作用域必须就是那个安装级常量。
#[test]
fn the_local_scope_carries_the_desktop_organization_id() {
    assert_eq!(
        local_organization_scope().organization_id().as_str(),
        LOCAL_ORGANIZATION_ID
    );
}

/// 只有本组织的作用域才算「本地」。
#[test]
fn only_the_local_scope_counts_as_local() {
    assert!(is_local_scope(&local_organization_scope()));
    assert!(!is_local_scope(&foreign_scope()));
}

/// IPC 路径与后台路径必须指向**同一个**组织。
///
/// 后台路径（monitor / MCP / 调度器）用的是安装级常量
/// [`local_organization_scope`]；IPC 路径用的是 adapter 组装的请求上下文。
/// 两条路径看到的组织必须一致，否则同一份数据会在两条路径上表现不同。
#[tokio::test]
async fn the_ipc_scope_and_the_background_scope_name_the_same_organization() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let mut state = test.state.clone();
    state.platform = ready_entry();

    let ipc = state
        .organization_scope()
        .expect("adapter 就绪时 IPC 作用域应当可用");
    assert_eq!(ipc.organization_id().as_str(), LOCAL_ORGANIZATION_ID);
    assert_eq!(
        ipc.organization_id(),
        local_organization_scope().organization_id(),
        "IPC 路径与后台路径必须看到同一个组织"
    );
}

/// 【P0 行为回归·表征用例】adapter 组装失败时，IPC 路径**取不到**作用域。
///
/// 桌面是单租户，组织是安装级常量，与「当前 OS 用户名能不能取到」无关。
/// 改造前仪表板 / workflow 命令不依赖 adapter；改造后它们经
/// [`crate::commands::AppState::organization_scope`] 取作用域，
/// 于是 adapter 不可用时这些命令整体失败。
///
/// 这条用例把当前行为**钉住**，以便修法落地时能明确看到行为变化。
#[tokio::test]
async fn the_ipc_scope_is_unavailable_when_the_platform_adapter_failed_to_assemble() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let mut state = test.state.clone();
    state.platform = failed_entry();

    let error = state
        .organization_scope()
        .expect_err("adapter 组装失败时 IPC 作用域必须失败");
    assert!(
        format!("{error:?}").contains("Internal"),
        "期望是内部错误（adapter 不可用），实际：{error:?}"
    );
}

/// 【P0】桌面存储层**本身**不需要 platform adapter。
///
/// 存储实现只按 `is_local_scope` 判定组织，与 adapter 无关：作用域取自
/// 安装级常量时，adapter 不可用也能正常读写本组织的数据。
///
/// 这条用例证明：改造引入的依赖只落在**作用域的来源**上，不在存储层。
#[tokio::test]
async fn the_desktop_store_serves_its_own_organization_without_a_platform_adapter() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let mut state = test.state.clone();
    state.platform = failed_entry();

    let scope = local_organization_scope();
    let app_db = state.store.app_db();
    let now = chrono::Utc::now().to_rfc3339();
    app_db
        .upsert_dashboard_row(
            &scope,
            &crate::store::DashboardRecord {
                id: "dash-1".into(),
                name: "Board".into(),
                created_at: now.clone(),
                updated_at: now,
                layout_cols: 12,
                layout_row_height: 80,
                enabled: true,
                refresh_paused: false,
            },
        )
        .expect("本组织的数据应当写得进去");
    assert_eq!(app_db.list_dashboard_rows(&scope).unwrap().len(), 1);

    // 别的组织仍然读不到：闸门与 adapter 无关。
    assert!(app_db
        .list_dashboard_rows(&foreign_scope())
        .unwrap()
        .is_empty());
}
