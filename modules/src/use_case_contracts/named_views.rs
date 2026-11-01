//! named_views 用例的双传输规格：binding 端口、共享夹具与七个 scenario。
//!
//! 规格本体（本文件除 `baseline` 外的一切）不感知 IPC/HTTP 词汇：
//! scenario 只经 [`NamedViewsContractBinding`] 触达用例，入参出参与断言
//! 全部落在 `datazen::named_views` 的 application 词汇上。两条宿主腿与
//! 无宿主基线各自实现同一个 binding：
//!
//! - 基线（本文件 `baseline`，`cfg(test)`）：直调 `NamedViewService`——
//!   宿主整体换掉后规格仍原样过，是 commands-layer-review §1 判据的本体；
//! - HTTP 腿：`server/tests/use_case_contracts.rs`（WR-C2 §6.9 端点形态）；
//! - IPC 腿：桌面 `commands/named_views_use_case_contract_tests.rs`
//!   （真实 `#[tauri::command]` 入口）。
//!
//! 两腿的 scenario 测试与本文件 scenario 函数**逐字同名**，绿名单逐条
//! 对得上——「两腿同绿」因此可核对到每条用例。

use async_trait::async_trait;

use crate::named_views::{
    NamedView, NamedViewError, NamedViewIdMinter, NamedViewRepository, NamedViewScope,
};
use crate::use_case_contracts::{ContractFailure, ContractResult};

// ── 传输装配端口 ─────────────────────────────────────────────────────────

/// named_views 用例的传输装配端口（binding harness）。
///
/// 每条腿实现一次：`seed` 是 setup——把夹具记录**直写**进本腿装配的存储
/// （不经用例，对应生产的「存储里已有/没有这些行」）；三个用例方法则经
/// 本腿的真实传输（tauri command / axum handler）触达 application。规格
/// 对实现方式零假设。
#[async_trait]
pub trait NamedViewsContractBinding: Send + Sync {
    /// setup：夹具记录直写本腿存储。
    async fn seed(&self, views: Vec<NamedView>) -> Result<(), String>;

    /// `list` 用例经本腿传输。
    async fn list(&self, scope: Option<NamedViewScope>) -> ContractResult<Vec<NamedView>>;

    /// `save` 用例经本腿传输。
    async fn save(&self, view: NamedView) -> ContractResult<NamedView>;

    /// `delete` 用例经本腿传输。
    async fn delete(&self, id: String) -> ContractResult<Vec<NamedView>>;
}

// ── 共享夹具（setup 材料）─────────────────────────────────────────────────

/// 内存仓储假体：整集合读写，别无其它（与 `named_views::tests` 的
/// MemoryRepository 同源语义；导出为共享夹具供 HTTP 腿与基线装配）。
#[derive(Default)]
pub struct MemoryViewStore {
    views: std::sync::Mutex<Vec<NamedView>>,
}

impl MemoryViewStore {
    /// setup 直写：替换整集合。
    pub async fn seed(&self, views: Vec<NamedView>) {
        *self.views.lock().expect("memory view store lock") = views;
    }
}

#[async_trait]
impl NamedViewRepository for MemoryViewStore {
    async fn load_views(&self) -> Result<Vec<NamedView>, NamedViewError> {
        Ok(self.views.lock().expect("memory view store lock").clone())
    }

    async fn save_views(&self, views: &[NamedView]) -> Result<(), NamedViewError> {
        *self.views.lock().expect("memory view store lock") = views.to_vec();
        Ok(())
    }
}

/// 规格自带的 id 铸造假体（宿主贡献 id 铸造是端口契约；铸造**格式**是
/// 宿主的事，规格只断言「非空且稳定」）。
#[derive(Default)]
pub struct ContractIdMinter {
    counter: std::sync::Mutex<u32>,
}

impl NamedViewIdMinter for ContractIdMinter {
    fn mint_id(&self) -> String {
        let mut counter = self.counter.lock().expect("contract id minter lock");
        *counter += 1;
        format!("contract-view-{counter}")
    }
}

/// 主 scope：所有「同 scope」规则的舞台。
pub fn contract_scope() -> NamedViewScope {
    NamedViewScope {
        connection_id: "conn-contract".into(),
        database: Some("app".into()),
        schema: None,
        table: "orders".into(),
    }
}

/// 对照 scope：证明规则止于 scope 边界（同名跨 scope 合法、default 不跨
/// scope 清除）。取值刻意 URL 安全（两腿装配都可能把它编进查询参数）。
pub fn other_scope() -> NamedViewScope {
    NamedViewScope {
        connection_id: "conn-contract".into(),
        database: Some("app".into()),
        schema: None,
        table: "customers".into(),
    }
}

/// 夹具**已存**视图：带稳定 id，供 `seed` 直写存储。
pub fn stored_fixture(
    id: &str,
    name: &str,
    scope: &NamedViewScope,
    is_default: bool,
) -> NamedView {
    NamedView {
        id: id.into(),
        name: name.into(),
        scope: scope.clone(),
        filters: Vec::new(),
        filter_logic: "and".into(),
        sorts: Vec::new(),
        visible_columns: None,
        column_order: None,
        is_default,
        created_at: chrono::Utc::now(),
        updated_at: None,
    }
}

/// 夹具**草稿**视图：空 id（铸造路径），供 `save` 组装 request。
pub fn draft_fixture(name: &str, scope: &NamedViewScope) -> NamedView {
    let mut view = stored_fixture("", name, scope, false);
    view.created_at = chrono::Utc::now();
    view
}

/// application 错误 → 形态中立观测（基线腿的装配映射；两腿各自的映射
/// 必须产出与这里相同的 code/message 词汇）。
pub fn application_failure(error: NamedViewError) -> ContractFailure {
    ContractFailure::from_application(error.code().map(str::to_string), error.to_string())
}

// ── scenarios（规格本体：setup → request → 断言）──────────────────────────

/// 正常路径：铸造保存返回**落库后的记录**（id 已铸、时间已盖），且经用例
/// 可读回——返回值即存储事实，两腿一致。
pub async fn saves_a_new_view_and_returns_the_stored_record(
    binding: &dyn NamedViewsContractBinding,
) {
    binding.seed(Vec::new()).await.expect("seed an empty store");

    let draft = draft_fixture("Monthly orders", &contract_scope());
    let saved = binding
        .save(draft)
        .await
        .expect("a valid draft saves fine");

    assert!(
        !saved.id.trim().is_empty(),
        "the backend mints a non-empty id"
    );
    assert_eq!(saved.name, "Monthly orders");
    assert!(
        saved.updated_at.is_some(),
        "a new record is stamped on save"
    );

    let list = binding.list(None).await.expect("list after save");
    assert_eq!(list.len(), 1, "the saved record is the only one");
    assert_eq!(list[0].id, saved.id, "the minted id is stable");
    assert_eq!(list[0].created_at, saved.created_at);
}

/// 正常路径：菜单序（default 先、其余按名字不分大小写）与 scope 过滤。
/// 排序与过滤是用例规则，不是存储顺序——两腿必须给出同一个序。
pub async fn listing_orders_default_first_then_by_name(
    binding: &dyn NamedViewsContractBinding,
) {
    binding
        .seed(vec![
            stored_fixture("view-a", "beta", &contract_scope(), false),
            stored_fixture("view-b", "Alpha", &contract_scope(), true),
            stored_fixture("view-c", "gamma", &contract_scope(), false),
            stored_fixture("view-d", "Alpha", &other_scope(), false),
        ])
        .await
        .expect("seed four fixture views");

    let all = binding.list(None).await.expect("list every scope");
    assert_eq!(all.len(), 4, "no scope filter lists every scope");
    let ids: Vec<&str> = all.iter().map(|view| view.id.as_str()).collect();
    assert_eq!(
        ids,
        ["view-b", "view-d", "view-a", "view-c"],
        "default first, then by name case-insensitively"
    );

    let scoped = binding
        .list(Some(contract_scope()))
        .await
        .expect("list one scope");
    assert_eq!(scoped.len(), 3, "the scope filter keeps its own views");
    let ids: Vec<&str> = scoped.iter().map(|view| view.id.as_str()).collect();
    assert_eq!(ids, ["view-b", "view-a", "view-c"]);
}

/// 正常路径：更新（已知 id）保留身份与创建时间；改名后经用例读回的是
/// 同一条记录。
pub async fn renaming_keeps_the_identity_and_the_creation_time(
    binding: &dyn NamedViewsContractBinding,
) {
    let seeded = stored_fixture("view-1", "Users", &contract_scope(), false);
    let created_at = seeded.created_at;
    binding.seed(vec![seeded]).await.expect("seed one view");

    let mut rename = draft_fixture("Customers", &contract_scope());
    rename.id = "view-1".into();
    let saved = binding.save(rename).await.expect("a known id updates");

    assert_eq!(saved.id, "view-1", "the identity is kept");
    assert_eq!(saved.name, "Customers", "the name moved");
    assert_eq!(
        saved.created_at, created_at,
        "the creation time is kept, not re-stamped"
    );
    assert!(saved.updated_at.is_some(), "the update time moved");

    let list = binding.list(None).await.expect("list after rename");
    assert_eq!(list.len(), 1, "a rename does not fork the record");
    assert_eq!(list[0].name, "Customers");
}

/// 正常路径：一 scope 一 default——把新视图提为 default 的**同一次写**
/// 清掉同 scope 旧 default；对照 scope 的 default 不受牵连。
pub async fn the_one_default_per_scope_rule_is_enforced_in_one_write(
    binding: &dyn NamedViewsContractBinding,
) {
    binding
        .seed(vec![
            stored_fixture("view-1", "Users", &contract_scope(), true),
            stored_fixture("view-2", "Customers", &contract_scope(), false),
            stored_fixture("view-3", "Users", &other_scope(), true),
        ])
        .await
        .expect("seed three fixture views");

    let mut promote = draft_fixture("Archive", &contract_scope());
    promote.is_default = true;
    let saved = binding.save(promote).await.expect("a valid draft saves");

    assert!(saved.is_default, "the new default is the saved record");
    let scoped = binding
        .list(Some(contract_scope()))
        .await
        .expect("list the scope");
    let defaults: Vec<&NamedView> = scoped.iter().filter(|view| view.is_default).collect();
    assert_eq!(
        defaults.len(),
        1,
        "at most one default per scope after the write"
    );
    assert_eq!(defaults[0].id, saved.id, "the newcomer holds the flag");

    let others = binding
        .list(Some(other_scope()))
        .await
        .expect("list the other scope");
    assert!(
        others.iter().any(|view| view.id == "view-3" && view.is_default),
        "the rule stops at the scope boundary"
    );
}

/// 正常路径：删除幂等——未知 id 不是错误（重试语义），已知 id 删除后
/// 用例读回空集。
pub async fn deleting_an_unknown_id_is_a_no_op_and_the_known_id_is_gone(
    binding: &dyn NamedViewsContractBinding,
) {
    binding
        .seed(vec![stored_fixture("view-1", "Users", &contract_scope(), false)])
        .await
        .expect("seed one view");

    let unchanged = binding
        .delete("no-such-view".into())
        .await
        .expect("an unknown id deletes as a no-op");
    assert_eq!(unchanged.len(), 1, "the store is untouched");

    let remaining = binding
        .delete("view-1".into())
        .await
        .expect("the known id deletes");
    assert!(remaining.is_empty(), "the answer is the remaining set");

    let list = binding.list(None).await.expect("list after delete");
    assert!(list.is_empty(), "the view is gone for good");
}

/// 错误路径（规则拒绝族）：scope 内同名不分大小写拒绝；拒绝携带契约码
/// `grid.view.nameExists` 与 application 固定句——**文本不携带用户输入**
/// （D1：宿主消息只说宿主知道的）。跨 scope 同名是合法的另一张表。
pub async fn duplicate_names_in_one_scope_are_refused(
    binding: &dyn NamedViewsContractBinding,
) {
    binding
        .seed(vec![
            stored_fixture("view-1", "Users", &contract_scope(), false),
            // D1 的风险形状本身在存量里：碰撞名就是凭据形状的名字。
            stored_fixture("view-2", "password=hunter2", &contract_scope(), false),
        ])
        .await
        .expect("seed two views");

    let failure = binding
        .save(draft_fixture("users", &contract_scope()))
        .await
        .expect_err("the case-insensitive same name in one scope is refused");
    assert_eq!(
        failure.code.as_deref(),
        Some("grid.view.nameExists"),
        "the application contract code travels both legs"
    );
    assert_eq!(
        failure.message,
        "grid.view.nameExists: a saved view with this name already exists for this table",
        "the application sentence travels verbatim"
    );
    assert!(
        !failure.message.contains("users"),
        "the message never quotes the user's name"
    );

    let failure = binding
        .save(draft_fixture("PASSWORD=HUNTER2", &contract_scope()))
        .await
        .expect_err("a credential-shaped name collides case-insensitively too");
    assert_eq!(
        failure.code.as_deref(),
        Some("grid.view.nameExists"),
        "the application contract code travels both legs"
    );
    assert!(
        !failure.message.contains("hunter2") && !failure.message.contains("password="),
        "a credential-shaped name travels in no shape"
    );

    let elsewhere = binding
        .save(draft_fixture("Users", &other_scope()))
        .await
        .expect("another scope is a different table");
    assert!(
        !elsewhere.id.trim().is_empty(),
        "the same name in another scope saves fine"
    );
}

/// 错误路径（校验拒绝族）：不满足共享规则的记录在**落库前**被拒；拒绝
/// 携带 `grid.view.invalid` 与规则原因，存量经用例读回不受影响。
pub async fn invalid_records_are_refused_before_the_repository(
    binding: &dyn NamedViewsContractBinding,
) {
    binding
        .seed(vec![stored_fixture("view-1", "Users", &contract_scope(), false)])
        .await
        .expect("seed one view");

    let mut invalid = draft_fixture("   ", &contract_scope());
    invalid.id = "view-1".into();
    let failure = binding
        .save(invalid)
        .await
        .expect_err("an empty name is refused");
    assert_eq!(
        failure.code.as_deref(),
        Some("grid.view.invalid"),
        "the application contract code travels both legs"
    );
    assert!(
        failure.message.contains("the view name must not be empty"),
        "the rule reason travels: {}",
        failure.message
    );

    let list = binding.list(None).await.expect("list after the refusal");
    assert_eq!(list.len(), 1, "the refusal happened before the write");
    assert_eq!(list[0].name, "Users", "the stored record is untouched");
}

// ── 无宿主基线（判据本体：宿主整体换掉，规格原样过）───────────────────────

#[cfg(test)]
mod baseline {
    use super::*;
    use crate::named_views::NamedViewService;
    use std::sync::Arc;

    /// 无传输绑定：直调 `NamedViewService`。实现与本文件 scenario 用的
    /// 是同一端口、同一夹具——这就是「把宿主从 Tauri 换成 axum，测试
    /// 还能原样过」的极限形态（连宿主都没有）。
    struct DirectServiceBinding {
        service: NamedViewService,
        store: Arc<MemoryViewStore>,
    }

    #[async_trait]
    impl NamedViewsContractBinding for DirectServiceBinding {
        async fn seed(&self, views: Vec<NamedView>) -> Result<(), String> {
            self.store.seed(views).await;
            Ok(())
        }

        async fn list(&self, scope: Option<NamedViewScope>) -> ContractResult<Vec<NamedView>> {
            self.service
                .list(scope.as_ref())
                .await
                .map_err(application_failure)
        }

        async fn save(&self, view: NamedView) -> ContractResult<NamedView> {
            self.service.save(view).await.map_err(application_failure)
        }

        async fn delete(&self, id: String) -> ContractResult<Vec<NamedView>> {
            self.service.delete(&id).await.map_err(application_failure)
        }
    }

    fn harness() -> DirectServiceBinding {
        let store = Arc::new(MemoryViewStore::default());
        let service = NamedViewService::new(store.clone(), Arc::new(ContractIdMinter::default()));
        DirectServiceBinding { service, store }
    }

    #[tokio::test]
    async fn saves_a_new_view_and_returns_the_stored_record() {
        super::saves_a_new_view_and_returns_the_stored_record(&harness()).await;
    }

    #[tokio::test]
    async fn listing_orders_default_first_then_by_name() {
        super::listing_orders_default_first_then_by_name(&harness()).await;
    }

    #[tokio::test]
    async fn renaming_keeps_the_identity_and_the_creation_time() {
        super::renaming_keeps_the_identity_and_the_creation_time(&harness()).await;
    }

    #[tokio::test]
    async fn the_one_default_per_scope_rule_is_enforced_in_one_write() {
        super::the_one_default_per_scope_rule_is_enforced_in_one_write(&harness()).await;
    }

    #[tokio::test]
    async fn deleting_an_unknown_id_is_a_no_op_and_the_known_id_is_gone() {
        super::deleting_an_unknown_id_is_a_no_op_and_the_known_id_is_gone(&harness()).await;
    }

    #[tokio::test]
    async fn duplicate_names_in_one_scope_are_refused() {
        super::duplicate_names_in_one_scope_are_refused(&harness()).await;
    }

    #[tokio::test]
    async fn invalid_records_are_refused_before_the_repository() {
        super::invalid_records_are_refused_before_the_repository(&harness()).await;
    }
}
