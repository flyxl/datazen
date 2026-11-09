//! 授权词汇：动作与资源类型（web-server-host §6.1 的词表）。
//!
//! 权限是**逐 action** 判定，不做「全权限」通配（§6.1）；资源类型词表
//! 与 v5 迁移 `ck_acl_resource` 的 CHECK 逐字对齐——唯一登记差异是
//! `wapp`：web-server-host §6.1 把它列在资源类型里，persistence-model §3.10
//! 的 CHECK 没有（ACL 行不可落 wapp），因此 wapp 只出现在角色基线
//! （代码常量）与未来的安装闸上，ACL 表不承载它（v5 迁移文档同款登记）。

/// 逐 action 判定的权限词（§6.1 `ck_acl_action` CHECK 取值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Read,
    Execute,
    Write,
    Cancel,
    Download,
    Subscribe,
    Admin,
}

impl Action {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Execute => "execute",
            Self::Write => "write",
            Self::Cancel => "cancel",
            Self::Download => "download",
            Self::Subscribe => "subscribe",
            Self::Admin => "admin",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "read" => Self::Read,
            "execute" => Self::Execute,
            "write" => Self::Write,
            "cancel" => Self::Cancel,
            "download" => Self::Download,
            "subscribe" => Self::Subscribe,
            "admin" => Self::Admin,
            _ => return None,
        })
    }
}

/// 资源类型（§6.1 词表；ACL 表可落的六个见 [`Self::acl_allowed`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Connection,
    Session,
    Execution,
    Job,
    Artifact,
    Stream,
    /// 仅角色基线与安装闸使用（见模块头登记差异）；ACL 表不承载。
    Wapp,
}

impl ResourceType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::Session => "session",
            Self::Execution => "execution",
            Self::Job => "job",
            Self::Artifact => "artifact",
            Self::Stream => "stream",
            Self::Wapp => "wapp",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "connection" => Self::Connection,
            "session" => Self::Session,
            "execution" => Self::Execution,
            "job" => Self::Job,
            "artifact" => Self::Artifact,
            "stream" => Self::Stream,
            "wapp" => Self::Wapp,
            _ => return None,
        })
    }

    /// `acl_entries` 可承载的类型（v5 `ck_acl_resource` 的 CHECK 词表）。
    pub const fn acl_allowed(self) -> bool {
        !matches!(self, Self::Wapp)
    }
}

/// 固定内置角色（§6.1：首版不可由管理员自定义；`ck_membership_role`
/// 的 CHECK 词表，不建 `roles` 表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    OrgAdmin,
    OrgMember,
    ConnectionOwner,
    ConnectionViewer,
    JobOperator,
    Auditor,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OrgAdmin => "orgAdmin",
            Self::OrgMember => "orgMember",
            Self::ConnectionOwner => "connectionOwner",
            Self::ConnectionViewer => "connectionViewer",
            Self::JobOperator => "jobOperator",
            Self::Auditor => "auditor",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "orgAdmin" => Self::OrgAdmin,
            "orgMember" => Self::OrgMember,
            "connectionOwner" => Self::ConnectionOwner,
            "connectionViewer" => Self::ConnectionViewer,
            "jobOperator" => Self::JobOperator,
            "auditor" => Self::Auditor,
            _ => return None,
        })
    }

    /// 角色基线：该角色在资源类型上是否允许该 action（§6.1 求值顺序的
    /// 第 4 步；组织边界与 ACL 覆盖在 [`crate::policy::acl`] 求值）。
    ///
    /// 首版基线矩阵是**代码常量**（无 `roles` 表、无策略语言，§6.1）。
    /// 取值是对角色名的保守读取，裁决依据：
    ///
    /// - `orgAdmin`：组织内全权（含 `admin` 与 wapp 安装闸的 `write`，
    ///   web-server-host §15.3）。
    /// - `orgMember`：日常使用基线——可读、可执行/订阅/下载/取消，**不可
    ///   写、不可管理**（写与组织级变更属于 owner/admin 语义）。
    /// - `connectionOwner` / `connectionViewer`：连接的建立者与只读协作者
    ///   的基线（逐资源覆盖仍走 ACL）；会话与执行按 owner 继承（§6.1），
    ///   角色只给资源类型基线。
    /// - `jobOperator`：任务操作者——读/执行/取消 Job 与执行，不写配置。
    /// - `auditor`：全资源只读（审计口径）；无任何执行/写/取消权。
    ///
    /// 矩阵被 `rbac` 钉子测试**逐格**钉住；调整矩阵 = 改授权基线 = 改测试。
    pub const fn allows(self, resource: ResourceType, action: Action) -> bool {
        match self {
            Self::OrgAdmin => true,
            Self::OrgMember => match resource {
                ResourceType::Connection => matches!(action, Action::Read | Action::Execute),
                ResourceType::Session => matches!(action, Action::Read | Action::Execute),
                ResourceType::Execution => {
                    matches!(action, Action::Read | Action::Execute | Action::Cancel)
                }
                ResourceType::Job => {
                    matches!(action, Action::Read | Action::Cancel)
                }
                ResourceType::Artifact => matches!(action, Action::Read | Action::Download),
                ResourceType::Stream => matches!(action, Action::Read | Action::Subscribe),
                ResourceType::Wapp => matches!(action, Action::Read),
            },
            Self::ConnectionOwner => match resource {
                ResourceType::Connection => {
                    matches!(action, Action::Read | Action::Execute | Action::Write)
                }
                ResourceType::Session => matches!(
                    action,
                    Action::Read | Action::Execute | Action::Write | Action::Cancel
                ),
                ResourceType::Execution => {
                    matches!(action, Action::Read | Action::Execute | Action::Cancel)
                }
                ResourceType::Artifact => matches!(action, Action::Read | Action::Download),
                ResourceType::Stream => matches!(action, Action::Subscribe),
                ResourceType::Job | ResourceType::Wapp => false,
            },
            Self::ConnectionViewer => match resource {
                ResourceType::Connection => matches!(action, Action::Read),
                ResourceType::Artifact => matches!(action, Action::Read),
                ResourceType::Session
                | ResourceType::Execution
                | ResourceType::Job
                | ResourceType::Stream
                | ResourceType::Wapp => false,
            },
            Self::JobOperator => match resource {
                ResourceType::Job => {
                    matches!(action, Action::Read | Action::Execute | Action::Cancel)
                }
                ResourceType::Execution => {
                    matches!(action, Action::Read | Action::Execute | Action::Cancel)
                }
                ResourceType::Connection => matches!(action, Action::Read),
                ResourceType::Artifact => matches!(action, Action::Read | Action::Download),
                ResourceType::Stream => matches!(action, Action::Subscribe),
                ResourceType::Session | ResourceType::Wapp => false,
            },
            Self::Auditor => matches!(action, Action::Read),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_ACTIONS: [Action; 7] = [
        Action::Read,
        Action::Execute,
        Action::Write,
        Action::Cancel,
        Action::Download,
        Action::Subscribe,
        Action::Admin,
    ];

    const DB_RESOURCE_TYPES: [ResourceType; 6] = [
        ResourceType::Connection,
        ResourceType::Session,
        ResourceType::Execution,
        ResourceType::Job,
        ResourceType::Artifact,
        ResourceType::Stream,
    ];

    /// 基线矩阵逐格钉住（调整矩阵 = 改授权基线 = 改本测试）。
    #[test]
    fn the_baseline_matrix_is_pinned_cell_by_cell() {
        // orgAdmin：全格。
        for resource in DB_RESOURCE_TYPES {
            for action in ALL_ACTIONS {
                assert!(
                    Role::OrgAdmin.allows(resource, action),
                    "orgAdmin must allow {action:?} on {resource:?}"
                );
            }
        }
        // orgMember：日常使用基线。
        assert!(Role::OrgMember.allows(ResourceType::Connection, Action::Read));
        assert!(Role::OrgMember.allows(ResourceType::Connection, Action::Execute));
        assert!(!Role::OrgMember.allows(ResourceType::Connection, Action::Write));
        assert!(!Role::OrgMember.allows(ResourceType::Connection, Action::Admin));
        assert!(Role::OrgMember.allows(ResourceType::Job, Action::Read));
        assert!(Role::OrgMember.allows(ResourceType::Job, Action::Cancel));
        assert!(!Role::OrgMember.allows(ResourceType::Job, Action::Execute));
        assert!(Role::OrgMember.allows(ResourceType::Artifact, Action::Download));
        assert!(Role::OrgMember.allows(ResourceType::Stream, Action::Subscribe));
        assert!(!Role::OrgMember.allows(ResourceType::Wapp, Action::Write));
        // connectionOwner：写自己的连接配置，不触 admin。
        assert!(Role::ConnectionOwner.allows(ResourceType::Connection, Action::Write));
        assert!(!Role::ConnectionOwner.allows(ResourceType::Connection, Action::Admin));
        assert!(Role::ConnectionOwner.allows(ResourceType::Session, Action::Cancel));
        assert!(!Role::ConnectionOwner.allows(ResourceType::Job, Action::Read));
        // connectionViewer：只读。
        assert!(Role::ConnectionViewer.allows(ResourceType::Connection, Action::Read));
        assert!(!Role::ConnectionViewer.allows(ResourceType::Connection, Action::Execute));
        // jobOperator：任务三件套。
        assert!(Role::JobOperator.allows(ResourceType::Job, Action::Cancel));
        assert!(Role::JobOperator.allows(ResourceType::Execution, Action::Execute));
        assert!(!Role::JobOperator.allows(ResourceType::Connection, Action::Write));
        // auditor：全资源只读。
        for resource in DB_RESOURCE_TYPES {
            assert!(Role::Auditor.allows(resource, Action::Read));
            for action in ALL_ACTIONS {
                if !matches!(action, Action::Read) {
                    assert!(
                        !Role::Auditor.allows(resource, action),
                        "auditor must not {action:?} on {resource:?}"
                    );
                }
            }
        }
    }

    /// 词表与 v5 CHECK 逐字一致（漂移 = 授权行写不进/读不出）。
    #[test]
    fn vocabularies_match_the_v5_check_literals() {
        assert_eq!(Role::OrgAdmin.as_str(), "orgAdmin");
        assert_eq!(Role::ConnectionOwner.as_str(), "connectionOwner");
        assert_eq!(Role::JobOperator.as_str(), "jobOperator");
        assert_eq!(Action::Subscribe.as_str(), "subscribe");
        for action in ALL_ACTIONS {
            assert_eq!(Action::from_db(action.as_str()), Some(action));
        }
        for resource in DB_RESOURCE_TYPES {
            assert_eq!(ResourceType::from_db(resource.as_str()), Some(resource));
            assert!(resource.acl_allowed());
        }
        assert_eq!(ResourceType::from_db("wapp"), Some(ResourceType::Wapp));
        assert!(!ResourceType::Wapp.acl_allowed(), "ACL 表不承载 wapp");
        assert_eq!(Action::from_db("all"), None);
        assert_eq!(Role::from_db("superAdmin"), None);
    }
}
