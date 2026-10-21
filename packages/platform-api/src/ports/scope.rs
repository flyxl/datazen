//! `OrganizationScope`：按组织限定存储访问的作用域。
//!
//! 存储端口的方法若不带作用域，多租户宿主就无法在**同一个存储实现**上区分
//! 「本组织的数据」与「别的组织的数据」——只能退化成「每个组织一个存储实例」，
//! 把租户边界藏进实例数量里，而不是写进契约里。
//!
//! 规则（承自 §4.1 的端口约定）：
//!
//! * 作用域**只能由 adapter 构造**。身份来自 [`crate::context::RequestContext`]，
//!   而请求上下文只能由 adapter 构造（INV-01）——任何请求体字段都不得覆盖组织身份。
//! * **越组织访问一律按「不存在」处理**（返回 `None` / 空列表），不返回「无权限」
//!   ——后者会泄漏「存在但无权」（CM-05）。
//! * 作用域**只表达组织维度**。连接档案的可见性是另一回事，见
//!   [`crate::ports::profile::ProfileScope`]（它限定的是某组织的连接档案）。
//!
//! 取用方式：
//!
//! ```
//! use datazen_platform_api::ports::scope::OrganizationScope;
//! use datazen_platform_api::context::RequestContext;
//!
//! fn scoped(ctx: &RequestContext) -> OrganizationScope {
//!     OrganizationScope::from_context(ctx)
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::context::RequestContext;
use crate::id::OrganizationId;

/// 以组织限定的存储作用域。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationScope {
    pub organization_id: OrganizationId,
}

impl OrganizationScope {
    /// 直接以组织 id 构造作用域。优先用 [`Self::from_context`]，除非调用方
    /// 已经持有可信的组织身份。
    pub fn new(organization_id: OrganizationId) -> Self {
        Self { organization_id }
    }

    /// 从请求上下文取作用域。
    ///
    /// 身份只能来自 adapter 构造的上下文（INV-01），因此本方法**不接受**任何
    /// 请求体字段作为入参。
    pub fn from_context(ctx: &RequestContext) -> Self {
        Self::new(ctx.organization_id.clone())
    }

    /// 本作用域的组织 id。
    pub fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{ClientInstanceId, PrincipalId, RequestId};

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    #[test]
    fn scope_is_taken_from_the_adapter_built_context() {
        let ctx = context();
        let scope = OrganizationScope::from_context(&ctx);
        assert_eq!(scope.organization_id(), &ctx.organization_id);
    }

    #[test]
    fn two_organizations_get_different_scopes() {
        let a = OrganizationScope::new(OrganizationId::new("org-a"));
        let b = OrganizationScope::new(OrganizationId::new("org-b"));
        assert_ne!(a, b);
    }

    #[test]
    fn scope_carries_no_principal_or_credential() {
        let scope = OrganizationScope::from_context(&context());
        let debug = format!("{scope:?}");
        assert!(!debug.to_ascii_lowercase().contains("password"));
        assert!(!debug.to_ascii_lowercase().contains("principal"));
    }
}
