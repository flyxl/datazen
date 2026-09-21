//! Java object-stream decoder — a *read-only structural outline*.
//!
//! This module never instantiates a single Java object. It validates the stream
//! header (`0xACED 0x0005`), walks the `TC_*` tokens, and emits a JSON tree of
//! class names, field names/types and primitive values — the same "deserializ-
//! ation viewer" strategy used for safe inspection of untrusted blobs. Because
//! we only read bytes and never resolve classes or call `readObject`, gadget
//! chains (CommonsCollections, Spring, …) are inert here. Anything that does not
//! conform to the expected token layout is rejected rather than guessed.

use serde_json::{Map, Value as JsonValue};

const STREAM_MAGIC: u16 = 0xACED;
const STREAM_VERSION: u16 = 0x0005;

const TC_NULL: u8 = 0x70;
const TC_REFERENCE: u8 = 0x71;
const TC_CLASSDESC: u8 = 0x72;
const TC_OBJECT: u8 = 0x65;
const TC_STRING: u8 = 0x74;
const TC_ARRAY: u8 = 0xDC;
const TC_LONGSTRING: u8 = 0x7C;
const TC_ENDBLOCKDATA: u8 = 0x78;
const TC_BLOCKDATA: u8 = 0x77;
const TC_BLOCKDATALONG: u8 = 0x7A;
const TC_CLASS: u8 = 0x76;
const TC_ENUM: u8 = 0xFE;

const SC_WRITE_METHOD: u8 = 0x01;
const SC_SERIALIZABLE: u8 = 0x02;

const HANDLE_BASE: u32 = 0x7E0000;
const MAX_LEN: usize = 50 * 1024 * 1024;
const MAX_MEMBERS: usize = 1_000_000;
const MAX_DEPTH: u32 = 64;

struct Stream<'a> {
    data: &'a [u8],
    pos: usize,
    handles: Vec<Option<JsonValue>>,
}

impl<'a> Stream<'a> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.data.get(self.pos).ok_or("unexpected EOF")?;
        self.pos += 1;
        Ok(b)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if n > MAX_LEN {
            return Err("declared length exceeds limit".to_string());
        }
        let end = self.pos.checked_add(n).ok_or("length overflow")?;
        if end > self.data.len() {
            return Err("unexpected EOF".to_string());
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }
    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i64(&mut self) -> Result<i64, String> {
        let b = self.take(8)?;
        Ok(i64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn f64(&mut self) -> Result<f64, String> {
        let b = self.take(8)?;
        Ok(f64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn utf(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        let bytes = self.take(n)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }
    fn new_handle(&mut self) -> usize {
        let idx = self.handles.len();
        self.handles.push(None);
        idx
    }
    fn set_handle(&mut self, idx: usize, v: JsonValue) {
        if idx < self.handles.len() {
            self.handles[idx] = Some(v);
        }
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<JsonValue, String> {
    let mut s = Stream {
        data: bytes,
        pos: 0,
        handles: Vec::new(),
    };
    if s.u16()? != STREAM_MAGIC {
        return Err("not a Java serialized stream (bad magic)".to_string());
    }
    let version = s.u16()?;
    if version != STREAM_VERSION {
        return Err(format!("unsupported Java stream version {version}"));
    }
    let value = s.read_content(0)?;
    let rest = &s.data[s.pos..];
    if !rest.is_empty() {
        return Err(format!(
            "unexpected trailing bytes after Java stream ({} byte(s))",
            rest.len()
        ));
    }
    Ok(value)
}

impl<'a> Stream<'a> {
    fn read_content(&mut self, depth: u32) -> Result<JsonValue, String> {
        if depth > MAX_DEPTH {
            return Err("Java stream exceeds depth limit".to_string());
        }
        let tc = self.byte()?;
        match tc {
            TC_NULL => Ok(JsonValue::Null),
            TC_STRING => {
                let idx = self.new_handle();
                let s = self.utf()?;
                self.set_handle(idx, JsonValue::String(s.clone()));
                Ok(JsonValue::String(s))
            }
            TC_LONGSTRING => {
                let idx = self.new_handle();
                let n = self.i64()?;
                if n < 0 || (n as usize) > MAX_LEN {
                    return Err("invalid Java long-string length".to_string());
                }
                let bytes = self.take(n as usize)?;
                let s = String::from_utf8_lossy(bytes).into_owned();
                self.set_handle(idx, JsonValue::String(s.clone()));
                Ok(JsonValue::String(s))
            }
            TC_REFERENCE => {
                let h = self.u32()?;
                let rel = h
                    .checked_sub(HANDLE_BASE)
                    .ok_or("reference handle below base")? as usize;
                self.handles
                    .get(rel)
                    .and_then(|x| x.as_ref())
                    .cloned()
                    .ok_or_else(|| format!("unresolved Java handle {h}"))
            }
            TC_OBJECT => self.read_object(depth),
            TC_ARRAY => self.read_array(depth),
            TC_CLASS => {
                // A class descriptor as a value: outline its name only.
                let _ = self.new_handle();
                let _desc = self.read_class_desc(depth)?;
                Ok(serde_json::json!({ "$class": "java.lang.Class" }))
            }
            TC_ENUM => Err("Java enum deserialization is not supported".to_string()),
            other => Err(format!("unsupported Java content token 0x{other:02x}")),
        }
    }

    fn read_class_desc(&mut self, depth: u32) -> Result<Option<DescInfo>, String> {
        match self.peek() {
            Some(TC_NULL) => {
                self.byte()?;
                Ok(None)
            }
            Some(TC_REFERENCE) => {
                self.byte()?;
                let _h = self.u32()?;
                // Referenced class descriptors carry only field *shapes*; we do
                // not reconstruct them and cannot read their values reliably.
                Err("referenced Java class descriptor is not supported".to_string())
            }
            Some(TC_CLASSDESC) => {
                self.byte()?;
                let idx = self.new_handle();
                let name = self.utf()?;
                let _svuid = self.i64()?;
                self.set_handle(idx, JsonValue::String(name.clone()));
                let flags = self.byte()?;
                let mut fields = Vec::new();
                if flags & SC_SERIALIZABLE != 0 {
                    let count = self.u16()? as usize;
                    if count > MAX_MEMBERS {
                        return Err("Java class descriptor exceeds field limit".to_string());
                    }
                    for _ in 0..count {
                        let typecode = self.byte()?;
                        let fname = self.utf()?;
                        if typecode == b'L' || typecode == b'[' {
                            // className token (TC_STRING / TC_REFERENCE / TC_NULL)
                            let _ = self.read_content(depth + 1)?;
                        }
                        fields.push((typecode, fname));
                    }
                }
                let has_write = flags & SC_WRITE_METHOD != 0;
                let super_desc = self.read_class_desc(depth)?;
                Ok(Some(DescInfo {
                    name,
                    fields,
                    has_write,
                    super_class: super_desc.map(Box::new),
                }))
            }
            _ => Err("expected Java class descriptor".to_string()),
        }
    }

    fn read_object(&mut self, depth: u32) -> Result<JsonValue, String> {
        let obj_idx = self.new_handle();
        let desc = self
            .read_class_desc(depth)?
            .ok_or_else(|| "object with null class descriptor".to_string())?;

        // Collect the superclass chain base-first: values are written from the
        // most general class to the most derived.
        let mut chain: Vec<&DescInfo> = Vec::new();
        let mut cur: Option<&DescInfo> = Some(&desc);
        while let Some(d) = cur {
            chain.push(d);
            cur = d.super_class.as_deref();
        }
        chain.reverse();

        let mut out = Map::new();
        out.insert("$class".into(), JsonValue::String(desc.name.clone()));
        for class in &chain {
            for (typecode, fname) in &class.fields {
                let v = self.read_field_value(*typecode, depth)?;
                out.insert(fname.clone(), v);
            }
            if class.has_write {
                // writeObject produced block-data annotations; read and discard
                // them as opaque bytes (never materialised as objects).
                let n = self.skip_class_annotation()?;
                if n > 0 {
                    out.insert(
                        format!("$writeObject:{}", class.name),
                        JsonValue::Number(n.into()),
                    );
                }
            }
        }
        let obj = JsonValue::Object(out);
        self.set_handle(obj_idx, obj.clone());
        Ok(obj)
    }

    fn read_field_value(&mut self, typecode: u8, depth: u32) -> Result<JsonValue, String> {
        match typecode {
            b'B' => Ok(JsonValue::from(self.byte()? as i16)),
            b'Z' => Ok(JsonValue::Bool(self.byte()? != 0)),
            b'S' => {
                let b = self.take(2)?;
                Ok(JsonValue::from(i16::from_be_bytes([b[0], b[1]])))
            }
            b'I' => Ok(JsonValue::from(self.u32()? as i32)),
            b'J' => Ok(JsonValue::from(self.i64()?)),
            b'F' => Ok(json_float(self.f32()? as f64)),
            b'D' => Ok(json_float(self.f64()?)),
            b'C' => {
                let c = self.u16()?;
                Ok(JsonValue::String(
                    char::from_u32(c as u32)
                        .map(|c| c.to_string())
                        .unwrap_or_default(),
                ))
            }
            b'L' | b'[' => self.read_content(depth + 1),
            other => Err(format!("unknown Java field type code {:?}", other as char)),
        }
    }

    /// Read block-data until TC_ENDBLOCKDATA, returning how many opaque bytes
    /// were skipped. Never parsed as objects.
    fn skip_class_annotation(&mut self) -> Result<usize, String> {
        let mut skipped = 0usize;
        loop {
            match self.peek() {
                Some(TC_ENDBLOCKDATA) => {
                    self.byte()?;
                    return Ok(skipped);
                }
                Some(TC_BLOCKDATA) => {
                    self.byte()?;
                    let n = self.byte()? as usize;
                    let _ = self.take(n)?;
                    skipped += n;
                }
                Some(TC_BLOCKDATALONG) => {
                    self.byte()?;
                    let n = self.u32()? as usize;
                    let _ = self.take(n)?;
                    skipped += n;
                }
                Some(_) => {
                    // An embedded object value inside the annotation stream.
                    let _ = self.read_content(MAX_DEPTH)?;
                }
                None => return Err("unterminated Java class annotation".to_string()),
            }
        }
    }

    fn read_array(&mut self, depth: u32) -> Result<JsonValue, String> {
        let arr_idx = self.new_handle();
        let desc = self
            .read_class_desc(depth)?
            .ok_or_else(|| "array with null class descriptor".to_string())?;
        let len = self.u32()? as usize;
        if len > MAX_MEMBERS {
            return Err("Java array exceeds length limit".to_string());
        }
        // Component type code is the last char of the descriptor name (e.g. "[I").
        let comp = desc.name.chars().last().unwrap_or('L');
        let mut arr = Vec::with_capacity(len.min(1024));
        for _ in 0..len {
            arr.push(self.read_field_value(comp as u8, depth)?);
        }
        let out = serde_json::json!({ "$array": desc.name, "items": arr });
        self.set_handle(arr_idx, out.clone());
        Ok(out)
    }
}

struct DescInfo {
    name: String,
    fields: Vec<(u8, String)>,
    has_write: bool,
    super_class: Option<Box<DescInfo>>,
}

fn json_float(f: f64) -> JsonValue {
    if f.is_finite() {
        serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)
    } else if f.is_nan() {
        JsonValue::String("NaN".into())
    } else if f.is_sign_negative() {
        JsonValue::String("-Infinity".into())
    } else {
        JsonValue::String("Infinity".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf(s: &str, out: &mut Vec<u8>) {
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    fn header(out: &mut Vec<u8>) {
        out.extend_from_slice(&STREAM_MAGIC.to_be_bytes());
        out.extend_from_slice(&STREAM_VERSION.to_be_bytes());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = Vec::new();
        b.extend_from_slice(&0xBEEFu16.to_be_bytes());
        b.extend_from_slice(&STREAM_VERSION.to_be_bytes());
        assert!(decode(&b).unwrap_err().contains("magic"));
    }

    #[test]
    fn decodes_simple_pojo() {
        // Object of class "Foo" (superclass = null), int field "n"=5, String
        // field "s"="hi". No writeObject, no references.
        let mut b = Vec::new();
        header(&mut b);
        b.push(TC_OBJECT);
        b.push(TC_CLASSDESC);
        utf("Foo", &mut b);
        b.extend_from_slice(&1i64.to_be_bytes()); // serialVersionUID
        b.push(SC_SERIALIZABLE); // flags, no writeObject
        b.extend_from_slice(&2u16.to_be_bytes()); // two fields
        b.push(b'I');
        utf("n", &mut b);
        b.push(b'L');
        utf("s", &mut b);
        b.push(TC_STRING);
        utf("Ljava/lang/String;", &mut b);
        b.push(TC_NULL); // superClass descriptor
                         // field values (base-first = only this class)
        b.extend_from_slice(&5i32.to_be_bytes()); // n
        b.push(TC_STRING);
        utf("hi", &mut b); // s

        let v = decode(&b).unwrap();
        assert_eq!(v["$class"], JsonValue::from("Foo"));
        assert_eq!(v["n"], JsonValue::from(5));
        assert_eq!(v["s"], JsonValue::from("hi"));
    }

    #[test]
    fn decodes_string_root() {
        let mut b = Vec::new();
        header(&mut b);
        b.push(TC_STRING);
        utf("hello", &mut b);
        assert_eq!(decode(&b).unwrap(), JsonValue::from("hello"));
    }

    #[test]
    fn rejects_unknown_token() {
        let mut b = Vec::new();
        header(&mut b);
        b.push(0x01);
        assert!(decode(&b).unwrap_err().contains("unsupported Java content"));
    }
}
