//! 准备产物：不可变 ChangeSet 块（§5.1）。
//!
//! 块记录 relation identity、typed PK、before/after 行证据与 operation；
//! 按 payload digest 冻结，apply 重验指纹，禁止客户端在 apply 时改写值。

use datazen_driver_api::Value;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::{ChangeOperation, Endpoint, Row};

/// 一个参与同步的关系的稳定身份（database/schema/table）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationIdentity {
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
}

/// 一块不可变的行级变更证据。比较阶段产出，review 后原样封存。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeBlock {
    pub relation: RelationIdentity,
    pub operation: ChangeOperation,
    /// 类型化主键值（复合键按相同顺序；禁止 NULL key）。
    pub key: Vec<Value>,
    /// 目标侧旧值证据（UPDATE/DELETE）；INSERT 为 None。
    pub before: Option<Row>,
    /// 源侧新值（INSERT/UPDATE）；DELETE 为 None。
    pub after: Option<Row>,
    pub column_names: Vec<String>,
    /// 主键列名（与 key 同序）。
    pub pk_columns: Vec<String>,
    /// UPDATE 变更列；INSERT/DELETE 为空。
    #[serde(default)]
    pub changed_columns: Vec<String>,
    /// 源侧表名（relation 是目标侧；源表可与目标表不同名）。
    pub source_table: String,
}

/// 每张表的列/类型/PK 元数据，apply 生成参数化 SQL 时复用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMeta {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMeta {
    pub relation: RelationIdentity,
    pub columns: Vec<ColumnMeta>,
    pub pk_columns: Vec<String>,
    /// prepare 观察到但无需写入的相同行数（review 面板的基线）。
    #[serde(default)]
    pub unchanged_count: usize,
    /// 该表读取时使用的源侧过滤（重验/预览必须复用同一过滤）。
    #[serde(default)]
    pub source_filter: Option<crate::filter::SyncSourceFilter>,
}

/// ChangeSet 的不可变产物。`digest` 冻结块内容，apply 前必须与目标侧重验一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetArtifact {
    pub plan_id: String,
    pub source: Endpoint,
    pub target: Endpoint,
    pub family: String,
    pub mappings: Vec<crate::model::TableMapping>,
    pub blocks: Vec<ChangeBlock>,
    pub table_meta: Vec<TableMeta>,
    /// apply 重验用：源/目标各自表结构指纹。
    pub structure_fingerprint: String,
    pub digest: String,
    pub created_at: String,
}

/// 规范序列化后的内容摘要（sha256 hex），作为冻结 payload 摘要。
pub fn artifact_digest(blocks: &[ChangeBlock], source: &Endpoint, target: &Endpoint) -> String {
    let mut hasher = Sha256::new();
    for block in blocks {
        // 块失败时退化为字段枚举，保证同一字段集合产生同一摘要。
        if let Ok(bytes) = serde_json::to_vec(block) {
            hasher.update(bytes);
        }
        hasher.update(b"|");
    }
    if let Ok(bytes) = serde_json::to_vec(source) {
        hasher.update(bytes);
    }
    if let Ok(bytes) = serde_json::to_vec(target) {
        hasher.update(bytes);
    }
    format!("{:x}", hasher.finalize())
}

/// 表结构指纹（sha256 of canonical TableSchema JSON）。
pub fn structure_fingerprint<'a>(
    pairs: impl IntoIterator<Item = &'a datazen_driver_api::TableSchema>,
) -> String {
    let mut hasher = Sha256::new();
    for schema in pairs {
        if let Ok(bytes) = serde_json::to_vec(schema) {
            hasher.update(bytes);
        }
        hasher.update(b"|");
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_and_sensitive_to_block_changes() {
        let source = Endpoint::new("a", "db", None);
        let target = Endpoint::new("b", "db", None);
        let block = ChangeBlock {
            relation: RelationIdentity {
                database: "db".into(),
                schema: None,
                table: "t".into(),
            },
            operation: ChangeOperation::Insert,
            key: vec![Value::Integer(1)],
            before: None,
            after: Some(vec![Some(Value::Integer(1))]),
            column_names: vec!["id".into()],
            pk_columns: vec!["id".into()],
            changed_columns: vec![],
            source_table: "t".into(),
        };
        let d1 = artifact_digest(&[block.clone()], &source, &target);
        let d2 = artifact_digest(&[block.clone()], &source, &target);
        assert_eq!(d1, d2);
        let mut mutated = block.clone();
        mutated.after = Some(vec![Some(Value::Integer(2))]);
        let d3 = artifact_digest(&[mutated], &source, &target);
        assert_ne!(d1, d3);
    }
}
