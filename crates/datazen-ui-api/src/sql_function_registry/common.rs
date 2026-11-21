use std::sync::OnceLock;

use super::{FunctionEntry, FunctionParam};

pub const COMMON_FUNCTION_NAMES: [&str; 9] = [
    "COUNT", "SUM", "AVG", "MIN", "MAX", "COALESCE", "NULLIF", "CAST", "CONCAT",
];

fn param(name: &str, param_type: &str) -> FunctionParam {
    FunctionParam {
        name: name.to_owned(),
        param_type: param_type.to_owned(),
        optional: None,
    }
}

fn entry(name: &str, description: &str, params: Vec<FunctionParam>) -> FunctionEntry {
    FunctionEntry {
        name: name.to_owned(),
        description: description.to_owned(),
        params,
        dialects: None,
        returns_value: None,
    }
}

pub fn common_functions() -> &'static [FunctionEntry] {
    static FUNCTIONS: OnceLock<Vec<FunctionEntry>> = OnceLock::new();
    FUNCTIONS.get_or_init(|| {
        vec![
            entry("COUNT", "Count rows or non-null values", vec![param("expr", "any")]),
            entry("SUM", "Sum of numeric values", vec![param("expr", "number")]),
            entry("AVG", "Average of numeric values", vec![param("expr", "number")]),
            entry("MIN", "Minimum value", vec![param("expr", "any")]),
            entry("MAX", "Maximum value", vec![param("expr", "any")]),
            entry(
                "COALESCE",
                "First non-null value",
                vec![param("expr1", "any"), param("expr2", "any")],
            ),
            entry(
                "NULLIF",
                "NULL if two values are equal",
                vec![param("value1", "any"), param("value2", "any")],
            ),
            entry("CAST", "Convert value to another type", vec![param("expr", "any")]),
            entry(
                "CONCAT",
                "Concatenate strings",
                vec![param("str1", "string"), param("str2", "string")],
            ),
        ]
    })
}
