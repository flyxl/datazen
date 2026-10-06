//! Canonical typed conversion and comparison for Transfer recordset bounds.

use datazen_driver_api::Value;

/// Error text returned for invalid recordset bounds; callers map it into
/// their own domain error types.
fn validation(msg: impl Into<String>) -> String {
    msg.into()
}

#[derive(Debug, Clone)]
pub enum BoundKey {
    Signed(i128),
    Unsigned(u128),
    Decimal(DecimalKey),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct DecimalKey {
    negative: bool,
    digits: String,
    scale: i32,
}

#[derive(Debug, Clone, Copy)]
enum BoundKind {
    Signed { min: i128, max: i128 },
    Unsigned { max: u128 },
    Decimal,
    Float,
    Boolean,
    Text,
    Unsupported,
}

fn bound_kind(data_type: &str) -> BoundKind {
    let normalized = data_type.trim().to_ascii_lowercase();
    let base = normalized
        .split('(')
        .next()
        .unwrap_or(normalized.as_str())
        .trim();
    if base == "bool" || base == "boolean" {
        return BoundKind::Boolean;
    }
    let unsigned = normalized.contains("unsigned");
    let base_without_spaces = base.replace("unsigned", "").replace(' ', "");
    let integer_width = match base_without_spaces.as_str() {
        "tinyint" => Some(8),
        "smallint" | "int2" | "smallserial" | "serial2" => Some(16),
        "mediumint" => Some(24),
        "int" | "integer" | "int4" | "serial" | "serial4" => Some(32),
        "bigint" | "int8" | "bigserial" | "serial8" => Some(64),
        _ => None,
    };
    if let Some(width) = integer_width {
        if unsigned {
            return BoundKind::Unsigned {
                max: (1u128 << width) - 1,
            };
        }
        let half = 1i128 << (width - 1);
        return BoundKind::Signed {
            min: -half,
            max: half - 1,
        };
    }
    if matches!(
        base_without_spaces.as_str(),
        "decimal" | "dec" | "numeric" | "money"
    ) {
        return BoundKind::Decimal;
    }
    if base_without_spaces == "real"
        || base_without_spaces == "float"
        || base_without_spaces == "float4"
        || base_without_spaces == "float8"
        || base_without_spaces == "double"
        || normalized.contains("double precision")
    {
        return BoundKind::Float;
    }
    if matches!(
        base_without_spaces.as_str(),
        "char"
            | "character"
            | "varchar"
            | "nvarchar"
            | "nchar"
            | "text"
            | "tinytext"
            | "mediumtext"
            | "longtext"
            | "citext"
    ) || matches!(base, "character varying" | "national character varying")
    {
        return BoundKind::Text;
    }
    BoundKind::Unsupported
}

/// Recordset comparison accepts only scalar types with an explicit canonical
/// representation. Unknown/custom types must not inherit text semantics.
pub fn ensure_supported_bound_type(data_type: &str, name: &str) -> Result<(), String> {
    if matches!(bound_kind(data_type), BoundKind::Unsupported) {
        return Err(validation(format!(
            "recordset {name} source type '{data_type}' has no verified ordering contract"
        )));
    }
    Ok(())
}

pub fn is_text_bound_type(data_type: &str) -> bool {
    matches!(bound_kind(data_type), BoundKind::Text)
}

fn raw_bound_text(value: &serde_json::Value, name: &str) -> Result<String, String> {
    match value {
        serde_json::Value::String(value) => Ok(value.trim().to_string()),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        _ => Err(validation(format!(
            "recordset {name} bound must be a scalar number or string"
        ))),
    }
}

pub fn canonical_bound_value(
    raw: &serde_json::Value,
    data_type: &str,
    name: &str,
) -> Result<(Value, BoundKey), String> {
    match bound_kind(data_type) {
        BoundKind::Signed { min, max } => {
            let text = raw_bound_text(raw, name)?;
            let parsed = text.parse::<i128>().map_err(|_| {
                validation(format!(
                    "recordset {name} bound '{text}' is not a valid {data_type} integer"
                ))
            })?;
            if parsed < min || parsed > max {
                return Err(validation(format!(
                    "recordset {name} bound '{text}' is outside source type {data_type}"
                )));
            }
            Ok((Value::Integer(parsed as i64), BoundKey::Signed(parsed)))
        }
        BoundKind::Unsigned { max } => {
            let text = raw_bound_text(raw, name)?;
            let parsed = text.parse::<u128>().map_err(|_| {
                validation(format!(
                    "recordset {name} bound '{text}' is not a valid unsigned {data_type} integer"
                ))
            })?;
            if parsed > max {
                return Err(validation(format!(
                    "recordset {name} bound '{text}' is outside source type {data_type}"
                )));
            }
            let value = i64::try_from(parsed)
                .map(Value::Integer)
                .unwrap_or_else(|_| Value::String(text.clone()));
            Ok((value, BoundKey::Unsigned(parsed)))
        }
        BoundKind::Decimal => {
            let text = raw_bound_text(raw, name)?;
            let key = parse_decimal_key(&text, name)?;
            Ok((Value::String(text), BoundKey::Decimal(key)))
        }
        BoundKind::Float => {
            let text = raw_bound_text(raw, name)?;
            let value = text.parse::<f64>().map_err(|_| {
                validation(format!(
                    "recordset {name} bound '{text}' is not a valid {data_type} number"
                ))
            })?;
            if !value.is_finite() {
                return Err(validation(format!(
                    "recordset {name} bound must be a finite number"
                )));
            }
            Ok((Value::String(text), BoundKey::Float(value)))
        }
        BoundKind::Boolean => {
            let text = raw_bound_text(raw, name)?.to_ascii_lowercase();
            let value = match text.as_str() {
                "true" | "t" | "1" => true,
                "false" | "f" | "0" => false,
                _ => {
                    return Err(validation(format!(
                        "recordset {name} bound '{text}' is not a valid boolean"
                    )));
                }
            };
            Ok((Value::Bool(value), BoundKey::Unsigned(u128::from(value))))
        }
        BoundKind::Text => match raw {
            serde_json::Value::String(value) => {
                Ok((Value::String(value.clone()), BoundKey::Text(value.clone())))
            }
            serde_json::Value::Null => {
                Err(validation(format!("recordset {name} bound cannot be NULL")))
            }
            _ => Err(validation(format!(
                "recordset {name} bound must be a string for source type {data_type}"
            ))),
        },
        BoundKind::Unsupported => Err(validation(format!(
            "recordset {name} source type '{data_type}' has no verified ordering contract"
        ))),
    }
}

fn parse_decimal_key(text: &str, name: &str) -> Result<DecimalKey, String> {
    let (mantissa, exponent) = match text.find(|character| character == 'e' || character == 'E') {
        Some(index) => {
            if text[index + 1..]
                .chars()
                .any(|character| character == 'e' || character == 'E')
            {
                return Err(validation(format!(
                    "recordset {name} bound '{text}' is not a valid decimal"
                )));
            }
            let exponent = text[index + 1..].parse::<i32>().map_err(|_| {
                validation(format!(
                    "recordset {name} bound '{text}' is not a valid decimal"
                ))
            })?;
            (&text[..index], exponent)
        }
        None => (text, 0),
    };
    let (negative, unsigned) = match mantissa.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
    };
    let (whole, fraction) = match unsigned.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (unsigned, ""),
    };
    if whole.is_empty() && fraction.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(validation(format!(
            "recordset {name} bound '{text}' is not a valid decimal"
        )));
    }
    let mut digits = format!("{whole}{fraction}");
    let fraction_scale = i32::try_from(fraction.len())
        .map_err(|_| validation(format!("recordset {name} decimal bound is too precise")))?;
    let mut scale = fraction_scale
        .checked_sub(exponent)
        .ok_or_else(|| validation(format!("recordset {name} decimal bound is too large")))?;
    let first_nonzero = digits
        .bytes()
        .position(|byte| byte != b'0')
        .unwrap_or(digits.len());
    if first_nonzero == digits.len() {
        return Ok(DecimalKey {
            negative: false,
            digits: "0".into(),
            scale: 0,
        });
    }
    digits.drain(..first_nonzero);
    while scale > 0 && digits.ends_with('0') {
        digits.pop();
        scale -= 1;
    }
    if scale < 0 {
        let zeros = usize::try_from(scale.unsigned_abs())
            .map_err(|_| validation(format!("recordset {name} decimal bound is too large")))?;
        if zeros > 1_000_000 {
            return Err(validation(format!(
                "recordset {name} decimal bound is too large"
            )));
        }
        digits.extend(std::iter::repeat('0').take(zeros));
        scale = 0;
    }
    Ok(DecimalKey {
        negative,
        digits,
        scale,
    })
}

pub fn compare_bound_keys(left: &BoundKey, right: &BoundKey) -> Result<std::cmp::Ordering, String> {
    match (left, right) {
        (BoundKey::Signed(left), BoundKey::Signed(right)) => Ok(left.cmp(right)),
        (BoundKey::Unsigned(left), BoundKey::Unsigned(right)) => Ok(left.cmp(right)),
        (BoundKey::Float(left), BoundKey::Float(right)) => left
            .partial_cmp(right)
            .ok_or_else(|| validation("recordset bounds must be finite numbers")),
        (BoundKey::Text(left), BoundKey::Text(right)) => Ok(left.cmp(right)),
        (BoundKey::Decimal(left), BoundKey::Decimal(right)) => compare_decimal_keys(left, right),
        _ => Err(validation(
            "recordset bounds do not use a consistent source type",
        )),
    }
}

fn compare_decimal_keys(
    left: &DecimalKey,
    right: &DecimalKey,
) -> Result<std::cmp::Ordering, String> {
    if left.negative != right.negative {
        return Ok(if left.negative {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        });
    }
    let left_integer_digits = left.digits.len() as i64 - i64::from(left.scale);
    let right_integer_digits = right.digits.len() as i64 - i64::from(right.scale);
    let mut ordering = left_integer_digits.cmp(&right_integer_digits);
    if ordering == std::cmp::Ordering::Equal {
        let max_scale = left.scale.max(right.scale);
        let left_len = left.digits.len() as i32 + max_scale - left.scale;
        let right_len = right.digits.len() as i32 + max_scale - right.scale;
        let max_len = left_len.max(right_len) as usize;
        for index in 0..max_len {
            let left_digit = decimal_digit_at(left, index);
            let right_digit = decimal_digit_at(right, index);
            ordering = left_digit.cmp(&right_digit);
            if ordering != std::cmp::Ordering::Equal {
                break;
            }
        }
    }
    if left.negative {
        Ok(ordering.reverse())
    } else {
        Ok(ordering)
    }
}

fn decimal_digit_at(value: &DecimalKey, index: usize) -> u8 {
    let integer_len = value.digits.len() as i32 - value.scale;
    let target = index as i32;
    if target < integer_len {
        return value.digits.as_bytes()[target as usize];
    }
    let fractional_index = target - integer_len;
    if fractional_index < 0 || fractional_index >= value.scale {
        b'0'
    } else {
        let source = integer_len + fractional_index;
        value.digits.as_bytes()[source as usize]
    }
}
