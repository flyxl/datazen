use serde_json::Value;

pub const BLOB_TYPE_KEYWORDS: [&str; 5] = ["blob", "binary", "bytea", "image", "raw"];

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn is_blob_type(type_name: &str) -> bool {
    BLOB_TYPE_KEYWORDS.iter().any(|keyword| type_name.contains(keyword))
}

fn js_byte(number: f64) -> u8 {
    if !number.is_finite() || number == 0.0 {
        return 0;
    }
    let reduced = number.trunc() % 256.0;
    if reduced < 0.0 {
        (reduced + 256.0) as u8
    } else {
        reduced as u8
    }
}

pub fn binary_value_bytes(value: &Value) -> Option<Vec<u8>> {
    let array = value.as_array()?;
    if array.iter().all(|item| item.is_number()) {
        return Some(
            array
                .iter()
                .map(|item| item.as_f64().map(js_byte).unwrap_or(0))
                .collect(),
        );
    }
    None
}

pub fn is_json_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Object(_) | Value::Array(_) => true,
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
                return false;
            }
            serde_json::from_str::<Value>(trimmed).is_ok()
        }
        _ => false,
    }
}

pub fn pretty_json(value: &Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let parsed = match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
                return None;
            }
            serde_json::from_str::<Value>(trimmed).ok()?
        }
        other => other.clone(),
    };
    if !parsed.is_object() && !parsed.is_array() {
        return None;
    }
    serde_json::to_string_pretty(&parsed).ok()
}

pub fn raw_json_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub fn is_quick_lookable_value(value: &Value, column_type: Option<&str>) -> bool {
    if value.is_null() {
        return false;
    }
    let normalized = column_type.unwrap_or("").to_lowercase();
    if is_blob_type(&normalized) {
        return true;
    }
    if is_json_value(value) {
        return true;
    }
    match value {
        Value::String(text) => text.encode_utf16().count() > 120,
        _ => false,
    }
}

fn js_number_string(number: &serde_json::Number) -> String {
    if let Some(integer) = number.as_i64() {
        return integer.to_string();
    }
    if let Some(unsigned) = number.as_u64() {
        return unsigned.to_string();
    }
    if let Some(float) = number.as_f64() {
        if float == 0.0 {
            return "0".to_owned();
        }
        if float.fract() == 0.0 {
            return format!("{float:.0}");
        }
        return format!("{float}");
    }
    String::new()
}

pub fn to_edit_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => js_number_string(number),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub fn bytes_to_base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index] as u32;
        let second = if index + 1 < bytes.len() {
            bytes[index + 1] as u32
        } else {
            0
        };
        let third = if index + 2 < bytes.len() {
            bytes[index + 2] as u32
        } else {
            0
        };
        let packed = (first << 16) | (second << 8) | third;
        out.push(BASE64_ALPHABET[((packed >> 18) & 63) as usize] as char);
        out.push(BASE64_ALPHABET[((packed >> 12) & 63) as usize] as char);
        out.push(if index + 1 < bytes.len() {
            BASE64_ALPHABET[((packed >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if index + 2 < bytes.len() {
            BASE64_ALPHABET[(packed & 63) as usize] as char
        } else {
            '='
        });
        index += 3;
    }
    out
}
