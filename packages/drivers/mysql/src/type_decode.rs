//! MySQL text column decoding and Row → DriverValue conversion.

use datazen_driver_api::*;
use rust_decimal::prelude::ToPrimitive;
use sqlx::mysql::MySqlRow;
use sqlx::{Column, MySql, Row};

const JS_MAX_SAFE_INT: i64 = 9_007_199_254_740_991;
const JS_MIN_SAFE_INT: i64 = -9_007_199_254_740_991;

pub(crate) fn decode_mysql_text(row: &MySqlRow, col: &str) -> String {
    row.try_get::<String, _>(col)
        .or_else(|_| {
            row.try_get::<Vec<u8>, _>(col)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        })
        .unwrap_or_default()
}

pub(crate) fn decode_mysql_text_opt(row: &MySqlRow, col: &str) -> Option<String> {
    row.try_get::<String, _>(col).ok().or_else(|| {
        row.try_get::<Vec<u8>, _>(col)
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    })
}

pub(crate) fn decode_mysql_text_idx(row: &MySqlRow, index: usize) -> String {
    row.try_get::<String, _>(index)
        .or_else(|_| {
            row.try_get::<Vec<u8>, _>(index)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        })
        .unwrap_or_default()
}


impl super::MysqlDriver {
    pub(crate) fn safe_integer(v: i64) -> Value {
        if v > JS_MAX_SAFE_INT || v < JS_MIN_SAFE_INT {
            Value::String(v.to_string())
        } else {
            Value::Integer(v)
        }
    }

    /// Bind `Value` params into a sqlx MySQL query (`?` placeholders).
    pub(crate) fn bind_values<'q>(
        mut query: sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments>,
        params: &'q [Value],
    ) -> sqlx::query::Query<'q, MySql, sqlx::mysql::MySqlArguments> {
        for p in params {
            query = match p {
                Value::Null => query.bind(Option::<String>::None),
                Value::Bool(b) => query.bind(*b),
                Value::Integer(i) => query.bind(*i),
                Value::Float(f) => query.bind(*f),
                Value::String(s) | Value::Timestamp(s) => query.bind(s.as_str()),
                Value::Bytes(b) => query.bind(b.as_slice()),
                Value::Json(j) => query.bind(j),
            };
        }
        query
    }

    pub(crate) fn columns_of_row(row: &sqlx::mysql::MySqlRow) -> Vec<ColumnInfo> {
        row.columns()
            .iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: c.type_info().to_string(),
                nullable: true,
            })
            .collect()
    }

    pub(crate) fn decode_rows(rows: &[sqlx::mysql::MySqlRow]) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
        let columns: Vec<ColumnInfo> = rows.first().map(Self::columns_of_row).unwrap_or_default();

        let result_rows: Vec<Vec<Option<Value>>> = rows
            .iter()
            .map(|row| {
                row.columns()
                    .iter()
                    .enumerate()
                    .map(|(i, col)| {
                        let debug_name = format!("{:?}", col.type_info());
                        let display_name = col.type_info().to_string();
                        let upper = format!("{} {}", debug_name, display_name).to_uppercase();
                        match upper.as_str() {
                            s if s.contains("BIGINT") || s.contains("INT8") => row
                                .try_get::<i64, _>(i)
                                .ok()
                                .map(Self::safe_integer)
                                .or_else(|| {
                                    row.try_get::<u64, _>(i).ok().map(|v| {
                                        if v > JS_MAX_SAFE_INT as u64 {
                                            Value::String(v.to_string())
                                        } else {
                                            Value::Integer(v as i64)
                                        }
                                    })
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            s if s.contains("MEDIUMINT") => {
                                // MEDIUMINT: 3 bytes, sqlx reads as i32/u32
                                row.try_get::<i32, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| {
                                        row.try_get::<u32, _>(i)
                                            .ok()
                                            .map(|v| Value::Integer(v as i64))
                                    })
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("SMALLINT") => {
                                // SMALLINT: 2 bytes — only use i16/u16
                                row.try_get::<i16, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| {
                                        row.try_get::<u16, _>(i)
                                            .ok()
                                            .map(|v| Value::Integer(v as i64))
                                    })
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("TINYINT") => {
                                // TINYINT: 1 byte — only use i8/u8
                                row.try_get::<i8, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| {
                                        row.try_get::<u8, _>(i)
                                            .ok()
                                            .map(|v| Value::Integer(v as i64))
                                    })
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("INT") => {
                                // INT: 4 bytes — only use i32/u32
                                row.try_get::<i32, _>(i)
                                    .ok()
                                    .map(|v| Value::Integer(v as i64))
                                    .or_else(|| {
                                        row.try_get::<u32, _>(i)
                                            .ok()
                                            .map(|v| Value::Integer(v as i64))
                                    })
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("DOUBLE") => {
                                // DOUBLE: 8 bytes
                                row.try_get::<f64, _>(i).ok().map(Value::Float).or_else(|| {
                                    row.try_get::<String, _>(i)
                                        .ok()
                                        .and_then(|s| s.parse::<f64>().ok())
                                        .map(Value::Float)
                                })
                            }
                            s if s.contains("FLOAT") => {
                                // FLOAT: 4 bytes — use f32, then convert to f64
                                row.try_get::<f32, _>(i)
                                    .ok()
                                    .map(|v| Value::Float(v as f64))
                                    .or_else(|| {
                                        row.try_get::<String, _>(i)
                                            .ok()
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .map(Value::Float)
                                    })
                            }
                            s if s.contains("DECIMAL") || s.contains("NUMERIC") => row
                                .try_get::<rust_decimal::Decimal, _>(i)
                                .ok()
                                .map(|d| {
                                    if d.scale() == 0 {
                                        if let Some(n) = d.to_i64() {
                                            return Self::safe_integer(n);
                                        }
                                    }
                                    d.to_f64()
                                        .map(Value::Float)
                                        .unwrap_or_else(|| Value::String(d.to_string()))
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            s if s.contains("BIT") => row
                                .try_get::<bool, _>(i)
                                .ok()
                                .map(|v| Value::Integer(if v { 1 } else { 0 }))
                                .or_else(|| {
                                    row.try_get::<u8, _>(i)
                                        .ok()
                                        .map(|v| Value::Integer(v as i64))
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            s if s.contains("BOOL") || s.contains("BOOLEAN") => {
                                row.try_get::<bool, _>(i).ok().map(Value::Bool)
                            }
                            s if s.contains("DATE")
                                && !s.contains("DATETIME")
                                && !s.contains("TIMESTAMP") =>
                            {
                                row.try_get::<chrono::NaiveDate, _>(i)
                                    .ok()
                                    .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
                                    .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String))
                            }
                            s if s.contains("DATETIME") || s.contains("TIMESTAMP") => row
                                .try_get::<chrono::NaiveDateTime, _>(i)
                                .ok()
                                .map(|dt| Value::String(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            s if s.contains("TIME") => row
                                .try_get::<chrono::NaiveTime, _>(i)
                                .ok()
                                .map(|t| Value::String(t.format("%H:%M:%S").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            s if s.contains("YEAR") => row
                                .try_get::<u16, _>(i)
                                .ok()
                                .map(|v| Value::Integer(v as i64))
                                .or_else(|| {
                                    row.try_get::<i16, _>(i)
                                        .ok()
                                        .map(|v| Value::Integer(v as i64))
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            s if s.contains("JSON") => row
                                .try_get::<serde_json::Value, _>(i)
                                .ok()
                                .map(Value::Json)
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            _ => {
                                // Only try String for the catch-all; i64/f64 try_get can
                                // panic in sqlx-mysql if column byte-size doesn't match.
                                row.try_get::<String, _>(i).ok().map(Value::String)
                            }
                        }
                    })
                    .collect()
            })
            .collect();

        (columns, result_rows)
    }
}
