//! Data integration steps: `merge` (concatenate row groups into one table)
//! and `transform` (row-level computed columns / filter / sort / window).
//!
//! These are pure-Rust transformations over the already-produced step results;
//! they deliberately do not embed any scripting language. Each step reads from
//! the workflow context's JSON values and writes the result back via
//! `set_step_result`, using the same `{ "rows": [...], "columns": [...] }`
//! shape that query steps and the dashboard consumer already understand.

use serde_json::{Map, Value};

use super::context::{json_value_to_string, WorkflowContext};

// ──────────────────────────────────────────────────────────────────────────
// Shared helpers
// ──────────────────────────────────────────────────────────────────────────

/// Resolve a `from`/`source` expression into a JSON value, using the same
/// two-step resolution as `foreach.items`: try parsing as JSON, else treat as
/// a deep path into the workflow context.
pub fn resolve_rows_expression(expr: &str, context: &WorkflowContext) -> Result<Value, String> {
    let trimmed = expr.trim();
    let parsed = serde_json::from_str::<Value>(trimmed)
        .map_err(|_| ())
        .or_else(|_| context.resolve_deep_path(trimmed).ok_or(()))
        .map_err(|_| format!("Could not resolve row source '{trimmed}'"))?;
    Ok(parsed)
}

/// Coerce an arbitrary JSON value into a `Vec<Value>` where each element is an
/// object. Non-object entries are wrapped as `{ "value": <item> }`.
fn rows_to_objects(value: &Value) -> Result<Vec<Value>, String> {
    match value {
        Value::Array(items) => {
            let mut rows = Vec::new();
            for item in items {
                match item {
                    Value::Object(_) => rows.push(item.clone()),
                    _ => rows.push(serde_json::json!({ "value": item.clone() })),
                }
            }
            Ok(rows)
        }
        Value::Object(map) if map.contains_key("rows") => match map.get("rows") {
            Some(Value::Array(items)) => Ok(items.clone()),
            _ => Err("expression resolved to an object whose `rows` is not an array".into()),
        },
        Value::Null => Ok(Vec::new()),
        other => Err(format!("expression resolved to non-array value: {other}")),
    }
}

/// Build the standard result object `{ "rows": [...], "columns": [...] }`.
pub fn build_table(rows: Vec<Value>, columns: Vec<String>) -> Value {
    serde_json::json!({
        "rows": rows,
        "columns": columns,
        "rows_count": rows.len(),
    })
}

fn push_column(columns: &mut Vec<String>, name: String) {
    if !columns.contains(&name) {
        columns.push(name);
    }
}

/// Resolve a dot path (e.g. `order.amount` or `amount[0]`) against an object.
fn access<'a>(obj: &'a Map<String, Value>, path: &str) -> Value {
    let mut current: Value = serde_json::to_value(obj).unwrap_or(Value::Object(obj.clone()));
    for part in path.split('.') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(idx_bracket) = part.find('[') {
            let field = &part[..idx_bracket];
            let idx_str = part[idx_bracket + 1..].trim_end_matches(']');
            let next = if !field.is_empty() {
                current.get(field).cloned().unwrap_or(Value::Null)
            } else {
                current.clone()
            };
            current = match idx_str.parse::<usize>() {
                Ok(i) => next.get(i).cloned().unwrap_or(Value::Null),
                Err(_) => Value::Null,
            };
        } else if let Ok(idx) = part.parse::<usize>() {
            current = current.get(idx).cloned().unwrap_or(Value::Null);
        } else {
            current = current.get(part).cloned().unwrap_or(Value::Null);
        }
    }
    current
}

// ──────────────────────────────────────────────────────────────────────────
// `merge` step
// ──────────────────────────────────────────────────────────────────────────

/// Concatenate multiple row groups into a single table with optional column
/// projection, rename, and constant column injection.
pub fn run_merge(
    sources: &[super::model::MergeSource],
    global_columns: &Option<Vec<String>>,
    context: &WorkflowContext,
) -> Result<Value, String> {
    let mut all_rows: Vec<Value> = Vec::new();
    let mut column_order: Vec<String> = Vec::new();

    for source in sources {
        let raw = resolve_rows_expression(&source.source, context)?;
        let rows = rows_to_objects(&raw)?;

        for row in rows {
            let obj = match row {
                Value::Object(map) => map,
                _ => return Err(format!("merge source resolved to a non-object row: {row}")),
            };

            let mut out = Map::new();

            // When no projection is declared, keep all source columns as-is.
            if source.columns.is_empty() {
                for (k, v) in &obj {
                    out.insert(k.clone(), v.clone());
                    push_column(&mut column_order, k.clone());
                }
            } else {
                // Projected / renamed columns.
                for (output_name, src_field) in &source.columns {
                    let field_path = src_field
                        .as_str()
                        .ok_or_else(|| {
                            format!("merge: column '{output_name}' must map to a string field path")
                        })?
                        .to_string();
                    let v = access(&obj, &field_path);
                    out.insert(output_name.clone(), v);
                    push_column(&mut column_order, output_name.clone());
                }
            }

            // Constant `add` columns.
            for (k, v) in &source.add {
                out.insert(k.clone(), v.clone());
                push_column(&mut column_order, k.clone());
            }

            all_rows.push(Value::Object(out));
        }
    }

    // Apply optional global column order (projects/reorders output columns).
    if let Some(order) = global_columns {
        let ordered: Vec<Value> = all_rows
            .iter()
            .map(|r| {
                let obj = r.as_object().cloned().unwrap_or_default();
                let mut out = Map::new();
                for name in order {
                    if let Some(v) = obj.get(name) {
                        out.insert(name.clone(), v.clone());
                    }
                }
                for (k, v) in &obj {
                    if !order.contains(k) {
                        out.insert(k.clone(), v.clone());
                    }
                }
                Value::Object(out)
            })
            .collect();
        let mut cols = order.clone();
        for cobj in all_rows.iter().filter_map(|r| r.as_object()) {
            for k in cobj.keys() {
                push_column(&mut cols, k.clone());
            }
        }
        Ok(build_table(ordered, cols))
    } else {
        Ok(build_table(all_rows, column_order))
    }
}

// ──────────────────────────────────────────────────────────────────────────
// `transform` step
// ──────────────────────────────────────────────────────────────────────────

/// Row-level computed columns, filter, sort, offset and limit.
#[allow(clippy::too_many_arguments)]
pub fn run_transform(
    from: &str,
    add_columns: &[super::model::TransformColumn],
    filter: &Option<String>,
    sort_by: &Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    context: &WorkflowContext,
) -> Result<Value, String> {
    let raw = resolve_rows_expression(from, context)?;
    let mut rows = rows_to_objects(&raw)?;

    // Compute columns first.
    if !add_columns.is_empty() {
        for row in rows.iter_mut() {
            let obj = match row.as_object_mut() {
                Some(o) => o,
                None => {
                    return Err(
                        "transform: row is not an object (use merge for scalar sources)".into(),
                    )
                }
            };
            for col in add_columns {
                let v = evaluate_expression(&col.expr, obj)?;
                obj.insert(col.name.clone(), v);
            }
        }
    }

    // Filter.
    if let Some(expr) = filter {
        rows.retain(|row| {
            row.as_object()
                .map(|obj| evaluate_truthy_expression(expr, obj).unwrap_or(false))
                .unwrap_or(false)
        });
    }

    // Sort.
    if let Some(sb) = sort_by {
        let desc = sb.trim_start().starts_with('-');
        let field = sb.trim_start().trim_start_matches('-').trim().to_string();
        rows.sort_by(|a, b| {
            let av = access(a.as_object().unwrap_or(&Map::new()), &field);
            let bv = access(b.as_object().unwrap_or(&Map::new()), &field);
            compare_json(&av, &bv)
                .then_with(|| json_value_to_string(&av).cmp(&json_value_to_string(&bv)))
        });
        if desc {
            rows.reverse();
        }
    }

    // Offset / limit.
    let start = offset.unwrap_or(0);
    if start > 0 {
        if start >= rows.len() {
            rows.clear();
        } else {
            rows = rows.split_off(start);
        }
    }
    if let Some(lim) = limit {
        rows.truncate(lim);
    }

    // Column order: first-seen keys across rows.
    let mut cols: Vec<String> = Vec::new();
    for row in &rows {
        if let Some(obj) = row.as_object() {
            for k in obj.keys() {
                push_column(&mut cols, k.clone());
            }
        }
    }

    Ok(build_table(rows, cols))
}

/// Compare two JSON values for sorting (numbers numerically, otherwise string).
fn compare_json(a: &Value, b: &Value) -> std::cmp::Ordering {
    let an = a.as_f64();
    let bn = b.as_f64();
    match (an, bn) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => json_value_to_string(a).cmp(&json_value_to_string(b)),
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Minimal non-Turing expression evaluator
// ──────────────────────────────────────────────────────────────────────────
//
// Grammar (left-to-right with precedence):
//   atom   := number | 'quoted string' | ident (field ref / dot path) | '(' expr ')'
//   unary  := '!' unary | '-' unary | atom
//   mul    := unary (('*'|'/'|'%') unary)*
//   add    := mul (('+'|'-') mul)*
//   cmp    := add (('=='|'!='|'>'|'<'|'>='|'<=') add)?   -- single comparison
//   expr   := cmp (('&&'|'||') cmp)*
//
// Field references resolve against the current row object. Unknown fields
// evaluate to null (treated as empty/0 in numeric contexts). The evaluator is
// intentionally not Turing-complete.

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    Op(String),
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let mut num = String::new();
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                num.push(chars[i]);
                i += 1;
            }
            match num.parse::<f64>() {
                Ok(v) => out.push(Tok::Num(v)),
                Err(_) => return Err(format!("invalid number literal '{num}'")),
            }
            continue;
        }
        if c == '\'' || c == '"' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != quote {
                s.push(chars[i]);
                i += 1;
            }
            if i >= chars.len() {
                return Err("unterminated string literal".into());
            }
            i += 1;
            out.push(Tok::Str(s));
            continue;
        }
        let two: String = chars
            .get(i..i + 2)
            .map(|s| s.iter().collect())
            .unwrap_or_default();
        match two.as_str() {
            "==" | "!=" | ">=" | "<=" | "&&" | "||" => {
                out.push(Tok::Op(two.clone()));
                i += 2;
                continue;
            }
            _ => {}
        }
        match c {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '+' | '-' | '*' | '/' | '%' | '>' | '<' | '!' => {
                out.push(Tok::Op(c.to_string()));
                i += 1;
            }
            _ if c.is_alphanumeric() || c == '_' || c == '.' => {
                let mut id = String::new();
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    id.push(chars[i]);
                    i += 1;
                }
                out.push(Tok::Ident(id));
            }
            other => return Err(format!("unexpected character '{other}'")),
        }
    }
    Ok(out)
}

/// Evaluate an expression against a row object.
pub fn evaluate_expression(expr: &str, row: &Map<String, Value>) -> Result<Value, String> {
    let toks = tokenize(expr)?;
    let mut p = RowParser { toks, pos: 0, row };
    p.parse()
}

/// Evaluate a filter expression, returning a truthy/falsy decision.
pub fn evaluate_truthy_expression(expr: &str, row: &Map<String, Value>) -> Result<bool, String> {
    let v = evaluate_expression(expr, row)?;
    Ok(truthy(&v))
}

struct RowParser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    row: &'a Map<String, Value>,
}

impl<'a> RowParser<'a> {
    fn peek(&self) -> Option<Tok> {
        self.toks.get(self.pos).cloned()
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn expect_op(&mut self, op: &str) -> Result<(), String> {
        match self.next() {
            Some(Tok::Op(o)) if &o == op => Ok(()),
            other => Err(format!("expected '{op}', got {other:?}")),
        }
    }
    fn parse(&mut self) -> Result<Value, String> {
        self.parse_or()
    }
    fn parse_or(&mut self) -> Result<Value, String> {
        let mut lhs = self.parse_and()?;
        while matches!(self.peek(), Some(Tok::Op(op)) if op == "||") {
            self.next();
            let rhs = self.parse_and()?;
            lhs = Value::Bool(truthy(&lhs) || truthy(&rhs));
        }
        Ok(lhs)
    }
    fn parse_and(&mut self) -> Result<Value, String> {
        let mut lhs = self.parse_cmp()?;
        while matches!(self.peek(), Some(Tok::Op(op)) if op == "&&") {
            self.next();
            let rhs = self.parse_cmp()?;
            lhs = Value::Bool(truthy(&lhs) && truthy(&rhs));
        }
        Ok(lhs)
    }
    fn parse_cmp(&mut self) -> Result<Value, String> {
        let lhs = self.parse_add()?;
        for op in ["==", "!=", ">", "<", ">=", "<="] {
            if matches!(self.peek(), Some(Tok::Op(o)) if o == op) {
                self.next();
                let rhs = self.parse_add()?;
                return apply_comparison(op, &lhs, &rhs);
            }
        }
        Ok(lhs)
    }
    fn parse_add(&mut self) -> Result<Value, String> {
        let mut lhs = self.parse_mul()?;
        loop {
            match self.peek() {
                Some(Tok::Op(op)) if op == "+" || op == "-" => {
                    self.next();
                    let rhs = self.parse_mul()?;
                    lhs = apply_arith(&op, &lhs, &rhs)?;
                }
                _ => break,
            }
        }
        Ok(lhs)
    }
    fn parse_mul(&mut self) -> Result<Value, String> {
        let mut lhs = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Tok::Op(op)) if op == "*" || op == "/" || op == "%" => {
                    self.next();
                    let rhs = self.parse_unary()?;
                    lhs = apply_arith(&op, &lhs, &rhs)?;
                }
                _ => break,
            }
        }
        Ok(lhs)
    }
    fn parse_unary(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(Tok::Op(op)) if op == "!" || op == "-" => {
                self.next();
                let v = self.parse_unary()?;
                if op == "!" {
                    Ok(Value::Bool(!truthy(&v)))
                } else {
                    match v.as_f64() {
                        Some(n) => Ok(Value::from(-n)),
                        None => Ok(Value::Null),
                    }
                }
            }
            _ => self.parse_atom(),
        }
    }
    fn parse_atom(&mut self) -> Result<Value, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Value::from(n)),
            Some(Tok::Str(s)) => Ok(Value::String(s)),
            Some(Tok::Ident(name)) => Ok(access(self.row, &name)),
            Some(Tok::LParen) => {
                let v = self.parse()?;
                self.expect_op(")")?;
                Ok(v)
            }
            other => Err(format!("unexpected token: {other:?}")),
        }
    }
}

fn apply_arith(op: &str, a: &Value, b: &Value) -> Result<Value, String> {
    if op == "+" {
        if let (Some(av), None) = (a.as_str(), b.as_str()) {
            return Ok(Value::String(format!("{av}{}", json_value_to_string(b))));
        }
        if let (None, Some(bv)) = (a.as_str(), b.as_str()) {
            return Ok(Value::String(format!("{}{bv}", json_value_to_string(a))));
        }
        if let (Some(av), Some(bv)) = (a.as_str(), b.as_str()) {
            return Ok(Value::String(format!("{av}{bv}")));
        }
    }
    let an = a.as_f64().unwrap_or(0.0);
    let bn = b.as_f64().unwrap_or(0.0);
    let r = match op {
        "+" => an + bn,
        "-" => an - bn,
        "*" => an * bn,
        "/" => {
            if bn == 0.0 {
                return Err("division by zero".into());
            }
            an / bn
        }
        "%" => {
            if bn == 0.0 {
                return Err("modulo by zero".into());
            }
            an % bn
        }
        _ => return Err(format!("unsupported operator '{op}'")),
    };
    Ok(Value::from(r))
}

fn apply_comparison(op: &str, a: &Value, b: &Value) -> Result<Value, String> {
    let result = match op {
        "==" => compare_json(a, b) == std::cmp::Ordering::Equal,
        "!=" => compare_json(a, b) != std::cmp::Ordering::Equal,
        ">" => compare_json(a, b) == std::cmp::Ordering::Greater,
        "<" => compare_json(a, b) == std::cmp::Ordering::Less,
        ">=" => compare_json(a, b) != std::cmp::Ordering::Less,
        "<=" => compare_json(a, b) != std::cmp::Ordering::Greater,
        _ => return Err(format!("unsupported comparison '{op}'")),
    };
    Ok(Value::Bool(result))
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty() && s != "0" && s != "false" && s != "null",
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::model::{MergeSource, TransformColumn};

    fn ctx_with(entries: &[(&str, Value)]) -> WorkflowContext {
        let mut c = WorkflowContext::new(&serde_json::json!({}));
        for (name, value) in entries {
            c.set_step_result(name, value.clone());
        }
        c
    }

    #[test]
    fn merge_concatenates_and_injects_src_column() {
        let c = ctx_with(&[
            (
                "pg",
                serde_json::json!({"rows":[
                    {"customer":"a","amount":10},
                    {"customer":"b","amount":20}
                ]}),
            ),
            (
                "my",
                serde_json::json!({"rows":[{"customer":"c","amount":5}]}),
            ),
        ]);
        let sources = vec![
            MergeSource {
                source: "steps.pg.rows".into(),
                columns: serde_json::Map::new(),
                add: serde_json::json!({"src":"PG"})
                    .as_object()
                    .cloned()
                    .unwrap(),
            },
            MergeSource {
                source: "steps.my.rows".into(),
                columns: serde_json::Map::new(),
                add: serde_json::json!({"src":"MY"})
                    .as_object()
                    .cloned()
                    .unwrap(),
            },
        ];
        let out = run_merge(&sources, &None, &c).unwrap();
        let rows = out["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["customer"], "a");
        assert_eq!(rows[0]["src"], "PG");
        assert_eq!(rows[2]["src"], "MY");
    }

    #[test]
    fn merge_project_and_rename() {
        let c = ctx_with(&[(
            "pg",
            serde_json::json!({"rows":[{"customer_name":"x","amount":10}]}),
        )]);
        let mut cols = serde_json::Map::new();
        cols.insert("customer".into(), serde_json::json!("customer_name"));
        let sources = vec![MergeSource {
            source: "steps.pg.rows".into(),
            columns: cols,
            add: serde_json::Map::new(),
        }];
        let out = run_merge(&sources, &None, &c).unwrap();
        assert_eq!(out["rows"][0]["customer"], "x");
        assert!(out["rows"][0].get("amount").is_none());
    }

    #[test]
    fn transform_adds_computed_column() {
        let c = ctx_with(&[(
            "s",
            serde_json::json!({"rows":[
                {"amount":100,"cost":70},
                {"amount":50,"cost":30}
            ]}),
        )]);
        // `amount` and `cost` resolve via the tokenizer's field references.
        let cols = vec![TransformColumn {
            name: "profit".into(),
            expr: "amount - cost".into(),
        }];
        let out = run_transform("steps.s.rows", &cols, &None, &None, None, None, &c).unwrap();
        assert_eq!(out["rows"][0]["profit"], 30.0);
        assert_eq!(out["rows"][1]["profit"], 20.0);
    }

    #[test]
    fn transform_filters_rows() {
        let c = ctx_with(&[(
            "s",
            serde_json::json!({"rows":[
                {"amount":5},{"amount":15},{"amount":50}
            ]}),
        )]);
        let out = run_transform(
            "steps.s.rows",
            &[],
            &Some("amount > 10".into()),
            &None,
            None,
            None,
            &c,
        )
        .unwrap();
        let rows = out["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["amount"], 15);
    }

    #[test]
    fn transform_sort_desc_and_limit() {
        let c = ctx_with(&[("s", serde_json::json!({"rows":[{"v":3},{"v":1},{"v":2}]}))]);
        let out = run_transform(
            "steps.s.rows",
            &[],
            &None,
            &Some("-v".into()),
            None,
            Some(2),
            &c,
        )
        .unwrap();
        let rows = out["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["v"], 3);
        assert_eq!(rows[1]["v"], 2);
    }

    #[test]
    fn transform_compound_filter_with_and() {
        let c = ctx_with(&[(
            "s",
            serde_json::json!({"rows":[
                {"amount":15,"src":"A"},
                {"amount":5,"src":"A"},
                {"amount":20,"src":"B"}
            ]}),
        )]);
        let out = run_transform(
            "steps.s.rows",
            &[],
            &Some("amount > 10 && src == 'A'".into()),
            &None,
            None,
            None,
            &c,
        )
        .unwrap();
        let rows = out["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["amount"], 15);
    }

    #[test]
    fn evaluate_bare_field_and_string_concat() {
        let mut row = Map::new();
        row.insert("name".into(), serde_json::json!("alice"));
        row.insert("age".into(), serde_json::json!(30));
        assert_eq!(
            evaluate_expression("name", &row).unwrap().as_str().unwrap(),
            "alice"
        );
        assert_eq!(
            evaluate_expression("name + '!'", &row)
                .unwrap()
                .as_str()
                .unwrap(),
            "alice!"
        );
        assert_eq!(evaluate_expression("age * 2", &row).unwrap(), 60.0);
    }
}
