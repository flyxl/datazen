//! Workflow 定义仓储端口与登记规则（trait 归 domain，实现在宿主——§4.1 第 5 条）。
//!
//! 词源：feature-core::workflow::registry 的端口半（D6 item 3 切片 b 拆出）。
//! 仓储 trait 归领域层，宿主（桌面 SQLite）提供实现；编码/解码/列表投影是
//! 纯规则，与词汇同层。

use super::model::{WorkflowDefinition, WorkflowListItem, WorkflowVisibility};
use crate::shared::scope::OrganizationScope;

/// A saved workflow as the registry stores it.
///
/// The definition itself travels as YAML so a saved workflow stays the same
/// document the user wrote in the editor.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinitionRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub visibility: WorkflowVisibility,
    pub definition_yaml: String,
    pub created_at: String,
    pub updated_at: String,
}

/// The persistence a workflow registry needs from the environment it runs in.
///
/// The host implements this over its own store (the desktop host binds it to
/// the local SQLite database). Everything the registry *decides* — which
/// workflows a user sees, how a definition is encoded, what a saved workflow
/// looks like — stays in this crate. The registry holds the store as a trait
/// object, so the host's implementation type stays out of the shared
/// signatures and the host picks the implementation at assembly time.
pub trait WorkflowDefinitionRepository: Send + Sync {
    /// One saved workflow of the organization, or `None` when the id is unknown.
    fn get_workflow(
        &self,
        scope: &OrganizationScope,
        id: &str,
    ) -> Result<Option<WorkflowDefinitionRecord>, String>;

    /// Every saved workflow of the organization with the given visibility.
    fn list_workflows(
        &self,
        scope: &OrganizationScope,
        visibility: WorkflowVisibility,
    ) -> Result<Vec<WorkflowDefinitionRecord>, String>;

    /// Insert or update one saved workflow.
    fn upsert_workflow(
        &self,
        scope: &OrganizationScope,
        record: &WorkflowDefinitionRecord,
    ) -> Result<(), String>;

    /// Delete one saved workflow. Deleting an unknown id is not an error.
    ///
    /// **跨组织删除与删除一个不存在的 id 一样返回 `Ok(())`**：在本契约下二者
    /// 不可区分——越组织一律按「不存在」处理（CM-05），而「删除不存在的 id 不是
    /// 错误」是既有约定。因此调用方**不应**把这个 `Ok(())` 读作「目标已删除」，
    /// 它只说明「没有需要删除的东西」。
    fn delete_workflow(&self, scope: &OrganizationScope, id: &str) -> Result<(), String>;
}

/// A workflow id may only contain characters that survive a URL, a file name
/// and a SQL parameter without quoting.
pub fn validate_workflow_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Invalid workflow id: {id}. Only alphanumeric, dash, and underscore are allowed."
        ));
    }
    Ok(())
}

/// Encode a definition as the YAML the registry stores.
pub fn encode_definition(definition: &WorkflowDefinition) -> Result<String, String> {
    serde_yaml::to_string(definition).map_err(|error| error.to_string())
}

/// Decode a definition from the YAML a user wrote.
pub fn decode_definition(yaml: &str) -> Result<WorkflowDefinition, String> {
    serde_yaml::from_str(yaml).map_err(|error| format!("Invalid workflow YAML: {error}"))
}

/// The list projection of one definition: what the workflow list shows.
pub fn list_item_for(definition: &WorkflowDefinition) -> WorkflowListItem {
    WorkflowListItem {
        id: definition.id.clone(),
        name: definition.name.clone(),
        description: definition.description.clone(),
        variables: definition.variables.clone(),
        scheduled: definition
            .schedule
            .as_ref()
            .map(|schedule| schedule.enabled)
            .unwrap_or(false),
    }
}
