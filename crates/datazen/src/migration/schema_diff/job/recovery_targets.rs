//! 恢复目标的**纯**投影：目标 id 编解码、证据记录与表目标派生。
//!
//! B2 schema_diff 域刀（commands-layer-review §3.5）：桌面
//! `commands/schema_diff/job/recovery.rs` 中的形态中立纯函数逐字下沉。
//! 这些函数只做 `DatabaseObject` ↔ 线 id / 证据串的转换，不碰会话、驱动
//! 或宿主状态——会话绑定的 `prepare_target_objects` / `fingerprint_targets`
//! 留在宿主（它们先解析运行时会话再调用这些投影与 api 用例）。
//!
//! 线格式契约：
//!
//! * 目标 id：`"obj-"` + `DatabaseObject` JSON 的十六进制（≤512 字符）——
//!   落进 Job 的 `recoveryTargets.objectIds`，是持久化契约，不可变。
//! * 证据串：`"recoveryObject:"` + 目标 id（可枚举）；`"recoveryIdentity:"`
//!   + 目标数组 JSON（进程内整读）；投影不完整时落
//!   `"recoveryProjectionIncomplete"` 哨兵。

use datazen_driver_api::{DatabaseObject, ObjectKind};

const TARGET_ID_PREFIX: &str = "obj-";
const TARGET_EVIDENCE_PREFIX: &str = "recoveryObject:";
const TARGET_JSON_PREFIX: &str = "recoveryIdentity:";

/// 把恢复目标编码为可持久化的线 id（`obj-` + JSON 十六进制）。
///
/// 序列化失败或超长（>512）返回 `None`——调用方据此落「投影不完整」哨兵。
pub fn encode_target_id(target: &DatabaseObject) -> Option<String> {
    let json = serde_json::to_vec(target).ok()?;
    let mut id = String::with_capacity(TARGET_ID_PREFIX.len() + json.len() * 2);
    id.push_str(TARGET_ID_PREFIX);
    for byte in json {
        use std::fmt::Write;
        write!(&mut id, "{byte:02x}").ok()?;
    }
    (id.len() <= 512).then_some(id)
}

/// 解码 [`encode_target_id`] 产出的线 id；任何非该格式的输入返回 `None`。
pub fn decode_target_id(id: &str) -> Option<DatabaseObject> {
    let encoded = id.strip_prefix(TARGET_ID_PREFIX)?;
    if encoded.is_empty()
        || encoded.len() % 2 != 0
        || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    let bytes = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digits = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(digits, 16).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    serde_json::from_slice(&bytes).ok()
}

/// 从证据串里枚举全部目标线 id（`recoveryObject:` 前缀）。
pub fn target_ids_from_evidence(evidence: &[String]) -> Vec<String> {
    evidence
        .iter()
        .filter_map(|item| item.strip_prefix(TARGET_EVIDENCE_PREFIX).map(str::to_owned))
        .collect()
}

/// 从证据串里整读目标数组（`recoveryIdentity:` 前缀的 JSON）。
pub fn targets_from_evidence(evidence: &[String]) -> Option<Vec<DatabaseObject>> {
    let raw = evidence
        .iter()
        .find_map(|item| item.strip_prefix(TARGET_JSON_PREFIX))?;
    serde_json::from_str(raw).ok()
}

/// 把恢复目标写进证据串：一条可整读的 JSON + 每目标一条可枚举的线 id。
///
/// 任一目标编码失败时补一条 `"recoveryProjectionIncomplete"` 哨兵——恢复
/// 验证据此判「目标身份不可用」，而不是静默丢目标。
pub fn record_target_objects(evidence: &mut Vec<String>, targets: &[DatabaseObject]) {
    if let Ok(json) = serde_json::to_string(targets) {
        evidence.push(format!("{TARGET_JSON_PREFIX}{json}"));
    }
    let encoded = targets
        .iter()
        .map(encode_target_id)
        .collect::<Option<Vec<_>>>();
    if let Some(ids) = encoded {
        evidence.extend(
            ids.into_iter()
                .map(|id| format!("{TARGET_EVIDENCE_PREFIX}{id}")),
        );
    } else {
        evidence.push("recoveryProjectionIncomplete".into());
    }
}

/// 由表名派生恢复目标：限定名取最后一段为关系名（`public.users` → `users`）。
///
/// 与 schema diff 计划里的目标侧解析同构——DROP 的对象身份按目标侧的
/// 关系名记账。
pub fn table_target(table: &str, schema: Option<&str>) -> DatabaseObject {
    let relation = table
        .trim()
        .rsplit_once('.')
        .map(|(_, name)| name)
        .unwrap_or_else(|| table.trim());
    DatabaseObject {
        kind: ObjectKind::Table.as_str().into(),
        schema: schema.map(str::to_owned),
        name: relation.to_owned(),
        signature: None,
        target_schema: None,
        target_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(schema: Option<&str>, name: &str) -> DatabaseObject {
        DatabaseObject {
            kind: ObjectKind::View.as_str().into(),
            schema: schema.map(str::to_owned),
            name: name.into(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    #[test]
    fn target_id_roundtrips_through_the_wire_form() {
        let target = view(Some("public"), "users");
        let id = encode_target_id(&target).expect("id encodes");
        assert!(id.starts_with("obj-"), "{id}");
        assert!(id.len() <= 512);
        assert_eq!(decode_target_id(&id).as_ref(), Some(&target));
    }

    #[test]
    fn target_id_rejects_foreign_and_malformed_input() {
        assert!(decode_target_id("not-an-id").is_none());
        assert!(decode_target_id("obj-").is_none());
        assert!(decode_target_id("obj-zz").is_none());
        assert!(decode_target_id("obj-0011zz").is_none());
        // 合法十六进制但不是 JSON：同样读不出目标。
        assert!(decode_target_id("obj-deadbeef").is_none());
    }

    #[test]
    fn evidence_records_both_the_enumerable_and_readable_forms() {
        let targets = vec![
            table_target("public.users", Some("public")),
            view(None, "v"),
        ];
        let mut evidence = vec!["planTables:users".into()];
        record_target_objects(&mut evidence, &targets);

        assert_eq!(target_ids_from_evidence(&evidence).len(), 2);
        assert_eq!(targets_from_evidence(&evidence).as_ref(), Some(&targets));
        // 既有证据（planTables）不受影响。
        assert!(evidence[0].starts_with("planTables:"));
    }

    #[test]
    fn table_target_strips_the_qualifier_to_the_relation_name() {
        let target = table_target("public.users", Some("public"));
        assert_eq!(target.name, "users");
        assert_eq!(target.kind, ObjectKind::Table.as_str());
        assert_eq!(target.schema.as_deref(), Some("public"));

        let bare = table_target(" users ", None);
        assert_eq!(bare.name, "users");
        assert!(bare.schema.is_none());
    }
}
