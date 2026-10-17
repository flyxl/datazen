use super::{context::WorkflowContext, model::MergeSource};
use datazen_driver_api::MultiQueryResult;
pub(super) fn resolve_json_templates(
    value: &serde_json::Value,
    context: &WorkflowContext,
) -> Result<serde_json::Value, String> {
    match value {
        serde_json::Value::String(s) => Ok(serde_json::Value::String(context.resolve_template(s)?)),
        serde_json::Value::Array(items) => Ok(serde_json::Value::Array(
            items
                .iter()
                .map(|v| resolve_json_templates(v, context))
                .collect::<Result<_, _>>()?,
        )),
        serde_json::Value::Object(map) => Ok(serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), resolve_json_templates(v, context)?)))
                .collect::<Result<_, String>>()?,
        )),
        other => Ok(other.clone()),
    }
}

/// Resolve template expressions in a merge source's string fields.
pub(super) fn merge_template_resolve(
    source: &MergeSource,
    context: &WorkflowContext,
) -> Result<MergeSource, String> {
    let mut columns = serde_json::Map::new();
    for (k, v) in &source.columns {
        columns.insert(
            k.clone(),
            match v {
                serde_json::Value::String(s) => {
                    serde_json::Value::String(context.resolve_template(s)?)
                }
                other => other.clone(),
            },
        );
    }
    Ok(MergeSource {
        source: context.resolve_template(&source.source)?,
        columns,
        add: resolve_map_templates(&source.add, context)?,
    })
}

fn resolve_map_templates(
    map: &serde_json::Map<String, serde_json::Value>,
    context: &WorkflowContext,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let mut out = serde_json::Map::new();
    for (k, v) in map {
        out.insert(
            k.clone(),
            match v {
                serde_json::Value::String(s) => {
                    serde_json::Value::String(context.resolve_template(s)?)
                }
                other => other.clone(),
            },
        );
    }
    Ok(out)
}

pub(super) fn normalize_query_result(
    data: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let multi: MultiQueryResult = serde_json::from_value(data.clone())
        .map_err(|e| format!("failed to decode query command result: {e}"))?;
    let result = multi
        .results
        .into_iter()
        .next()
        .ok_or_else(|| "Query command returned no statement result".to_string())?;
    let columns = result.columns.clone();
    let col_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    let rows: Vec<serde_json::Value> = result
        .rows
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (i, name) in col_names.iter().enumerate() {
                let value = row.get(i).and_then(|v| v.as_ref());
                obj.insert(
                    name.clone(),
                    value
                        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Null))
                        .unwrap_or(serde_json::Value::Null),
                );
            }
            serde_json::Value::Object(obj)
        })
        .collect();
    Ok(serde_json::json!({
        "rows": rows,
        "rows_count": result.rows.len(),
        "columns": columns,
        "execution_time_ms": result.execution_time_ms,
        "truncated": result.truncated,
    }))
}
