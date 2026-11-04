//! 单租户收窄的已评审计划存储：包内 [`InMemoryReviewedPlanRepository`] 之上的策略层。
//!
//! B2 schema_diff 域刀（commands-layer-review §3.5「下沉核对」）：桌面
//! `commands/schema_diff/reviewed_repository.rs` 的 `DesktopReviewedPlanRepository`
//! 逐字下沉（改名去掉桌面前缀）。收窄逻辑形态中立——任何单租户宿主（桌面或
//! 未来只服务一个组织的 Web 形态）都复用同一份；宿主只保留**装配**（从自己的
//! 身份源取组织 id、身份不可用时落到 [`SingleTenantReviewedPlanRepository::deny_all`]）。
//!
//! ## 为什么单租户收窄不是「多余的」
//!
//! 存储本体（[`InMemoryReviewedPlanRepository`]）按组织分桶，越组织**读取**已经
//! 按「不存在」处理；单租户收窄再叠加两条：
//!
//! * **越组织写入直接丢弃**：不落进别的组织桶（写方自己也读不回来），
//!   单租户宿主的存储里不允许出现「不是我的组织」的计划；
//! * **身份不可用时deny-all**：不编造组织身份，一切访问按「不存在」处理。
//!
//! ## 时间基准与失效关闭
//!
//! 冻结时刻的时间基准、时钟回拨/前跳的失效关闭语义见 [`super::reviewed`] 模块
//! 文档；本层不引入新的时间判定。

use async_trait::async_trait;

use super::reviewed::{
    InMemoryReviewedPlanRepository, ReviewedPlan, ReviewedPlanRepository,
    ReviewedPlanRepositoryError,
};
use crate::shared::id::OrganizationId;
use crate::shared::scope::OrganizationScope;

/// 单租户形态的已评审计划存储：进程内存实现 + **只承认一个组织**。
///
/// 存储本体是包提供的 [`InMemoryReviewedPlanRepository`]（条目按组织分桶）；
/// 本类型在其上叠加单租户策略：只承认构造时给定的那一个组织。
pub struct SingleTenantReviewedPlanRepository {
    /// 本宿主唯一的组织。`None` 表示身份不可用 ⇒ 不承认任何组织。
    organization_id: Option<OrganizationId>,
    inner: InMemoryReviewedPlanRepository,
}

impl SingleTenantReviewedPlanRepository {
    /// 只承认给定的这一个组织。
    pub fn new(organization_id: OrganizationId) -> Self {
        Self {
            organization_id: Some(organization_id),
            inner: InMemoryReviewedPlanRepository::new(),
        }
    }

    /// 身份组装失败时的实例：不承认任何组织，一切访问按「不存在」处理。
    pub fn deny_all() -> Self {
        Self {
            organization_id: None,
            inner: InMemoryReviewedPlanRepository::new(),
        }
    }

    /// 本存储是否承认该作用域。
    fn acknowledges(&self, scope: &OrganizationScope) -> bool {
        self.organization_id.as_ref() == Some(scope.organization_id())
    }
}

#[async_trait]
impl ReviewedPlanRepository for SingleTenantReviewedPlanRepository {
    async fn put(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
        plan: ReviewedPlan,
    ) -> Result<(), ReviewedPlanRepositoryError> {
        if !self.acknowledges(scope) {
            // 越组织写入按「未发生」处理：不落数据，也不返回「无权限」。
            return Ok(());
        }
        self.inner.put(scope, plan_id, plan).await
    }

    async fn get(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<Option<ReviewedPlan>, ReviewedPlanRepositoryError> {
        if !self.acknowledges(scope) {
            return Ok(None);
        }
        self.inner.get(scope, plan_id).await
    }

    async fn remove(
        &self,
        scope: &OrganizationScope,
        plan_id: &str,
    ) -> Result<bool, ReviewedPlanRepositoryError> {
        if !self.acknowledges(scope) {
            return Ok(false);
        }
        self.inner.remove(scope, plan_id).await
    }
}

#[cfg(test)]
#[path = "reviewed_single_tenant_tests.rs"]
mod tests;
