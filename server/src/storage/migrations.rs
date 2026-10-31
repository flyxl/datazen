//! 管理库迁移表（expand/contract 纪律见 `super`）。
//!
//! 版本号只增；`expand` 是唯一被执行的 SQL。计划中的收缩步骤以
//! `contract` 注释记录，执行器拒绝跑它——等备份核验切片（WR-G3）落地。

/// 一个迁移单元。`contract` 是**计划**的收缩 SQL，当前执行器不执行。
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub expand: &'static str,
}

/// v1 基线：连接档案表（`ProfileRepository` 的服务端落地）。
///
/// 列语义与 `ProfileRecord` 一一对应：
/// - 复合主键 `(organization_id, connection_id)`：组织隔离在**存储层**就是
///   硬边界（CM-05），越组织的 id 查询天然落空。
/// - `secret_ref` 只存引用（`SecretRef` 字符串），明文永不入行——服务端
///   解析走 `SecretProvider`（WR-D4）。
/// - `initial_namespace_json` / `driver_options_json` 是投影列，读取时由
///   适配器反序列化回 domain 类型。
/// - `creation_key` 承载 `create` 的幂等（`{principalId}:{idempotencyKey}`，
///   与桌面 `platform_creation` 同款键形）；部分唯一索引让 NULL（非 create
///   写入的行）不参与唯一约束。
const V1_CONNECTION_PROFILE: Migration = Migration {
    version: 1,
    name: "connection_profile baseline",
    expand: r#"
CREATE TABLE connection_profile (
  organization_id TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  name TEXT NOT NULL,
  driver_id TEXT NOT NULL,
  config_revision INTEGER NOT NULL CHECK (config_revision > 0),
  credential_revision INTEGER NOT NULL CHECK (credential_revision > 0),
  network_route_revision INTEGER NOT NULL DEFAULT 1 CHECK (network_route_revision > 0),
  initial_namespace_json TEXT NOT NULL,
  driver_options_json TEXT NOT NULL DEFAULT '{}',
  secret_ref TEXT,
  network_route_ref TEXT,
  read_only INTEGER NOT NULL DEFAULT 0 CHECK (read_only IN (0, 1)),
  policy_ref TEXT,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  creation_key TEXT,
  PRIMARY KEY (organization_id, connection_id)
);
CREATE UNIQUE INDEX idx_connection_profile_creation_key
  ON connection_profile (organization_id, creation_key)
  WHERE creation_key IS NOT NULL;
CREATE INDEX idx_connection_profile_list
  ON connection_profile (organization_id, enabled, name);
"#,
};

/// 全部迁移，按版本升序。执行器假定有序且无空洞。
pub const MIGRATIONS: &[Migration] = &[V1_CONNECTION_PROFILE];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_ascending_without_gaps() {
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                migration.version,
                (index as i64) + 1,
                "migration versions must be 1..=n without gaps"
            );
            assert!(!migration.name.is_empty());
            assert!(!migration.expand.trim().is_empty());
        }
    }
}
