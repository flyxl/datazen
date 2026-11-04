//! 资源 ACL 求值（team-server §6.1 求值顺序的纯函数半边）。
//!
//! [`evaluate`] 吃的是**快照**（角色 + ACL 行），不是连接——数据库读取
//! 与组织边界检查在 [`crate::policy::PolicyService::authorize`]，这里只
//! 做「任一步终止」的判定逻辑，便于把顺序逐条钉在测试上。
//!
//! 求值顺序（§6.1，任一步终止）：
//!
//! 1. **组织边界**：调用方在 DB 查询前已完成（跨组织资源根本不进本函数
//!    ——查询按请求组织限定，越组织连「存在」都不可见，CM-05）；
//! 2. **显式 deny**：deny 条目命中 (subject, action) → [`Decision::Denied`]；
//! 3. **ACL grant**：grant 覆盖该 action → [`Decision::Allowed`]；
//! 4. **角色基线**：[`Role::allows`] → [`Decision::Allowed`]；
//! 5. 否则 → [`Decision::Denied`]。
//!
//! 主体匹配：本刀只求值 `subject_type = user` 的条目（`subject_id` 即
//! principal）；`serviceAccount` / `delegated` 主体随 §6.4 委托身份切片
//! 进入求值（行已可落库，词表与 CHECK 已就位）。

use super::rbac::{Action, ResourceType, Role};

/// ACL 行快照（`acl_entries` 的求值投影）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclEntry {
    pub subject_type: String,
    pub subject_id: String,
    pub action: Action,
    pub deny: bool,
}

/// 授权判定的三值结果（§7.1）：HTTP 映射在 server host——
/// `NotVisible` → 404；`Denied`（id 寻址）→ 404；`Denied`（组织级动作）
/// → 403；`Allowed` → 放行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Denied,
    NotVisible,
}

/// 求值输入。`resource_id` 用空串表示**组织级**条目/动作（§6.1：主键列
/// 不能为 NULL；组织级动作的 `Denied` 在 host 映射 403 而非 404）。
#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub principal_id: &'a str,
    pub resource_type: ResourceType,
    pub resource_id: &'a str,
    pub action: Action,
}

/// §6.1 求值顺序的纯函数。`role` 是 `memberships` 读出的基线（`None` =
/// 无 membership 或已禁用——调用方已把这两种情形归入 `NotVisible`，
/// 这里再兜一层防御性判定）。
pub fn evaluate(role: Option<Role>, request: &Request<'_>, acl: &[AclEntry]) -> Decision {
    // 防御性兜底：无角色（或禁用 membership）的主体对任何资源都不可见
    // ——`PolicyService::authorize` 已先行判定，这里保证纯函数自身闭合。
    let Some(role) = role else {
        return Decision::NotVisible;
    };

    // 2. 显式 deny 优先于一切 grant（§6.1 第 2 步）。
    if acl.iter().any(|entry| {
        entry.deny && entry.matches(request.principal_id) && entry.action == request.action
    }) {
        return Decision::Denied;
    }
    // 3. 资源 ACL grant。
    if acl.iter().any(|entry| {
        !entry.deny && entry.matches(request.principal_id) && entry.action == request.action
    }) {
        return Decision::Allowed;
    }
    // 4. 角色基线。
    if role.allows(request.resource_type, request.action) {
        return Decision::Allowed;
    }
    // 5. 默认拒绝。
    Decision::Denied
}

impl AclEntry {
    /// 主体匹配：本刀只求值 `user` 主体（见模块头）。
    fn matches(&self, principal_id: &str) -> bool {
        self.subject_type == "user" && self.subject_id == principal_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(resource_id: &'static str, action: Action) -> Request<'static> {
        Request {
            principal_id: "user-1",
            resource_type: ResourceType::Connection,
            resource_id,
            action,
        }
    }

    fn grant(action: Action) -> AclEntry {
        AclEntry {
            subject_type: "user".into(),
            subject_id: "user-1".into(),
            action,
            deny: false,
        }
    }

    fn deny(action: Action) -> AclEntry {
        AclEntry {
            subject_type: "user".into(),
            subject_id: "user-1".into(),
            action,
            deny: true,
        }
    }

    /// 求值顺序钉子：deny > grant > 角色基线 > 默认拒绝（§6.1）。
    #[test]
    fn evaluation_order_is_deny_grant_baseline_default() {
        // 无 ACL：orgMember 对 connection 的 read 命中基线。
        assert_eq!(
            evaluate(Some(Role::OrgMember), &request("c-1", Action::Read), &[]),
            Decision::Allowed,
        );
        // 基线不放行 → 默认拒绝。
        assert_eq!(
            evaluate(Some(Role::OrgMember), &request("c-1", Action::Write), &[]),
            Decision::Denied,
        );
        // ACL grant 越过基线缺口。
        assert_eq!(
            evaluate(
                Some(Role::OrgMember),
                &request("c-1", Action::Write),
                &[grant(Action::Write)],
            ),
            Decision::Allowed,
        );
        // deny 压过基线（orgMember 本可 read）。
        assert_eq!(
            evaluate(
                Some(Role::OrgMember),
                &request("c-1", Action::Read),
                &[deny(Action::Read)],
            ),
            Decision::Denied,
        );
        // deny 压过 grant（同一 action 两条目并存时，deny 优先终止）。
        assert_eq!(
            evaluate(
                Some(Role::OrgAdmin),
                &request("c-1", Action::Write),
                &[grant(Action::Write), deny(Action::Write)],
            ),
            Decision::Denied,
        );
        // grant 不压 deny（顺序恒定）。
        assert_eq!(
            evaluate(
                Some(Role::OrgAdmin),
                &request("c-1", Action::Write),
                &[deny(Action::Write), grant(Action::Write)],
            ),
            Decision::Denied,
        );
    }

    #[test]
    fn entries_of_other_subjects_do_not_match() {
        let other_user = AclEntry {
            subject_type: "user".into(),
            subject_id: "user-2".into(),
            action: Action::Write,
            deny: false,
        };
        let service = AclEntry {
            subject_type: "serviceAccount".into(),
            subject_id: "user-1".into(),
            action: Action::Write,
            deny: false,
        };
        // 别人的 grant 不放行我；serviceAccount 主体随 §6.4 切片进入求值。
        assert_eq!(
            evaluate(
                Some(Role::OrgMember),
                &request("c-1", Action::Write),
                &[other_user, service],
            ),
            Decision::Denied,
        );
    }

    #[test]
    fn missing_role_is_not_visible() {
        assert_eq!(
            evaluate(None, &request("c-1", Action::Read), &[grant(Action::Read)]),
            Decision::NotVisible,
            "无 membership 的主体：连 ACL grant 也不可见（404 语义）"
        );
    }
}
