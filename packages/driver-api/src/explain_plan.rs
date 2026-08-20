use crate::{ExplainPlanDetail, ExplainPlanNode};
use serde_json::Value;

const MYSQL_OPERATION_KEYS: &[&str] = &[
    "nested_loop",
    "grouping_operation",
    "ordering_operation",
    "duplicating_weedout",
    "materialized_from_subquery",
    "buffer_result",
    "hash_join",
];

fn is_record(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    value.as_object()
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "NULL".into(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn parse_numeric(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn parse_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn parse_postgres_node(node: &serde_json::Map<String, Value>, id: &str) -> ExplainPlanNode {
    let node_type = node
        .get("Node Type")
        .and_then(|v| v.as_str())
        .unwrap_or("Plan");

    let suffix = ["Relation Name", "Index Name", "Alias"]
        .iter()
        .find_map(|key| node.get(*key).and_then(|v| v.as_str()));

    let label = match suffix {
        Some(name) => format!("{node_type} · {name}"),
        None => node_type.to_string(),
    };

    let details = node
        .iter()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "Plans"
                    | "Node Type"
                    | "Total Cost"
                    | "Plan Rows"
                    | "Startup Cost"
                    | "Relation Name"
                    | "Index Name"
                    | "Alias"
            )
        })
        .map(|(key, value)| ExplainPlanDetail {
            key: key.clone(),
            value: format_value(value),
        })
        .collect();

    let children = node
        .get("Plans")
        .and_then(|v| v.as_array())
        .map(|plans| {
            plans
                .iter()
                .enumerate()
                .filter_map(|(index, child)| {
                    is_record(child)
                        .map(|child_obj| parse_postgres_node(child_obj, &format!("{id}.{index}")))
                })
                .collect()
        })
        .unwrap_or_default();

    ExplainPlanNode {
        id: id.to_string(),
        label,
        cost: node.get("Total Cost").and_then(parse_numeric),
        rows: node.get("Plan Rows").and_then(parse_i64),
        details,
        children,
    }
}

/// Normalize PostgreSQL `EXPLAIN (FORMAT JSON)` output into a driver-neutral plan tree.
pub fn normalize_postgres_explain_plan(plan_json: &Value) -> Option<ExplainPlanNode> {
    let root = plan_json.as_array()?.first()?;
    let plan = is_record(root)?.get("Plan")?;
    let plan_obj = is_record(plan)?;
    Some(parse_postgres_node(plan_obj, "pg"))
}

fn mysql_table_label(table: &serde_json::Map<String, Value>) -> String {
    let table_name = table
        .get("table_name")
        .and_then(|v| v.as_str())
        .unwrap_or("table");
    let access_type = table
        .get("access_type")
        .and_then(|v| v.as_str())
        .unwrap_or("access");
    format!("{access_type} → {table_name}")
}

fn mysql_table_details(table: &serde_json::Map<String, Value>) -> Vec<ExplainPlanDetail> {
    table
        .iter()
        .filter(|(key, _)| key.as_str() != "table_name" && key.as_str() != "access_type")
        .map(|(key, value)| ExplainPlanDetail {
            key: key.clone(),
            value: format_value(value),
        })
        .collect()
}

fn parse_mysql_block(block: &serde_json::Map<String, Value>, id: &str) -> Vec<ExplainPlanNode> {
    let mut nodes = Vec::new();

    if let Some(table) = block.get("table").and_then(is_record) {
        let cost = block
            .get("cost_info")
            .and_then(is_record)
            .and_then(|info| info.get("query_cost"))
            .and_then(parse_numeric);
        nodes.push(ExplainPlanNode {
            id: format!("{id}.table"),
            label: mysql_table_label(table),
            cost,
            rows: table.get("rows_examined_per_scan").and_then(parse_i64),
            details: mysql_table_details(table),
            children: Vec::new(),
        });
    }

    for key in MYSQL_OPERATION_KEYS {
        let Some(value) = block.get(*key) else {
            continue;
        };

        if *key == "nested_loop" {
            if let Some(items) = value.as_array() {
                for (index, item) in items.iter().enumerate() {
                    let Some(item_obj) = is_record(item) else {
                        continue;
                    };
                    let children = parse_mysql_block(item_obj, &format!("{id}.nl.{index}"));
                    match children.len() {
                        0 => {}
                        1 => nodes.push(children.into_iter().next().unwrap()),
                        _ => nodes.push(ExplainPlanNode {
                            id: format!("{id}.nl.{index}"),
                            label: "nested loop".into(),
                            cost: None,
                            rows: None,
                            details: Vec::new(),
                            children,
                        }),
                    }
                }
            }
            continue;
        }

        if let Some(value_obj) = is_record(value) {
            let children = parse_mysql_block(value_obj, &format!("{id}.{key}"));
            nodes.push(ExplainPlanNode {
                id: format!("{id}.{key}"),
                label: key.replace('_', " "),
                cost: value_obj
                    .get("cost_info")
                    .and_then(is_record)
                    .and_then(|info| info.get("query_cost"))
                    .and_then(parse_numeric),
                rows: None,
                details: Vec::new(),
                children,
            });
        }
    }

    nodes
}

/// Normalize MySQL `EXPLAIN FORMAT=JSON` output into a driver-neutral plan tree.
pub fn normalize_mysql_explain_plan(plan_json: &Value) -> Option<ExplainPlanNode> {
    let query_block = is_record(plan_json)?.get("query_block")?;
    let query_block_obj = is_record(query_block)?;
    let children = parse_mysql_block(query_block_obj, "mysql");
    if children.is_empty() {
        return None;
    }
    if children.len() == 1 {
        return Some(children.into_iter().next().unwrap());
    }
    Some(ExplainPlanNode {
        id: "mysql.root".into(),
        label: "query block".into(),
        cost: query_block_obj
            .get("cost_info")
            .and_then(is_record)
            .and_then(|info| info.get("query_cost"))
            .and_then(parse_numeric),
        rows: None,
        details: Vec::new(),
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_postgres_explain_plan_parses_nested_nodes() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "users",
                "Total Cost": 12.5,
                "Plan Rows": 100,
                "Filter": "id > 1",
                "Plans": [{
                    "Node Type": "Index Scan",
                    "Index Name": "users_pkey",
                    "Total Cost": 0.5
                }]
            }
        }]);

        let root = normalize_postgres_explain_plan(&plan).expect("plan tree");
        assert_eq!(root.label, "Seq Scan · users");
        assert_eq!(root.cost, Some(12.5));
        assert_eq!(root.rows, Some(100));
        assert_eq!(root.details.len(), 1);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].label, "Index Scan · users_pkey");
    }

    #[test]
    fn normalize_mysql_explain_plan_parses_query_block() {
        let plan = json!({
            "query_block": {
                "cost_info": { "query_cost": 3.2 },
                "table": {
                    "table_name": "users",
                    "access_type": "ALL",
                    "rows_examined_per_scan": 50
                }
            }
        });

        let root = normalize_mysql_explain_plan(&plan).expect("plan tree");
        assert_eq!(root.label, "ALL → users");
        assert_eq!(root.cost, Some(3.2));
        assert_eq!(root.rows, Some(50));
    }
}
