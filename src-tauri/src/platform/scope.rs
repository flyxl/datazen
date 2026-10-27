//! 桌面形态的组织作用域。
//!
//! 桌面是**单租户**：一份 app data 就是一个组织（见
//! [`super::identity::LOCAL_ORGANIZATION_ID`]）。因此桌面的存储实现只承认
//! **自己那一个组织**：别的组织的作用域一律按「不存在」处理（空列表 / NotFound），
//! **绝不**返回「无权限」类错误——那会泄漏「存在但无权」（CM-05）。
//!
//! 作用域里的组织 id 与 [`super::identity::DesktopIdentity::organization_id`] 一致：
//! 桌面形态唯一的生产构造入口 [`super::identity::DesktopIdentity::from_desktop_user`]
//! 把组织钉在 [`super::identity::LOCAL_ORGANIZATION_ID`]。

use datazen_modules::shared::id::OrganizationId;
use datazen_modules::shared::scope::OrganizationScope;

use super::identity::LOCAL_ORGANIZATION_ID;

/// 桌面形态自己的组织作用域。
///
/// 用于**没有请求上下文**的桌面内路径（后台 monitor、MCP、调度器）：那些路径
/// 处理的本来就是桌面本地数据，用的就是桌面自己这一个组织。
pub fn local_organization_scope() -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new(LOCAL_ORGANIZATION_ID))
}

/// 作用域是否属于桌面形态自己的组织。
///
/// 桌面存储实现用它把「本组织的数据」与「别的组织的数据」分开。
pub fn is_local_scope(scope: &OrganizationScope) -> bool {
    scope.organization_id().as_str() == LOCAL_ORGANIZATION_ID
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
