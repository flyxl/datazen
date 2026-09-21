//! PHP `serialize()` decoder — a strictly parse-only parser.
//!
//! Handles the scalar and array forms (`N;`, `b:`, `i:`, `d:`, `s:`, `a:`) and
//! produces a JSON tree. Object serialisations (`O:` / `C:` / `stdClass` and
//! friends) are rejected: reconstructing a PHP object can trigger `__wakeup` /
//! gadget chains, which a read-only viewer must never do. Reference markers
//! (`r:` / `R:`) are likewise refused rather than silently aliased.

use serde_json::{Map, Value as JsonValue};

const MAX_DEPTH: u32 = 64;
const MAX_LEN: usize = 50 * 1024 * 1024;
const MAX_MEMBERS: usize = 1_000_000;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.data.get(self.pos).ok_or("unexpected EOF")?;
        self.pos += 1;
        Ok(b)
    }
    fn expect(&mut self, b: u8) -> Result<(), String> {
        let got = self.byte()?;
        if got != b {
            return Err(format!(
                "expected '{}' but found {:?}",
                b as char, got as char
            ));
        }
        Ok(())
    }
    /// Read bytes up to (but not including) `delim`, advancing past the delim.
    fn until(&mut self, delim: u8) -> Result<&'a [u8], String> {
        let start = self.pos;
        let idx = self.data[start..]
            .iter()
            .position(|&b| b == delim)
            .ok_or("unterminated token")?;
        self.pos = start + idx + 1;
        Ok(&self.data[start..start + idx])
    }
    /// Read exactly `n` bytes of a quoted string: `"…";`.
    fn take_string(&mut self, n: usize) -> Result<&'a [u8], String> {
        self.expect(b'"')?;
        if n > MAX_LEN {
            return Err("declared string length exceeds limit".to_string());
        }
        let end = self.pos.checked_add(n).ok_or("string length overflow")?;
        if end > self.data.len() {
            return Err("unexpected EOF in string payload".to_string());
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        self.expect(b'"')?;
        self.expect(b';')?;
        Ok(slice)
    }
    fn number(&mut self) -> Result<&'a [u8], String> {
        self.until(b';')
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<JsonValue, String> {
    let mut r = Reader {
        data: bytes,
        pos: 0,
    };
    let v = parse_value(&mut r, 0)?;
    let rest = &r.data[r.pos..];
    if !rest.is_empty() {
        return Err(format!(
            "unexpected trailing bytes after PHP value ({} byte(s))",
            rest.len()
        ));
    }
    Ok(v)
}

fn parse_value(r: &mut Reader, depth: u32) -> Result<JsonValue, String> {
    if depth > MAX_DEPTH {
        return Err("PHP serialization exceeds depth limit".to_string());
    }
    let tag = r.peek().ok_or("unexpected EOF")?;
    match tag {
        b'N' => {
            r.byte()?;
            r.expect(b';')?;
            Ok(JsonValue::Null)
        }
        b'b' => {
            r.byte()?;
            r.expect(b':')?;
            let n = r.number()?;
            Ok(JsonValue::Bool(n.first() == Some(&b'1')))
        }
        b'i' => {
            r.byte()?;
            r.expect(b':')?;
            let n = r.number()?;
            let s = String::from_utf8_lossy(n);
            match s.trim().parse::<i64>() {
                Ok(v) => Ok(JsonValue::from(v)),
                Err(_) => Ok(JsonValue::String(s.into_owned())),
            }
        }
        b'd' => {
            r.byte()?;
            r.expect(b':')?;
            let n = r.number()?;
            let s = String::from_utf8_lossy(n).trim().to_string();
            match s.as_str() {
                "INF" => Ok(JsonValue::String("Infinity".into())),
                "-INF" => Ok(JsonValue::String("-Infinity".into())),
                "NAN" => Ok(JsonValue::String("NaN".into())),
                _ => match s.parse::<f64>() {
                    Ok(f) if f.is_finite() => Ok(serde_json::Number::from_f64(f)
                        .map(JsonValue::Number)
                        .unwrap_or(JsonValue::String(s))),
                    _ => Ok(JsonValue::String(s)),
                },
            }
        }
        b's' => {
            r.byte()?;
            r.expect(b':')?;
            let len_bytes = r.until(b':')?;
            let n = String::from_utf8_lossy(len_bytes)
                .trim()
                .parse::<usize>()
                .map_err(|_| "invalid PHP string length")?;
            let bytes = r.take_string(n)?;
            Ok(JsonValue::String(
                String::from_utf8_lossy(bytes).into_owned(),
            ))
        }
        b'a' => {
            r.byte()?;
            r.expect(b':')?;
            let count_bytes = r.until(b':')?;
            let count = String::from_utf8_lossy(count_bytes)
                .trim()
                .parse::<usize>()
                .map_err(|_| "invalid PHP array count")?;
            if count > MAX_MEMBERS {
                return Err("PHP array exceeds member limit".to_string());
            }
            r.expect(b'{')?;
            parse_array(r, count, depth)
        }
        b'O' | b'C' => {
            Err("PHP serialized object detected — refusing to reconstruct objects".to_string())
        }
        b'r' | b'R' => Err("PHP serialization references are not supported".to_string()),
        other => Err(format!("unsupported PHP tag {:?}", other as char)),
    }
}

fn parse_array(r: &mut Reader, count: usize, depth: u32) -> Result<JsonValue, String> {
    // PHP arrays are ordered maps; keys are scalars. Emit a JSON object when all
    // keys are strings/integers, keeping values as a nested tree.
    let mut obj = Map::with_capacity(count.min(1024));
    for _ in 0..count {
        let key = parse_value(r, depth + 1)?;
        let value = parse_value(r, depth + 1)?;
        obj.insert(key_to_string(&key), value);
    }
    r.expect(b'}')?;
    Ok(JsonValue::Object(obj))
}

fn key_to_string(key: &JsonValue) -> String {
    match key {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => "null".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_scalars_and_nested_array() {
        // a:2:{s:1:"a";i:7;b:1;}  →  {"a":7, ...bool}
        // Build: array with key "x" -> 42, key 0 -> true
        let bytes = b"a:2:{s:1:\"x\";i:42;i:0;b:1;}";
        let v = decode(bytes).unwrap();
        assert_eq!(v["x"], JsonValue::from(42));
        assert_eq!(v["0"], JsonValue::Bool(true));
    }

    #[test]
    fn decodes_null_and_double() {
        assert_eq!(decode(b"N;").unwrap(), JsonValue::Null);
        let d = decode(b"d:3.25;").unwrap();
        assert_eq!(d, JsonValue::from(3.25));
    }

    #[test]
    fn rejects_object() {
        let bytes = b"O:8:\"stdClass\":0:{}";
        let err = decode(bytes).unwrap_err();
        assert!(err.contains("object"), "got: {err}");
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(decode(b"i:1;junk").is_err());
    }
}
