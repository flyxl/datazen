//! Driver-owned key semantics used by Data Synchronization.
//!
//! A database's native `ORDER BY` is not sufficient for a cross-endpoint
//! comparison: two sessions may have different collations or return the same
//! native value through different wire representations.  Drivers therefore
//! describe the key domain they can compare safely and normalize each value
//! into this lossless host representation.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::{ColumnSchema, Value};

/// The only collation currently accepted by the sync contract.
///
/// Drivers may add a case-insensitive contract later, but it must be explicit
/// and must provide the matching SQL order expression.  Treating an unknown
/// database collation as binary would silently split equal keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncKeyCollation {
    Binary,
}

/// How NULL participates in key equality and ordering.
///
/// Primary keys are normally NOT NULL.  The explicit reject policy is kept in
/// the contract so a nullable unique key can never accidentally acquire a
/// made-up ordering during comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncKeyNullPolicy {
    Reject,
}

/// A driver-declared key domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncKeyKind {
    Integer { unsigned: bool },
    Text { collation: SyncKeyCollation },
    Decimal { scale: Option<u8> },
    Timestamp { with_timezone: bool, precision: u8 },
}

/// Semantics for one key column.  The host compares contracts on both
/// endpoints before it starts a page scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncKeyContract {
    pub kind: SyncKeyKind,
    pub null_policy: SyncKeyNullPolicy,
}

impl SyncKeyContract {
    pub fn reject_nulls(kind: SyncKeyKind) -> Self {
        Self {
            kind,
            null_policy: SyncKeyNullPolicy::Reject,
        }
    }

    pub fn normalize(&self, value: &Option<Value>) -> Result<SyncKeyValue, String> {
        let value = match value {
            None | Some(Value::Null) => {
                return match self.null_policy {
                    SyncKeyNullPolicy::Reject => Err(
                        "NULL key values are not supported by the Data Sync key contract".into(),
                    ),
                }
            }
            Some(value) => value,
        };

        match &self.kind {
            SyncKeyKind::Integer { unsigned } => {
                let text = match value {
                    Value::Integer(v) => v.to_string(),
                    Value::String(v) => v.trim().to_string(),
                    _ => {
                        return Err("integer sync key was returned with a non-integer value".into())
                    }
                };
                let parsed = text.parse::<i128>().map_err(|_| {
                    format!("integer sync key '{text}' is outside the supported exact range")
                })?;
                if *unsigned && parsed < 0 {
                    return Err(format!("unsigned sync key cannot be negative: {text}"));
                }
                Ok(SyncKeyValue::Integer(parsed))
            }
            SyncKeyKind::Text { .. } => match value {
                Value::String(v) => Ok(SyncKeyValue::Text(v.as_bytes().to_vec())),
                Value::Bytes(v) => Ok(SyncKeyValue::Text(v.clone())),
                _ => Err("text sync key was returned with a non-text value".into()),
            },
            SyncKeyKind::Decimal { .. } => {
                let text = match value {
                    Value::Integer(v) => v.to_string(),
                    Value::Float(v) if v.is_finite() => v.to_string(),
                    Value::String(v) => v.trim().to_string(),
                    _ => {
                        return Err("decimal sync key was returned with a non-numeric value".into())
                    }
                };
                Ok(SyncKeyValue::Decimal(DecimalKey::parse(&text)?))
            }
            SyncKeyKind::Timestamp {
                with_timezone,
                precision,
            } => {
                let text = match value {
                    Value::Timestamp(v) | Value::String(v) => v,
                    _ => return Err("timestamp sync key was returned with a non-text value".into()),
                };
                Ok(SyncKeyValue::Timestamp(normalize_timestamp(
                    text,
                    *with_timezone,
                    *precision,
                )?))
            }
        }
    }
}

/// A lossless, totally ordered key value shared by source and target pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncKeyValue {
    Integer(i128),
    Text(Vec<u8>),
    Decimal(DecimalKey),
    Timestamp(String),
}

impl Ord for SyncKeyValue {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a.cmp(b),
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
            (Self::Decimal(a), Self::Decimal(b)) => a.cmp(b),
            (Self::Timestamp(a), Self::Timestamp(b)) => a.cmp(b),
            (a, b) => sync_key_kind_rank(a).cmp(&sync_key_kind_rank(b)),
        }
    }
}

impl PartialOrd for SyncKeyValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn sync_key_kind_rank(value: &SyncKeyValue) -> u8 {
    match value {
        SyncKeyValue::Integer(_) => 0,
        SyncKeyValue::Text(_) => 1,
        SyncKeyValue::Decimal(_) => 2,
        SyncKeyValue::Timestamp(_) => 3,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecimalKey {
    negative: bool,
    digits: String,
    scale: u32,
}

impl DecimalKey {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("decimal sync key is empty".into());
        }
        if raw.len() > 4096 {
            return Err("decimal sync key exceeds the 4096-byte normalization limit".into());
        }
        let (negative, unsigned) = match raw.as_bytes()[0] {
            b'-' => (true, &raw[1..]),
            b'+' => (false, &raw[1..]),
            _ => (false, raw),
        };
        let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
            Some(index) => {
                let exponent = unsigned[index + 1..]
                    .parse::<i32>()
                    .map_err(|_| format!("invalid decimal exponent in '{raw}'"))?;
                (&unsigned[..index], exponent)
            }
            None => (unsigned, 0),
        };
        let mut parts = mantissa.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next().unwrap_or_default();
        if parts.next().is_some()
            || (whole.is_empty() && fraction.is_empty())
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(format!("invalid decimal sync key '{raw}'"));
        }
        let mut digits = format!("{whole}{fraction}");
        let mut scale = fraction.len() as i32 - exponent;
        if scale < 0 {
            let zeros = scale
                .checked_abs()
                .ok_or_else(|| "decimal exponent is outside the supported range".to_string())?;
            if zeros > 4096 {
                return Err("decimal exponent exceeds the normalization limit".into());
            }
            digits.extend(std::iter::repeat_n('0', zeros as usize));
            scale = 0;
        }
        if scale > 4096 {
            return Err("decimal scale exceeds the normalization limit".into());
        }
        while digits.starts_with('0') && digits.len() > 1 {
            digits.remove(0);
        }
        while scale > 0 && digits.ends_with('0') {
            digits.pop();
            scale -= 1;
        }
        if digits.is_empty() || digits.chars().all(|c| c == '0') {
            return Ok(Self {
                negative: false,
                digits: "0".into(),
                scale: 0,
            });
        }
        Ok(Self {
            negative,
            digits,
            scale: scale as u32,
        })
    }

    fn integral_digits(&self) -> usize {
        self.digits.len().saturating_sub(self.scale as usize)
    }
}

impl Ord for DecimalKey {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.negative != other.negative {
            return if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let magnitude = self
            .integral_digits()
            .cmp(&other.integral_digits())
            .then_with(|| {
                let scale = self.scale.max(other.scale) as usize;
                let left = format!("{}{}", self.digits, "0".repeat(scale - self.scale as usize));
                let right = format!(
                    "{}{}",
                    other.digits,
                    "0".repeat(scale - other.scale as usize)
                );
                left.cmp(&right)
            });
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

impl PartialOrd for DecimalKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn normalize_timestamp(raw: &str, with_timezone: bool, precision: u8) -> Result<String, String> {
    let raw = raw.trim();
    if with_timezone {
        let parsed = chrono::DateTime::parse_from_rfc3339(raw)
            .map_err(|_| format!("timestamp sync key '{raw}' is not a valid RFC3339 timestamp"))?
            .with_timezone(&chrono::Utc);
        return Ok(format_timestamp(
            parsed.format("%Y-%m-%d %H:%M:%S%.fZ").to_string(),
            precision,
        ));
    }
    let normalized = raw.replace('T', " ");
    let parsed = chrono::NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S%.f")
        .map_err(|_| {
            format!("timestamp sync key '{raw}' is not a valid timezone-free timestamp")
        })?;
    Ok(format_timestamp(
        parsed.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
        precision,
    ))
}

fn format_timestamp(mut value: String, precision: u8) -> String {
    if let Some(dot) = value.find('.') {
        let suffix_start = value[dot + 1..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|offset| dot + 1 + offset)
            .unwrap_or(value.len());
        let suffix = value[suffix_start..].to_string();
        let end = dot + 1 + usize::from(precision).min(suffix_start.saturating_sub(dot + 1));
        value.truncate(end);
        if value.ends_with('.') {
            value.pop();
        }
        value.push_str(&suffix);
    }
    value
}

impl SyncKeyValue {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        match value {
            Value::Integer(v) => Ok(Self::Integer(*v as i128)),
            Value::String(v) => Ok(Self::Text(v.as_bytes().to_vec())),
            Value::Bytes(v) => Ok(Self::Text(v.clone())),
            _ => Err("value has no generic sync key ordering".into()),
        }
    }
}

/// Infer a conservative contract for a column.  Drivers should override this
/// method when their type aliases or collation rules need more context.
pub fn contract_from_column(column: &ColumnSchema) -> Result<SyncKeyContract, String> {
    let lower = column.data_type.trim().to_ascii_lowercase();
    let base = lower.split(['(', ' ', ',']).next().unwrap_or_default();
    let kind = match base {
        "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "bigint" | "int2" | "int4"
        | "int8" | "serial" | "bigserial" | "smallserial" => SyncKeyKind::Integer {
            unsigned: lower.contains("unsigned"),
        },
        "char" | "character" | "varchar" | "character varying" | "text" | "tinytext"
        | "mediumtext" | "longtext" => SyncKeyKind::Text {
            collation: SyncKeyCollation::Binary,
        },
        "decimal" | "numeric" => SyncKeyKind::Decimal {
            scale: parse_scale(&lower),
        },
        "timestamp" | "datetime" | "timestamptz" => SyncKeyKind::Timestamp {
            with_timezone: lower.contains("with time zone") || base == "timestamptz",
            precision: parse_precision(&lower),
        },
        _ => {
            return Err(format!(
                "key type '{}' has no verified normalized equality/order contract",
                column.data_type
            ))
        }
    };
    Ok(SyncKeyContract::reject_nulls(kind))
}

fn parse_precision(value: &str) -> u8 {
    value
        .split_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(6)
}

fn parse_scale(value: &str) -> Option<u8> {
    value
        .split_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .and_then(|args| args.split(',').nth(1))
        .and_then(|v| v.trim().parse::<u8>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_normalization_is_exact_and_ordered() {
        let a = DecimalKey::parse("1.20").unwrap();
        let b = DecimalKey::parse("1.2").unwrap();
        let c = DecimalKey::parse("1.21").unwrap();
        assert_eq!(a, b);
        assert!(a < c);
        assert!(DecimalKey::parse("-2").unwrap() < DecimalKey::parse("-1").unwrap());
        assert_eq!(
            DecimalKey::parse("1e3").unwrap(),
            DecimalKey::parse("1000").unwrap()
        );
    }

    #[test]
    fn decimal_normalization_handles_negative_zero_and_exponent_edges() {
        assert_eq!(
            DecimalKey::parse("-0.000").unwrap(),
            DecimalKey::parse("0").unwrap()
        );
        assert!(DecimalKey::parse("-0.01").unwrap() < DecimalKey::parse("0").unwrap());
        assert!(DecimalKey::parse("0.001").unwrap() < DecimalKey::parse("1e-2").unwrap());
        assert!(DecimalKey::parse("-1e3").unwrap() < DecimalKey::parse("-999").unwrap());
        assert!(DecimalKey::parse("1e-3").unwrap() < DecimalKey::parse("0.01").unwrap());
    }

    #[test]
    fn timestamp_normalization_is_timezone_aware_and_precision_bounded() {
        let contract = SyncKeyContract::reject_nulls(SyncKeyKind::Timestamp {
            with_timezone: true,
            precision: 3,
        });
        assert_eq!(
            contract
                .normalize(&Some(Value::String(
                    "2026-01-01T00:00:00.123987+08:00".into()
                )))
                .unwrap(),
            SyncKeyValue::Timestamp("2025-12-31 16:00:00.123Z".into())
        );
        assert_eq!(
            contract
                .normalize(&Some(Value::String("2025-12-31T16:00:00.123000Z".into())))
                .unwrap(),
            SyncKeyValue::Timestamp("2025-12-31 16:00:00.123Z".into())
        );
        assert!(contract
            .normalize(&Some(Value::String("2026-01-01 00:00:00".into())))
            .is_err());
    }

    #[test]
    fn binary_text_and_unsigned_integer_contracts_reject_ambiguous_values() {
        let text = SyncKeyContract::reject_nulls(SyncKeyKind::Text {
            collation: SyncKeyCollation::Binary,
        });
        assert_eq!(
            text.normalize(&Some(Value::Bytes(vec![0x00, 0xff])))
                .unwrap(),
            SyncKeyValue::Text(vec![0x00, 0xff])
        );

        let unsigned = SyncKeyContract::reject_nulls(SyncKeyKind::Integer { unsigned: true });
        assert!(unsigned
            .normalize(&Some(Value::String("-1".into())))
            .is_err());
        assert!(unsigned.normalize(&Some(Value::Null)).is_err());
    }

    #[test]
    fn type_inference_accepts_multiword_text_and_rejects_float_keys() {
        let column = ColumnSchema {
            name: "name".into(),
            data_type: "character varying(80)".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        };
        assert!(matches!(
            contract_from_column(&column).unwrap().kind,
            SyncKeyKind::Text {
                collation: SyncKeyCollation::Binary
            }
        ));
        let float_column = ColumnSchema {
            data_type: "double precision".into(),
            ..column
        };
        assert!(contract_from_column(&float_column).is_err());
    }

    #[test]
    fn null_is_explicitly_rejected() {
        let contract = SyncKeyContract::reject_nulls(SyncKeyKind::Integer { unsigned: false });
        let err = contract.normalize(&Some(Value::Null)).unwrap_err();
        assert!(err.contains("NULL"));
    }

    #[test]
    fn unsupported_key_types_are_explicit() {
        let column = ColumnSchema {
            name: "id".into(),
            data_type: "uuid".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        };
        assert!(contract_from_column(&column)
            .unwrap_err()
            .contains("normalized"));
    }
}
