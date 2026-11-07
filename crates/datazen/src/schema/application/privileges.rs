//! Privilege catalog assembly for the `get_privileges` command.
//!
//! B2 schema 域刀（commands-layer-review §3.6）：`get_privileges` 的权限目录
//! 组装自桌面命令层（`commands/schema.rs`）逐字下沉。规则本身住在驱动侧
//! （`datazen_driver_api` 的 `list_privileges` 命令负责行级过滤），这里的职责
//! 只是把命令结果里的 `grants` 数组解析成 [`PrivilegeGrant`] 目录——字段键
//! `grants` 是驱动命令线契约，缺失或不可解析时按「无权限记录」处理（空目录
//! 是一个答案，不是一次失败）。

use datazen_driver_api::schema_objects::PrivilegeGrant;

/// Parse the privilege catalog out of a `list_privileges` command result.
///
/// The `grants` key is the driver-command wire contract. A missing key or a
/// malformed payload yields an empty catalog — the driver owns row-level
/// filtering (incomplete rows never make it into the array), so an empty
/// result means "this driver reports no grants", not "the call failed".
pub fn parse_grants_from_command(data: &serde_json::Value) -> Vec<PrivilegeGrant> {
    data.get("grants")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_grants_array() {
        let data = serde_json::json!({
            "grants": [
                {
                    "grantee": "alice",
                    "objectSchema": "public",
                    "objectName": "users",
                    "privilege": "SELECT"
                }
            ]
        });
        let grants = parse_grants_from_command(&data);
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].grantee, "alice");
        assert_eq!(grants[0].object_schema.as_deref(), Some("public"));
        assert_eq!(grants[0].object_name, "users");
        assert_eq!(grants[0].privilege, "SELECT");
    }

    #[test]
    fn missing_grants_key_is_an_empty_catalog() {
        let data = serde_json::json!({ "databases": ["app"] });
        assert!(parse_grants_from_command(&data).is_empty());
    }

    #[test]
    fn malformed_grants_payload_is_an_empty_catalog_not_a_failure() {
        let data = serde_json::json!({ "grants": "not-an-array" });
        assert!(parse_grants_from_command(&data).is_empty());
    }

    #[test]
    fn incomplete_rows_are_rejected_by_the_wire_shape() {
        // A row missing a required field (grantee / objectName / privilege)
        // cannot deserialize: the whole catalog reads as absent rather than
        // silently dropping the malformed entry — the driver is responsible
        // for never emitting such rows. (`objectSchema` is optional on the
        // wire — `PrivilegeGrant::object_schema` is `Option<String>` — so a
        // schema-less grant is a complete row, not an incomplete one.)
        let data = serde_json::json!({
            "grants": [
                { "objectName": "users", "privilege": "SELECT" }
            ]
        });
        assert!(parse_grants_from_command(&data).is_empty());
    }
}
