//! Workflow condition expression evaluation.

use super::context::WorkflowContext;

pub(crate) fn evaluate_condition(expr: &str, context: &WorkflowContext) -> bool {
    let expr = expr.trim();

    if let Some(rest) = expr.strip_suffix(".is_empty") {
        let val = context.resolve_expression(rest.trim());
        return val.is_empty() || val == "0" || val == "null" || val == "[]";
    }
    if let Some(rest) = expr.strip_suffix(".is_not_empty") {
        let val = context.resolve_expression(rest.trim());
        return !val.is_empty() && val != "0" && val != "null" && val != "[]";
    }

    for op in &[">=", "<=", "!=", "==", ">", "<"] {
        if let Some(pos) = expr.find(op) {
            let left = context.resolve_expression(expr[..pos].trim());
            let right = expr[pos + op.len()..]
                .trim()
                .trim_matches(|c: char| c == '\'' || c == '"')
                .to_string();
            let left_num = left.parse::<f64>().ok();
            let right_num = right.parse::<f64>().ok();
            return match *op {
                "==" => left == right,
                "!=" => left != right,
                ">" => left_num.zip(right_num).map_or(left > right, |(l, r)| l > r),
                "<" => left_num.zip(right_num).map_or(left < right, |(l, r)| l < r),
                ">=" => left_num
                    .zip(right_num)
                    .map_or(left >= right, |(l, r)| l >= r),
                "<=" => left_num
                    .zip(right_num)
                    .map_or(left <= right, |(l, r)| l <= r),
                _ => false,
            };
        }
    }

    let val = context.resolve_expression(expr);
    !val.is_empty() && val != "0" && val != "false" && val != "null"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_comparison() {
        let mut ctx = WorkflowContext::new(&serde_json::json!({}));
        ctx.set_step_result("s", serde_json::json!({"rows_count": 3}));
        assert!(evaluate_condition("steps.s.rows_count > 0", &ctx));
        assert!(!evaluate_condition("steps.s.rows_count > 5", &ctx));
        assert!(evaluate_condition("steps.s.rows_count == 3", &ctx));
    }

    #[test]
    fn empty_and_truthy() {
        let ctx = WorkflowContext::new(&serde_json::json!({"flag": "yes"}));
        assert!(evaluate_condition("flag", &ctx));
        assert!(!evaluate_condition("missing", &ctx));
    }
}
