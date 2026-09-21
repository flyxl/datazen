//! Python `pickle` decoder — a deliberately *restricted* opcode interpreter.
//!
//! Only data-bearing opcodes are understood (None/bool/int/float/str/bytes and
//! the tuple / list / dict constructors plus memoisation). Every opcode that can
//! cause code execution or object construction on the Python side
//! (`GLOBAL`, `STACK_GLOBAL`, `REDUCE`, `INST`, `OBJ`, `NEWOBJ`, `NEWOBJ_EX`,
//! `BUILD`, `PERSID`, `BINPERSID`) is rejected before anything is applied. This
//! module never imports or resolves classes — it produces a JSON value tree.

use serde_json::{Map, Value as JsonValue};

const MAX_LEN: usize = 50 * 1024 * 1024;
const MAX_MEMBERS: usize = 1_000_000;

// Frame/protocol
const OP_PROTO: u8 = 0x80;
const OP_FRAME: u8 = 0x95;
// Marker / control
const OP_MARK: u8 = b'(';
const OP_STOP: u8 = b'.';
const OP_POP: u8 = b'0';
const OP_POP_MARK: u8 = b'1';
const OP_DUP: u8 = b'2';
// Memo (read/write, harmless for data)
const OP_GET: u8 = b'g';
const OP_BINPUT: u8 = b'q';
const OP_LONG_BINPUT: u8 = b'r';
const OP_LONG_BGET: u8 = b'j';
// Scalars
const OP_NONE: u8 = b'N';
const OP_NEWFALSE: u8 = 0x89;
const OP_NEWTRUE: u8 = 0x88;
const OP_INT: u8 = b'I';
const OP_BININT: u8 = b'J';
const OP_BININT1: u8 = b'K';
const OP_BININT2: u8 = b'M';
const OP_LONG: u8 = b'L';
const OP_FLOAT: u8 = b'F';
const OP_BINFLOAT: u8 = b'G';
const OP_BINBYTES: u8 = b'B';
const OP_BINBYTES8: u8 = 0x8e;
const OP_SHORT_BINBYTES: u8 = b'C';
const OP_UNICODE: u8 = b'V';
const OP_BINUNICODE: u8 = b'X';
const OP_BINUNICODE8: u8 = 0x8d;
const OP_SHORT_BINUNICODE: u8 = 0x8c;
// Containers
const OP_EMPTY_TUPLE: u8 = b')';
const OP_TUPLE: u8 = b't';
const OP_TUPLE1: u8 = 0x85;
const OP_TUPLE2: u8 = 0x86;
const OP_TUPLE3: u8 = 0x87;
const OP_EMPTY_LIST: u8 = b']';
const OP_APPEND: u8 = b'a';
const OP_APPENDS: u8 = b'e';
const OP_EMPTY_DICT: u8 = b'}';
const OP_DICT: u8 = b'd';
const OP_SETITEM: u8 = b's';
const OP_SETITEMS: u8 = b'u';
// Executable / construction — always rejected
const OP_GLOBAL: u8 = b'c';
const OP_STACK_GLOBAL: u8 = 0x93;
const OP_REDUCE: u8 = b'R';
const OP_INST: u8 = b'i';
const OP_OBJ: u8 = 0xef;
const OP_NEWOBJ: u8 = 0x81;
const OP_NEWOBJ_EX: u8 = 0xc2;
const OP_BUILD: u8 = b'b';
const OP_PERSID: u8 = b'P';
const OP_BINPERSID: u8 = b'Q';

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
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
    fn until_newline(&mut self) -> Result<&'a [u8], String> {
        let start = self.pos;
        let idx = self.data[start..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("unterminated line payload")?;
        self.pos = start + idx + 1;
        Ok(&self.data[start..start + idx])
    }
    fn le_u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn le_u64(&mut self) -> Result<u64, String> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<JsonValue, String> {
    let mut r = Reader {
        data: bytes,
        pos: 0,
    };
    let mut stack: Vec<StackItem> = Vec::new();
    let mut memo: Vec<Option<JsonValue>> = Vec::new();
    let mut result: Option<JsonValue> = None;

    loop {
        let op = match r.byte() {
            Ok(b) => b,
            Err(_) => {
                if result.is_some() {
                    break;
                }
                return Err("unexpected end of pickle stream".to_string());
            }
        };
        match op {
            OP_PROTO => {
                let _ = r.byte()?; // protocol version, informational only
            }
            OP_FRAME => {
                let _len = r.le_u64()?; // frame length; opcodes parsed sequentially
            }
            OP_MARK => stack.push(StackItem::Mark),
            OP_STOP => {
                result = Some(top_value(&mut stack)?);
                break;
            }
            OP_POP => {
                let _ = top_value(&mut stack)?;
            }
            OP_POP_MARK => {
                pop_to_mark(&mut stack)?;
            }
            OP_DUP => {
                let v = top_value_ref(&stack)?;
                stack.push(StackItem::Val(v));
            }
            OP_NONE => stack.push(StackItem::Val(JsonValue::Null)),
            OP_NEWTRUE => stack.push(StackItem::Val(JsonValue::Bool(true))),
            OP_NEWFALSE => stack.push(StackItem::Val(JsonValue::Bool(false))),
            OP_BININT1 => {
                let b = r.byte()?;
                stack.push(StackItem::Val(JsonValue::from(b)));
            }
            OP_BININT2 => {
                let b = r.take(2)?;
                stack.push(StackItem::Val(JsonValue::from(
                    u16::from_le_bytes([b[0], b[1]]) as i64,
                )));
            }
            OP_BININT => {
                let n = r.le_u32()? as i32;
                stack.push(StackItem::Val(JsonValue::from(n as i64)));
            }
            OP_BINFLOAT => {
                let b = r.take(8)?;
                let f = f64::from_bits(u64::from_be_bytes([
                    b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                ]));
                stack.push(StackItem::Val(json_float(f)));
            }
            OP_SHORT_BINUNICODE => {
                let n = r.byte()? as usize;
                stack.push(StackItem::Val(json_str(r.take(n)?)));
            }
            OP_BINUNICODE => {
                let n = r.le_u32()? as usize;
                stack.push(StackItem::Val(json_str(r.take(n)?)));
            }
            OP_BINUNICODE8 => {
                let n = r.le_u64()? as usize;
                stack.push(StackItem::Val(json_str(r.take(n)?)));
            }
            OP_SHORT_BINBYTES => {
                let n = r.byte()? as usize;
                stack.push(StackItem::Val(json_bytes(r.take(n)?)));
            }
            OP_BINBYTES => {
                let n = r.le_u32()? as usize;
                stack.push(StackItem::Val(json_bytes(r.take(n)?)));
            }
            OP_BINBYTES8 => {
                let n = r.le_u64()? as usize;
                stack.push(StackItem::Val(json_bytes(r.take(n)?)));
            }
            OP_UNICODE => stack.push(StackItem::Val(json_str(r.until_newline()?))),
            OP_INT => stack.push(StackItem::Val(json_int_line(r.until_newline()?))),
            OP_LONG => stack.push(StackItem::Val(json_long_line(r.until_newline()?))),
            OP_FLOAT => stack.push(StackItem::Val(json_float_line(r.until_newline()?))),
            OP_BINPUT => {
                let id = r.byte()? as usize;
                let v = top_value_ref(&stack)?;
                memo_set(&mut memo, id, v);
            }
            OP_LONG_BINPUT => {
                let id = r.le_u32()? as usize;
                let v = top_value_ref(&stack)?;
                memo_set(&mut memo, id, v);
            }
            OP_GET | OP_LONG_BGET => {
                let id = if op == OP_GET {
                    r.byte()? as usize
                } else {
                    r.le_u32()? as usize
                };
                let v = memo
                    .get(id)
                    .and_then(|x| x.as_ref())
                    .cloned()
                    .ok_or_else(|| format!("pickle memo miss at id {id}"))?;
                stack.push(StackItem::Val(v));
            }
            OP_EMPTY_TUPLE => stack.push(StackItem::Val(JsonValue::Array(vec![]))),
            OP_TUPLE1 => {
                let a = top_value(&mut stack)?;
                stack.push(StackItem::Val(JsonValue::Array(vec![a])));
            }
            OP_TUPLE2 => {
                let b = top_value(&mut stack)?;
                let a = top_value(&mut stack)?;
                stack.push(StackItem::Val(JsonValue::Array(vec![a, b])));
            }
            OP_TUPLE3 => {
                let c = top_value(&mut stack)?;
                let b = top_value(&mut stack)?;
                let a = top_value(&mut stack)?;
                stack.push(StackItem::Val(JsonValue::Array(vec![a, b, c])));
            }
            OP_TUPLE => {
                let items = pop_to_mark(&mut stack)?;
                stack.push(StackItem::Val(JsonValue::Array(items)));
            }
            OP_EMPTY_LIST => stack.push(StackItem::Val(JsonValue::Array(vec![]))),
            OP_APPEND => {
                let v = top_value(&mut stack)?;
                append_to_list(&mut stack, vec![v])?;
            }
            OP_APPENDS => {
                let items = pop_to_mark(&mut stack)?;
                append_to_list(&mut stack, items)?;
            }
            OP_EMPTY_DICT => stack.push(StackItem::Val(JsonValue::Object(Map::new()))),
            OP_DICT => stack.push(StackItem::Val(JsonValue::Object(Map::new()))),
            OP_SETITEM => {
                let value = top_value(&mut stack)?;
                let key = top_value(&mut stack)?;
                set_into_dict(&mut stack, key, value)?;
            }
            OP_SETITEMS => {
                let items = pop_to_mark(&mut stack)?;
                setmany_into_dict(&mut stack, items)?;
            }
            OP_GLOBAL | OP_STACK_GLOBAL | OP_REDUCE | OP_INST | OP_OBJ | OP_NEWOBJ
            | OP_NEWOBJ_EX | OP_BUILD | OP_PERSID | OP_BINPERSID => {
                return Err(
                    "pickle contains executable object opcodes — refusing to parse".to_string(),
                );
            }
            other => {
                return Err(format!("unsupported pickle opcode 0x{other:02x}"));
            }
        }
    }

    result.ok_or_else(|| "pickle stream produced no value".to_string())
}

enum StackItem {
    Mark,
    Val(JsonValue),
}

fn top_value(stack: &mut Vec<StackItem>) -> Result<JsonValue, String> {
    match stack.pop() {
        Some(StackItem::Val(v)) => Ok(v),
        Some(StackItem::Mark) => Err("pickle mark/unmark mismatch".to_string()),
        None => Err("pickle stack underflow".to_string()),
    }
}

fn top_value_ref(stack: &[StackItem]) -> Result<JsonValue, String> {
    match stack.last() {
        Some(StackItem::Val(v)) => Ok(v.clone()),
        Some(StackItem::Mark) => Err("expected value, found mark".to_string()),
        None => Err("pickle stack underflow".to_string()),
    }
}

fn pop_to_mark(stack: &mut Vec<StackItem>) -> Result<Vec<JsonValue>, String> {
    let mut items = Vec::new();
    loop {
        match stack.pop() {
            Some(StackItem::Mark) => {
                items.reverse();
                return Ok(items);
            }
            Some(StackItem::Val(v)) => {
                items.push(v);
                if items.len() > MAX_MEMBERS {
                    return Err("pickle collection exceeds member limit".to_string());
                }
            }
            None => return Err("pickle mark not found".to_string()),
        }
    }
}

fn memo_set(memo: &mut Vec<Option<JsonValue>>, id: usize, value: JsonValue) {
    if id >= memo.len() {
        memo.resize(id + 1, None);
    }
    memo[id] = Some(value);
}

fn append_to_list(stack: &mut Vec<StackItem>, items: Vec<JsonValue>) -> Result<(), String> {
    // The target list is the value just below the (now-popped) mark region.
    let target = stack.last_mut().ok_or("append on empty stack")?;
    if let StackItem::Val(JsonValue::Array(arr)) = target {
        arr.extend(items);
        Ok(())
    } else {
        Err("APPEND target is not a list".to_string())
    }
}

fn set_into_dict(
    stack: &mut Vec<StackItem>,
    key: JsonValue,
    value: JsonValue,
) -> Result<(), String> {
    let target = stack.last_mut().ok_or("setitem on empty stack")?;
    if let StackItem::Val(JsonValue::Object(map)) = target {
        map.insert(key_to_string(&key), value);
        Ok(())
    } else {
        Err("SETITEM target is not a dict".to_string())
    }
}

fn setmany_into_dict(stack: &mut Vec<StackItem>, items: Vec<JsonValue>) -> Result<(), String> {
    let target = stack.last_mut().ok_or("setitems on empty stack")?;
    if let StackItem::Val(JsonValue::Object(map)) = target {
        let mut it = items.into_iter();
        while let Some(k) = it.next() {
            let v = it
                .next()
                .ok_or("SETITEMS received an odd number of items")?;
            map.insert(key_to_string(&k), v);
        }
        Ok(())
    } else {
        Err("SETITEMS target is not a dict".to_string())
    }
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

fn json_str(bytes: &[u8]) -> JsonValue {
    JsonValue::String(String::from_utf8_lossy(bytes).into_owned())
}

fn json_bytes(bytes: &[u8]) -> JsonValue {
    use base64::Engine;
    JsonValue::String(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn json_float(f: f64) -> JsonValue {
    if f.is_finite() {
        serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)
    } else {
        JsonValue::String(f.to_string())
    }
}

fn json_int_line(b: &[u8]) -> JsonValue {
    match String::from_utf8_lossy(b).trim().parse::<i64>() {
        Ok(n) => JsonValue::from(n),
        Err(_) => JsonValue::String(String::from_utf8_lossy(b).into_owned()),
    }
}

fn json_long_line(b: &[u8]) -> JsonValue {
    let s = String::from_utf8_lossy(b)
        .trim()
        .trim_end_matches('L')
        .to_string();
    match s.parse::<i64>() {
        Ok(n) => JsonValue::from(n),
        Err(_) => JsonValue::String(s),
    }
}

fn json_float_line(b: &[u8]) -> JsonValue {
    match String::from_utf8_lossy(b).trim().parse::<f64>() {
        Ok(f) => json_float(f),
        Err(_) => JsonValue::String(String::from_utf8_lossy(b).into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(bytes: &mut Vec<u8>, op: u8) {
        bytes.push(op);
    }

    #[test]
    fn decodes_dict_of_scalars() {
        // {"a": 7, "b": [true, None]}
        let mut b = Vec::new();
        push(&mut b, OP_PROTO);
        b.push(5);
        push(&mut b, OP_FRAME);
        b.extend_from_slice(&100u64.to_le_bytes());
        push(&mut b, OP_EMPTY_DICT);
        push(&mut b, OP_MARK);
        push(&mut b, OP_SHORT_BINUNICODE);
        b.push(1);
        b.push(b'a');
        push(&mut b, OP_BININT1);
        b.push(7);
        push(&mut b, OP_SHORT_BINUNICODE);
        b.push(1);
        b.push(b'b');
        push(&mut b, OP_EMPTY_LIST);
        push(&mut b, OP_MARK);
        push(&mut b, OP_NEWTRUE);
        push(&mut b, OP_NONE);
        push(&mut b, OP_APPENDS);
        push(&mut b, OP_SETITEMS);
        push(&mut b, OP_STOP);

        let v = decode(&b).unwrap();
        assert_eq!(v["a"], JsonValue::from(7));
        assert_eq!(v["b"][0], JsonValue::Bool(true));
        assert!(v["b"][1].is_null());
    }

    #[test]
    fn rejects_global_opcode() {
        let mut b = Vec::new();
        push(&mut b, OP_PROTO);
        b.push(2);
        push(&mut b, OP_GLOBAL); // os
        b.extend_from_slice(b"os\nsystem\n");
        push(&mut b, OP_STOP);
        let err = decode(&b).unwrap_err();
        assert!(err.contains("executable"), "got: {err}");
    }

    #[test]
    fn rejects_reduce_opcode() {
        let mut b = Vec::new();
        push(&mut b, OP_STACK_GLOBAL);
        b.push(b'\xff');
        push(&mut b, OP_REDUCE);
        let err = decode(&b).unwrap_err();
        assert!(err.contains("executable"), "got: {err}");
    }

    #[test]
    fn tuple_scalars() {
        let mut b = Vec::new();
        push(&mut b, OP_PROTO);
        b.push(4);
        push(&mut b, OP_BININT1);
        b.push(1);
        push(&mut b, OP_SHORT_BINUNICODE);
        b.push(1);
        b.push(b'x');
        push(&mut b, OP_TUPLE2);
        push(&mut b, OP_STOP);
        let v = decode(&b).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);
        assert_eq!(v[0], JsonValue::from(1));
        assert_eq!(v[1], JsonValue::from("x"));
    }
}
