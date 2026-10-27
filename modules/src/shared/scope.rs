//! `OrganizationScope`：按组织限定存储访问的作用域。
//!
//! 存储端口的方法若不带作用域，多租户宿主就无法在**同一个存储实现**上区分
//! 「本组织的数据」与「别的组织的数据」——只能退化成「每个组织一个存储实例」，
//! 把租户边界藏进实例数量里，而不是写进契约里。
//!
//! 规则（承自 §4.1 的端口约定）：
//!
//! * 作用域**只能由可信来源构造**：adapter 组装的请求上下文
//!   （[`OrganizationScope::from_context`]），或宿主形态的**安装级常量**——
//!   桌面是单租户，它的组织 id 是一个进程常量，与「当前 OS 用户名能不能取到」
//!   无关。两种来源都**不是**请求体字段：**任何请求体字段都不得覆盖组织身份**
//!   （INV-01）。
//! * **越组织访问一律按「不存在」处理**（返回 `None` / 空列表），不返回「无权限」
//!   ——后者会泄漏「存在但无权」（CM-05）。
//! * 作用域**只表达组织维度**。连接档案的可见性是另一回事，见
//!   [`crate::connection::domain::ProfileScope`]（它限定的是某组织的连接档案）。
//!
//! 取用方式（宿主也可经 platform-api 再导出消费，路径
//! `datazen::scope`；本 crate 是唯一定义，示例按
//! crate 内路径编译校验）：
//!
//! ```
//! use datazen::shared::context::RequestContext;
//! use datazen::shared::scope::OrganizationScope;
//!
//! fn scoped(ctx: &RequestContext) -> OrganizationScope {
//!     OrganizationScope::from_context(ctx)
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::shared::context::RequestContext;
use crate::shared::id::OrganizationId;

/// 以组织限定的存储作用域。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationScope {
    pub organization_id: OrganizationId,
}

impl OrganizationScope {
    /// 直接以组织 id 构造作用域。
    ///
    /// 只在调用方已经持有**可信的组织身份**时用：宿主形态的安装级常量
    /// （桌面单租户），或别的已解析好的组织身份。请求体字段**不得**流到这里
    /// （INV-01）——那等于让调用方指定组织，租户边界随之失效。
    /// 其余情况优先用 [`Self::from_context`]。
    pub fn new(organization_id: OrganizationId) -> Self {
        Self { organization_id }
    }

    /// 从请求上下文取作用域。
    ///
    /// 身份只能来自 adapter 构造的上下文（INV-01），因此本方法**不接受**任何
    /// 请求体字段作为入参。宿主形态若不经过 adapter（例如桌面用安装级常量
    /// 直接构造），见 [`Self::new`]。
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
    use crate::shared::id::{ClientInstanceId, PrincipalId, RequestId};

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
