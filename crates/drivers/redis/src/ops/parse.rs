//! Reply parsers shared by the collection readers, and the `Value` → `String` bridge.

pub(crate) fn parse_cursor_from_value(v: &redis::Value) -> Result<u64, String> {
    match v {
        redis::Value::Int(n) => Ok(*n as u64),
        redis::Value::BulkString(b) => {
            let s = String::from_utf8_lossy(b);
            s.trim()
                .parse::<u64>()
                .map_err(|e| format!("bad cursor: {e}"))
        }
        redis::Value::SimpleString(s) => s
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("bad cursor: {e}")),
        other => Err(format!("unexpected cursor value: {other:?}")),
    }
}

pub(crate) fn parse_flat_string_pairs(v: &redis::Value) -> Result<Vec<(String, String)>, String> {
    let items = match v {
        redis::Value::Array(items) => items,
        _ => return Err(format!("expected bulk array for scan pairs, got {v:?}")),
    };
    let mut pairs = Vec::with_capacity(items.len() / 2);
    for chunk in items.chunks(2) {
        if chunk.len() == 2 {
            let field = value_to_string(&chunk[0]);
            let value = value_to_string(&chunk[1]);
            pairs.push((field, value));
        }
    }
    Ok(pairs)
}

pub(crate) fn parse_flat_string_array(v: &redis::Value) -> Result<Vec<String>, String> {
    let items = match v {
        redis::Value::Array(items) => items,
        _ => return Err(format!("expected bulk array, got {v:?}")),
    };
    Ok(items.iter().map(value_to_string).collect())
}

pub(crate) fn parse_zscan_result(raw: &redis::Value) -> Result<(u64, Vec<(String, f64)>), String> {
    match raw {
        redis::Value::Array(items) if items.len() == 2 => {
            let next_cursor = parse_cursor_from_value(&items[0])?;
            let flat = parse_flat_string_array(&items[1])?;
            let mut members = Vec::with_capacity(flat.len() / 2);
            for chunk in flat.chunks(2) {
                if chunk.len() == 2 {
                    let member = chunk[0].clone();
                    let score = chunk[1].parse::<f64>().unwrap_or(0.0);
                    members.push((member, score));
                }
            }
            Ok((next_cursor, members))
        }
        _ => Err(format!("unexpected ZSCAN response: {raw:?}")),
    }
}

pub(crate) fn parse_scan_result_generic(raw: &redis::Value) -> Result<(u64, Vec<String>), String> {
    match raw {
        redis::Value::Array(items) if items.len() == 2 => {
            let next_cursor = parse_cursor_from_value(&items[0])?;
            let members = parse_flat_string_array(&items[1])?;
            Ok((next_cursor, members))
        }
        _ => Err(format!("unexpected SSCAN response: {raw:?}")),
    }
}

pub(crate) fn parse_string_array(raw: &redis::Value) -> Result<Vec<String>, String> {
    match raw {
        redis::Value::Array(items) => Ok(items.iter().map(value_to_string).collect()),
        _ => Err(format!("expected array for LRANGE, got {raw:?}")),
    }
}

pub(crate) fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::Nil => String::new(),
        redis::Value::Int(n) => n.to_string(),
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).into(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Okay => "OK".into(),
        other => format!("{other:?}"),
    }
}
