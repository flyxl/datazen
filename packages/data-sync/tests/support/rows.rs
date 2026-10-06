//! 行 / 表结构 / 端点夹具：CM-43/44/45 共用的确定性数据形状。

use datazen_data_sync::job::RelationIdentity;
use datazen_data_sync::model::{Endpoint, Row};
use datazen_driver_api::{ColumnSchema, TableOptions, TableSchema, Value};

pub const TABLE: &str = "users";
pub const SOURCE_TABLE: &str = "users";
pub const FAMILY: &str = "postgres";
pub const PLAN_ID: &str = "plan-1";

pub fn int(n: i64) -> Value {
    Value::Integer(n)
}

pub fn text(s: &str) -> Value {
    Value::String(s.to_string())
}

/// 把一行标称值包成 `Row`（无 NULL）。
pub fn row(values: Vec<Value>) -> Row {
    values.into_iter().map(Some).collect()
}

/// `Value` 刻意未实现 `PartialEq`（含 Float/Bytes），因此行相等由测试自行定义：
/// 逐格比较 Debug 文本。夹具里没有凭据，Debug 可安全进断言消息。
pub fn rows_equal(left: &Option<Row>, right: &Option<Row>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(x, y)| cell_repr(x) == cell_repr(y))
        }
        _ => false,
    }
}

/// 行相等断言（失败时回显两侧的 Debug 形态）。
#[macro_export]
macro_rules! assert_row_eq {
    ($actual:expr, $expected:expr $(,)?) => {{
        let actual = $actual;
        let expected = $expected;
        assert!(
            $crate::rows_equal(&actual, &expected),
            "row mismatch:\n  actual:   {:?}\n  expected: {:?}",
            actual,
            expected
        );
    }};
}

fn cell_repr(cell: &Option<Value>) -> String {
    match cell {
        None => "NULL".to_string(),
        Some(v) => key_repr(std::slice::from_ref(v)),
    }
}

/// 主键的稳定文本表示：用于内存目标表做键寻址和断言。
pub fn key_repr(key: &[Value]) -> String {
    key.iter()
        .map(|v| match v {
            Value::Null => "null".to_string(),
            Value::Bool(b) => format!("bool:{b}"),
            Value::Integer(i) => format!("int:{i}"),
            Value::Float(f) => format!("float:{f}"),
            Value::String(s) => format!("text:{s}"),
            Value::Bytes(b) => format!("bytes:{}", b.len()),
            Value::Timestamp(t) => format!("ts:{t}"),
            Value::Json(j) => format!("json:{j}"),
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub fn relation(table: &str) -> RelationIdentity {
    RelationIdentity {
        database: "app".to_string(),
        schema: Some("public".to_string()),
        table: table.to_string(),
    }
}

pub fn endpoint(connection_id: &str) -> Endpoint {
    Endpoint::new(connection_id, "app", Some("public".to_string()))
}

/// 通过 `check_table_gate` 的最小两列表：`id integer NOT NULL PK` + `name text NULL`。
pub fn schema(table: &str) -> TableSchema {
    TableSchema {
        table_name: table.to_string(),
        columns: vec![
            ColumnSchema {
                name: "id".to_string(),
                data_type: "integer".to_string(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "name".to_string(),
                data_type: "text".to_string(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec!["id".to_string()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: TableOptions::default(),
    }
}

/// 目标侧多一列的结构漂移（用于验证 apply 的结构指纹复验）。
pub fn drifted_schema(table: &str) -> TableSchema {
    let mut s = schema(table);
    s.columns.push(ColumnSchema {
        name: "extra".to_string(),
        data_type: "text".to_string(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    });
    s
}
