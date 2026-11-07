//! Type decoding from sqlx PgRow/PgColumn to DriverValue.

use crate::postgres::PostgresDriver;
use datazen_driver_api::*;
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

    pub(crate) fn decode_rows(
        rows: &[sqlx::postgres::PgRow],
    ) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
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
                            "NUMERIC" | "DECIMAL" => row.try_get_raw(i).ok().and_then(|raw| {
                                match raw.format() {
                                    sqlx::postgres::PgValueFormat::Text => {
                                        raw.as_str().ok().map(str::to_string)
                                    }
                                    sqlx::postgres::PgValueFormat::Binary => {
                                        raw.as_bytes().ok().and_then(|bytes| {
                                            crate::numeric::binary_numeric_text(bytes).ok()
                                        })
                                    }
                                }
                                .map(Value::String)
                            }),
                            "BOOL" | "BOOLEAN" => row.try_get::<bool, _>(i).ok().map(Value::Bool),
                            "DATE" => row
                                .try_get::<chrono::NaiveDate, _>(i)
                                .ok()
                                .map(|d| Value::String(d.format("%Y-%m-%d").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIME" | "TIME WITHOUT TIME ZONE" => row
                                .try_get::<chrono::NaiveTime, _>(i)
                                .ok()
                                .map(|t| Value::String(t.format("%H:%M:%S%.f").to_string()))
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIMETZ" | "TIME WITH TIME ZONE" => {
                                row.try_get::<String, _>(i).ok().map(Value::String)
                            }
                            "TIMESTAMP" | "TIMESTAMP WITHOUT TIME ZONE" => row
                                .try_get::<chrono::NaiveDateTime, _>(i)
                                .ok()
                                .map(|dt| {
                                    Value::String(dt.format("%Y-%m-%d %H:%M:%S%.f").to_string())
                                })
                                .or_else(|| row.try_get::<String, _>(i).ok().map(Value::String)),
                            "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" => row
                                .try_get::<chrono::DateTime<chrono::Utc>, _>(i)
                                .ok()
                                .map(|dt| Value::String(dt.to_rfc3339()))
                                .or_else(|| {
                                    row.try_get::<chrono::NaiveDateTime, _>(i).ok().map(|dt| {
                                        Value::String(dt.format("%Y-%m-%d %H:%M:%S%.f").to_string())
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
                                Self::decode_custom_cell(row, i, |bytes| {
                                    decode_network_binary(bytes, s)
                                })
                                .or_else(|| Some(format!("<{}>", s.to_lowercase())))
                                .map(Value::String)
                            }
                            "INTERVAL" => row
                                .try_get::<sqlx::postgres::types::PgInterval, _>(i)
                                .ok()
                                .map(|iv| {
                                    Value::String(format_interval(
                                        iv.months,
                                        iv.days,
                                        iv.microseconds,
                                    ))
                                })
                                .or_else(|| {
                                    // An interval whose typed decode fails used to come
                                    // back as `NULL`, which is indistinguishable from a
                                    // real NULL, so read the cell itself instead.
                                    Self::decode_custom_cell(row, i, decode_interval_binary)
                                        .map(Value::String)
                                }),
                            "BYTEA" => row.try_get::<Vec<u8>, _>(i).ok().map(Value::Bytes),
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

    /// Read one cell that sqlx has no typed `Decode` for.
    ///
    /// In text format the cell already is the server's own text, so it is taken
    /// verbatim; in binary format the wire layout is decoded by `decode`. Using
    /// `try_get::<String>` alone silently failed for both shapes, which surfaced
    /// as a `<inet>` placeholder and an interval that looked like `NULL`.
    fn decode_custom_cell<F>(row: &sqlx::postgres::PgRow, index: usize, decode: F) -> Option<String>
    where
        F: FnOnce(&[u8]) -> Option<String>,
    {
        let raw = row.try_get_raw(index).ok()?;
        match raw.format() {
            sqlx::postgres::PgValueFormat::Text => raw.as_str().ok().map(str::to_string),
            sqlx::postgres::PgValueFormat::Binary => decode(raw.as_bytes().ok()?),
        }
    }

    pub(crate) fn extract_pg_plan_metrics(
        plan_json: &serde_json::Value,
    ) -> (Option<f64>, Option<i64>) {
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

/// Render a PostgreSQL interval the way the server prints it.
///
/// Months become years and months, days stay days and the time part prints as
/// `HH:MM:SS`; an all-zero interval is `00:00:00` rather than an empty cell.
fn format_interval(months: i32, days: i32, microseconds: i64) -> String {
    let mut parts = Vec::new();
    if months != 0 {
        let years = months / 12;
        let remainder = months % 12;
        if years != 0 {
            parts.push(format!("{} years", years));
        }
        if remainder != 0 {
            parts.push(format!("{} mons", remainder));
        }
    }
    if days != 0 {
        parts.push(format!("{} days", days));
    }
    if microseconds != 0 {
        let total_secs = microseconds / 1_000_000;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        parts.push(format!("{:02}:{:02}:{:02}", hours, minutes, seconds));
    }
    if parts.is_empty() {
        "00:00:00".into()
    } else {
        parts.join(" ")
    }
}

/// Decode the 16-byte binary form of `interval`: time, days, months.
fn decode_interval_binary(bytes: &[u8]) -> Option<String> {
    if bytes.len() != 16 {
        return None;
    }
    let microseconds = i64::from_be_bytes(bytes[0..8].try_into().ok()?);
    let days = i32::from_be_bytes(bytes[8..12].try_into().ok()?);
    let months = i32::from_be_bytes(bytes[12..16].try_into().ok()?);
    Some(format_interval(months, days, microseconds))
}

/// Decode the binary form of `inet` / `cidr` / `macaddr` / `macaddr8`.
///
/// `inet` and `cidr` are `family, bits, is_cidr, length, address`; `macaddr` is
/// the bare 6 bytes and `macaddr8` the bare 8 bytes.
fn decode_network_binary(bytes: &[u8], type_name: &str) -> Option<String> {
    match type_name {
        "MACADDR" | "MACADDR8" => {
            let octets: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            Some(octets.join(":"))
        }
        _ => {
            if bytes.len() < 4 {
                return None;
            }
            let family = bytes[0];
            let bits = bytes[1];
            let address_len = usize::from(bytes[3]);
            let address = bytes.get(4..4 + address_len)?;
            let address = match (family, address_len) {
                (2, 4) => std::net::Ipv4Addr::new(address[0], address[1], address[2], address[3])
                    .to_string(),
                (3, 16) => {
                    let mut octets = [0u8; 16];
                    octets.copy_from_slice(address);
                    std::net::Ipv6Addr::from(octets).to_string()
                }
                _ => return None,
            };
            let full_width = u16::from(bits) == u16::try_from(address_len * 8).ok()?;
            Some(if full_width {
                address
            } else {
                format!("{}/{}", address, bits)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_binary_matches_the_server_text_form() {
        let bytes = 9_000_000_000i64
            .to_be_bytes()
            .into_iter()
            .chain(0i32.to_be_bytes())
            .chain(0i32.to_be_bytes())
            .collect::<Vec<u8>>();
        assert_eq!(decode_interval_binary(&bytes).as_deref(), Some("02:30:00"));
    }

    #[test]
    fn interval_binary_keeps_years_days_and_time_together() {
        let bytes = 3_600_000_000i64
            .to_be_bytes()
            .into_iter()
            .chain(2i32.to_be_bytes())
            .chain(14i32.to_be_bytes())
            .collect::<Vec<u8>>();
        assert_eq!(
            decode_interval_binary(&bytes).as_deref(),
            Some("1 years 2 mons 2 days 01:00:00")
        );
    }

    #[test]
    fn interval_binary_zero_is_not_an_empty_cell() {
        let bytes = 0i64
            .to_be_bytes()
            .into_iter()
            .chain(0i32.to_be_bytes())
            .chain(0i32.to_be_bytes())
            .collect::<Vec<u8>>();
        assert_eq!(decode_interval_binary(&bytes).as_deref(), Some("00:00:00"));
    }

    #[test]
    fn network_binary_decodes_ipv4_and_ipv6_without_a_placeholder() {
        let ipv4 = decode_network_binary(&[2, 32, 0, 4, 192, 168, 1, 1], "INET");
        assert_eq!(ipv4.as_deref(), Some("192.168.1.1"));
        let cidr = decode_network_binary(&[2, 24, 1, 4, 10, 0, 0, 0], "CIDR");
        assert_eq!(cidr.as_deref(), Some("10.0.0.0/24"));
        let ipv6 = decode_network_binary(
            &[
                3, 128, 0, 16, 0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
            ],
            "INET",
        );
        assert_eq!(ipv6.as_deref(), Some("2001:db8::1"));
    }

    #[test]
    fn network_binary_decodes_mac_addresses_as_hex_pairs() {
        let mac = decode_network_binary(&[0x08, 0x00, 0x2b, 0x01, 0x02, 0x03], "MACADDR");
        assert_eq!(mac.as_deref(), Some("08:00:2b:01:02:03"));
    }
}
