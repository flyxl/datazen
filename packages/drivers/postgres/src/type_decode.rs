//! Type decoding from sqlx PgRow/PgColumn to DriverValue.

use crate::postgres::PostgresDriver;
use datazen_driver_api::*;
use rust_decimal::prelude::ToPrimitive;
use sqlx::{Column, Executor, Postgres, Row};

const JS_MAX_SAFE_INT: i64 = 9_007_199_254_740_991;
const JS_MIN_SAFE_INT: i64 = -9_007_199_254_740_991;

impl PostgresDriver {
    pub(crate) fn safe_integer(v: i64) -> Value {
        if v > JS_MAX_SAFE_INT || v < JS_MIN_SAFE_INT {
            Value::String(v.to_string())
        } else {
            Value::Integer(v)
        }
    }

    /// Bind `Value` params into a sqlx Postgres query (`$1`, `$2`, … placeholders).
    pub(crate) fn bind_values<'q>(
        mut query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
        params: &'q [Value],
    ) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
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

    pub(crate) fn columns_of_row(row: &sqlx::postgres::PgRow) -> Vec<ColumnInfo> {
        row.columns()
            .iter()
            .map(|c| ColumnInfo {
                name: c.name().to_string(),
                data_type: c.type_info().to_string(),
                nullable: true,
            })
            .collect()
    }

    pub(crate) async fn describe_columns<'e, E>(executor: E, sql: &str) -> Vec<ColumnInfo>
    where
        E: Executor<'e, Database = Postgres>,
    {
        match executor.describe(sql).await {
            Ok(desc) => desc
                .columns()
                .iter()
                .enumerate()
                .map(|(i, c)| ColumnInfo {
                    name: c.name().to_string(),
                    data_type: c.type_info().to_string(),
                    nullable: desc.nullable(i).unwrap_or(true),
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    pub(crate) fn decode_rows(rows: &[sqlx::postgres::PgRow]) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
        let columns: Vec<ColumnInfo> = rows.first().map(Self::columns_of_row).unwrap_or_default();

        let result_rows: Vec<Vec<Option<Value>>> = rows
            .iter()
            .map(|row| {
                row.columns()
                    .iter()
                    .enumerate()
                    .map(|(i, col)| {
                        let type_name = col.type_info().to_string().to_uppercase();
                        match type_name.as_str() {
                            "INT8" | "BIGINT" | "BIGSERIAL" => {
                                row.try_get::<i64, _>(i).ok().map(Self::safe_integer)
                            }
                            "INT4" | "INT" | "INTEGER" | "SERIAL" => row
                                .try_get::<i32, _>(i)
                                .ok()
                                .map(|v| Value::Integer(v as i64))
                                .or_else(|| row.try_get::<i64, _>(i).ok().map(Self::safe_integer)),
                            "INT2" | "SMALLINT" | "SMALLSERIAL" => row
                                .try_get::<i16, _>(i)
                                .ok()
                                .map(|v| Value::Integer(v as i64))
                                .or_else(|| {
                                    row.try_get::<i32, _>(i)
                                        .ok()
                                        .map(|v| Value::Integer(v as i64))
                                }),
                            "FLOAT4" | "REAL" => row
                                .try_get::<f32, _>(i)
                                .ok()
                                .map(|v| Value::Float(v as f64))
                                .or_else(|| row.try_get::<f64, _>(i).ok().map(Value::Float)),
                            "FLOAT8" | "DOUBLE PRECISION" => {
                                row.try_get::<f64, _>(i).ok().map(Value::Float)
                            }
                            "NUMERIC" | "DECIMAL" => row
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
                                .or_else(|| row.try_get::<f64, _>(i).ok().map(Value::Float))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "BOOL" | "BOOLEAN" => row.try_get::<bool, _>(i).ok().map(Value::Bool),
                            "DATE" => row
                                .try_get::<chrono::NaiveDate, _>(i)
                                .ok()
                                .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIME" | "TIME WITHOUT TIME ZONE" => row
                                .try_get::<chrono::NaiveTime, _>(i)
                                .ok()
                                .map(|t| Value::String(t.format("%H:%M:%S").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIMETZ" | "TIME WITH TIME ZONE" => {
                                row.try_get::<String, _>(i).ok().map(Value::String)
                            }
                            "TIMESTAMP" | "TIMESTAMP WITHOUT TIME ZONE" => row
                                .try_get::<chrono::NaiveDateTime, _>(i)
                                .ok()
                                .map(|dt| Value::String(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => row
                                .try_get::<chrono::DateTime<chrono::Utc>, _>(i)
                                .ok()
                                .map(|dt| Value::String(dt.to_rfc3339()))
                                .or_else(|| {
                                    row.try_get::<chrono::NaiveDateTime, _>(i).ok().map(|dt| {
                                        Value::String(dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                    })
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "UUID" => row
                                .try_get::<uuid::Uuid, _>(i)
                                .ok()
                                .map(|u| Value::String(u.to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "JSON" | "JSONB" => row
                                .try_get::<serde_json::Value, _>(i)
                                .ok()
                                .map(Value::Json)
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            s @ ("INET" | "CIDR" | "MACADDR" | "MACADDR8") => {
                                row.try_get::<String, _>(i)
                                    .ok()
                                    .map(Value::String)
                                    .or_else(|| Some(Value::String(format!("<{}>", s.to_lowercase()))))
                            }
                            "INTERVAL" => row
                                .try_get::<sqlx::postgres::types::PgInterval, _>(i)
                                .ok()
                                .map(|iv| {
                                    let mut parts = Vec::new();
                                    if iv.months != 0 {
                                        let years = iv.months / 12;
                                        let months = iv.months % 12;
                                        if years != 0 {
                                            parts.push(format!("{} years", years));
                                        }
                                        if months != 0 {
                                            parts.push(format!("{} mons", months));
                                        }
                                    }
                                    if iv.days != 0 {
                                        parts.push(format!("{} days", iv.days));
                                    }
                                    if iv.microseconds != 0 {
                                        let total_secs = iv.microseconds / 1_000_000;
                                        let h = total_secs / 3600;
                                        let m = (total_secs % 3600) / 60;
                                        let s = total_secs % 60;
                                        parts.push(format!("{:02}:{:02}:{:02}", h, m, s));
                                    }
                                    Value::String(if parts.is_empty() {
                                        "00:00:00".into()
                                    } else {
                                        parts.join(" ")
                                    })
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "BYTEA" => row.try_get::<Vec<u8>, _>(i).ok().map(|bytes| {
                                let hex: String =
                                    bytes.iter().map(|b| format!("{:02x}", b)).collect();
                                Value::String(format!("\\x{}", hex))
                            }),
                            _ => row
                                .try_get::<String, _>(i)
                                .ok()
                                .map(Value::String)
                                .or_else(|| row.try_get::<i64, _>(i).ok().map(Self::safe_integer))
                                .or_else(|| row.try_get::<f64, _>(i).ok().map(Value::Float))
                                .or_else(|| row.try_get::<bool, _>(i).ok().map(Value::Bool)),
                        }
                    })
                    .collect()
            })
            .collect();

        (columns, result_rows)
    }

    pub(crate) fn extract_pg_plan_metrics(plan_json: &serde_json::Value) -> (Option<f64>, Option<i64>) {
        let plan = plan_json
            .as_array()
            .and_then(|rows| rows.first())
            .and_then(|row| row.get("Plan"));
        let Some(plan) = plan else {
            return (None, None);
        };
        let total_cost = plan.get("Total Cost").and_then(|v| v.as_f64());
        let estimated_rows = plan.get("Plan Rows").and_then(|v| v.as_i64());
        (total_cost, estimated_rows)
    }
}
