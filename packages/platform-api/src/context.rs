//! 请求上下文与归属。
//!
//! 概要 §6.1 定义 [`RequestContext`] 的六个字段；CM §3 INV-01 规定身份只能由 adapter 构造，
//! **任何请求体字段都不得覆盖身份**。因此本模块只提供构造器，不提供「从请求体填充」的入口，
//! 也没有 `Default`。

use serde::{Deserialize, Serialize};

use crate::id::{
    AuthenticationSessionId, ClientInstanceId, DelegationId, EditorSessionId, JobId,
    OrganizationId, PrincipalId, RequestId, Timestamp,
};

/// 单次调用的身份上下文。**只能由 adapter 构造**并向下传递。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestContext {
    pub organization_id: OrganizationId,
    pub principal_id: PrincipalId,
    /// 未认证上下文为 `None`；端口据此判断能否落到用户态资源。
    pub authentication_session_id: Option<AuthenticationSessionId>,
    /// 客户端实例。**不是授权依据**（概要 §6.1）：它只用于定位 owner 与事件流归属。
    pub client_instance_id: ClientInstanceId,
    /// 落进 `ApiError.requestId` 与事件，供前端上报对账。
    pub request_id: RequestId,
    /// 委托执行时非空；作用域与有效期校验见 CM-06 与 `PolicyService::verify_delegation`。
    pub delegation_id: Option<DelegationId>,
}

impl RequestContext {
    /// adapter 的唯一构造入口。参数顺序固定为概要 §6.1 的字段顺序，便于逐项核对。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        principal_id: PrincipalId,
        authentication_session_id: Option<AuthenticationSessionId>,
        client_instance_id: ClientInstanceId,
        request_id: RequestId,
        delegation_id: Option<DelegationId>,
    ) -> Self {
        Self {
            organization_id,
            principal_id,
            authentication_session_id,
            client_instance_id,
            request_id,
            delegation_id,
        }
    }

    /// 是否处于委托执行上下文。委托主体必须同时经 `PolicyService::verify_delegation` 校验。
    pub fn is_delegated(&self) -> bool {
        self.delegation_id.is_some()
    }
}

/// 归属意图。CM §4 明确：后端必须确认 editor 属于当前 client、job/block 属于已授权 Job，
/// **不能允许用户声称任意 job owner**；组织和用户绑定存于服务端记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OwnerRef {
    /// 编辑器页签。
    Editor {
        client_instance_id: ClientInstanceId,
        editor_session_id: EditorSessionId,
    },
    /// 任务的某阶段。
    Job {
        job_id: JobId,
        stage_id: crate::id::StageId,
    },
    /// 工作流块。
    WorkflowBlock {
        job_id: JobId,
        block_id: crate::id::BlockId,
    },
    /// 非编辑器的长驻客户端会话（MCP、server 端内部调用等）。
    ClientSession {
        client_instance_id: ClientInstanceId,
        purpose: String,
    },
}

/// 委托凭据引用。作用域与有效期的匹配由 `PolicyService::verify_delegation` 判定，
/// 任何一项不匹配都必须让请求失败（CM-06）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationRef {
    pub delegation_id: DelegationId,
    pub organization_id: OrganizationId,
    /// 被委托主体。
    pub principal_id: PrincipalId,
    pub client_instance_id: ClientInstanceId,
    /// 授权作用域标识；与请求动作的匹配由策略端口判定。
    pub scope: String,
    pub expires_at: Timestamp,
}

impl DelegationRef {
    /// 参数顺序固定为字段顺序，便于逐项核对概要 §6.1。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        delegation_id: DelegationId,
        organization_id: OrganizationId,
        principal_id: PrincipalId,
        client_instance_id: ClientInstanceId,
        scope: String,
        expires_at: Timestamp,
    ) -> Self {
        Self {
            delegation_id,
            organization_id,
            principal_id,
            client_instance_id,
            scope,
            expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            Some(AuthenticationSessionId::new("auth-1")),
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    #[test]
    fn request_context_round_trips_with_camel_case_keys() {
        let original = context();
        let value = serde_json::to_value(&original).expect("serialize");
        assert_eq!(
            value,
            json!({
                "organizationId": "org-1",
                "principalId": "user-1",
                "authenticationSessionId": "auth-1",
                "clientInstanceId": "client-1",
                "requestId": "req-1",
                "delegationId": null,
            })
        );
        let back: RequestContext = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, original);
        assert!(!back.is_delegated());
    }

    #[test]
    fn unauthenticated_context_carries_none_session() {
        let mut ctx = context();
        ctx.authentication_session_id = None;
        ctx.delegation_id = Some(DelegationId::new("del-1"));
        let value = serde_json::to_value(&ctx).expect("serialize");
        assert_eq!(value["authenticationSessionId"], json!(null));
        let back: RequestContext = serde_json::from_value(value).expect("deserialize");
        assert!(back.is_delegated());
        assert_eq!(back.authentication_session_id, None);
    }

    #[test]
    fn owner_ref_variants_use_the_spec_tag_names() {
        let owners = [
            OwnerRef::Editor {
                client_instance_id: ClientInstanceId::new("c"),
                editor_session_id: EditorSessionId::new("e"),
            },
            OwnerRef::Job {
                job_id: JobId::new("j"),
                stage_id: crate::id::StageId::new("s"),
            },
            OwnerRef::WorkflowBlock {
                job_id: JobId::new("j"),
                block_id: crate::id::BlockId::new("b"),
            },
            OwnerRef::ClientSession {
                client_instance_id: ClientInstanceId::new("c"),
                purpose: "mcp".into(),
            },
        ];
        let kinds: Vec<String> = owners
            .iter()
            .map(|o| {
                serde_json::to_value(o).expect("serialize")["kind"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        assert_eq!(kinds, ["editor", "job", "workflowBlock", "clientSession"]);

        for owner in owners {
            let value = serde_json::to_value(&owner).expect("serialize");
            let back: OwnerRef = serde_json::from_value(value).expect("deserialize");
            assert_eq!(back, owner);
        }
    }

    #[test]
    fn editor_owner_round_trips_camel_case_fields() {
        let owner = OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("c-1"),
            editor_session_id: EditorSessionId::new("e-1"),
        };
        let value = serde_json::to_value(&owner).expect("serialize");
        assert_eq!(value["clientInstanceId"], json!("c-1"));
        assert_eq!(value["editorSessionId"], json!("e-1"));
    }
}
