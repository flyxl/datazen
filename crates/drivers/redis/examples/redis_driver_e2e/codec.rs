use std::io::Write;

use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::{
    write::{DeflateEncoder, GzEncoder, ZlibEncoder},
    Compression,
};
use serde_json::{json, Value};

use super::Journey;

async fn round_trip(journey: &mut Journey<'_>, codec: &str, stored: &[u8]) -> Value {
    let key = format!("contract:codec:{codec}");
    let encoded = STANDARD.encode(stored);
    let response = journey
        .command("set_string_raw", json!({"key":key,"dataB64":encoded}))
        .await;
    assert_eq!(response["ok"], true);
    let frame = journey.command("get_key_raw", json!({"key":key})).await;
    assert_eq!(frame["rawB64"], encoded);
    let witness: Vec<u8> = journey.read("GET", &[&key]).await;
    assert_eq!(witness, stored);
    journey
        .command(
            "decode_value",
            json!({"codec":codec,"data":frame["rawB64"]}),
        )
        .await
}

fn java_utf(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn java_primitive_record() -> Vec<u8> {
    let mut out = vec![0xac, 0xed, 0, 5, 0x73, 0x72];
    java_utf(&mut out, "Record");
    out.extend_from_slice(&1_i64.to_be_bytes());
    out.extend_from_slice(&[2, 0, 10]);
    for (kind, name) in [
        (b'B', "byte"),
        (b'C', "char"),
        (b'D', "double"),
        (b'F', "float"),
        (b'I', "int"),
        (b'J', "long"),
        (b'S', "short"),
        (b'Z', "flag"),
    ] {
        out.push(kind);
        java_utf(&mut out, name);
    }
    out.push(b'L');
    java_utf(&mut out, "one");
    out.push(0x74);
    java_utf(&mut out, "Ljava/lang/String;");
    out.push(b'L');
    java_utf(&mut out, "two");
    out.push(0x71);
    out.extend_from_slice(&0x7e0001_u32.to_be_bytes());
    out.extend_from_slice(&[0x78, 0x70, (-3_i8) as u8]);
    out.extend_from_slice(&u16::from(b'Q').to_be_bytes());
    out.extend_from_slice(&f64::from(2.25).to_be_bytes());
    out.extend_from_slice(&f32::NEG_INFINITY.to_be_bytes());
    out.extend_from_slice(&256_i32.to_be_bytes());
    out.extend_from_slice(&(-70000_i64).to_be_bytes());
    out.extend_from_slice(&(-42_i16).to_be_bytes());
    out.extend_from_slice(&[1, 0x74]);
    java_utf(&mut out, "memoized");
    out.push(0x71);
    out.extend_from_slice(&0x7e0003_u32.to_be_bytes());
    out
}

pub async fn run(journey: &mut Journey<'_>) {
    let json_document = br#"{"fixture":[1,2,3]}"#;
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(json_document).unwrap();
    let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
    zlib.write_all(json_document).unwrap();
    let mut deflate = DeflateEncoder::new(Vec::new(), Compression::default());
    deflate.write_all(json_document).unwrap();

    for (codec, stored) in [
        ("none", json_document.to_vec()),
        ("base64", STANDARD.encode(json_document).into_bytes()),
        ("gzip", gzip.finish().unwrap()),
        ("zlib", zlib.finish().unwrap()),
        ("deflate", deflate.finish().unwrap()),
    ] {
        let result = round_trip(journey, codec, &stored).await;
        assert_eq!(result["ok"], true, "{codec}: {result}");
        assert_eq!(result["kind"], "bytes");
        assert_eq!(result["data"], STANDARD.encode(json_document));
        let parsed: Value = serde_json::from_str(result["json"].as_str().unwrap()).unwrap();
        assert_eq!(parsed, json!({"fixture":[1,2,3]}));
    }

    // Standalone data-only payloads: no imports, class initialization or invocation.
    let mut java_string = vec![0xac, 0xed, 0, 5, 0x74, 0, 7];
    java_string.extend_from_slice(b"Fixture");

    // Java Object Serialization stream protocol §6.4: one ordinary object
    // and one primitive array, with empty class annotations.
    let mut java_object = vec![0xac, 0xed, 0, 5, 0x73, 0x72, 0, 7];
    java_object.extend_from_slice(b"Fixture");
    java_object.extend_from_slice(&1_i64.to_be_bytes());
    java_object.extend_from_slice(&[2, 0, 1, b'I', 0, 1, b'x', 0x78, 0x70]);
    java_object.extend_from_slice(&7_i32.to_be_bytes());
    let mut java_array = vec![0xac, 0xed, 0, 5, 0x75, 0x72, 0, 2, b'[', b'I'];
    java_array.extend_from_slice(&0x4dba602676eab2a5_i64.to_be_bytes());
    java_array.extend_from_slice(&[2, 0, 0, 0x78, 0x70]);
    java_array.extend_from_slice(&2_i32.to_be_bytes());
    java_array.extend_from_slice(&7_i32.to_be_bytes());
    java_array.extend_from_slice(&(-11_i32).to_be_bytes());
    let structured = [
        (
            "msgpack",
            vec![0x82, 0xa1, b'x', 7, 0xa1, b'b', 0x92, 0xc3, 0xc0],
            json!({"x":7,"b":[true,null]}),
        ),
        (
            "php",
            b"a:2:{s:1:\"x\";i:7;i:0;b:1;}".to_vec(),
            json!({"x":7,"0":true}),
        ),
        (
            "pickle",
            vec![0x80, 4, b'K', 7, 0x8c, 1, b'x', 0x86, b'.'],
            json!([7, "x"]),
        ),
        ("java", java_string, json!("Fixture")),
        ("java", java_object, json!({"$class":"Fixture","x":7})),
        ("java", java_array, json!({"$array":"[I","items":[7,-11]})),
        (
            "java",
            java_primitive_record(),
            json!({
                "$class":"Record","byte":-3,"char":"Q","double":2.25,
                "float":"-Infinity","int":256,"long":-70000,"short":-42,
                "flag":true,"one":"memoized","two":"memoized"
            }),
        ),
    ];
    for (codec, stored, expected) in structured {
        let result = round_trip(journey, codec, &stored).await;
        assert_eq!(result["ok"], true, "{codec}: {result}");
        assert_eq!(result["kind"], "json");
        let parsed: Value = serde_json::from_str(result["json"].as_str().unwrap()).unwrap();
        assert_eq!(parsed, expected, "{codec}");
    }
    let fixtures: Vec<Value> =
        serde_json::from_slice(&std::fs::read("pickle-fixtures.json").unwrap()).unwrap();
    assert!(!fixtures.is_empty());
    for (i, case) in fixtures.into_iter().enumerate() {
        let bytes = STANDARD.decode(case["data"].as_str().unwrap()).unwrap();
        let decoded = round_trip(journey, "pickle", &bytes).await;
        assert_eq!(decoded["ok"], true, "pickle fixture {i}: {decoded}");
        let value: Value = serde_json::from_str(decoded["json"].as_str().unwrap()).unwrap();
        assert_eq!(value, case["expected"], "pickle fixture {i}");
    }

    for (codec, stored) in [
        ("gzip", b"plain text".as_slice()),
        ("msgpack", b"not MessagePack".as_slice()),
        ("php", b"unrecognized PHP payload".as_slice()),
        ("java", b"not Java".as_slice()),
    ] {
        let refused = round_trip(journey, codec, stored).await;
        assert_eq!(refused["ok"], false, "{codec}: {refused}");
        assert_eq!(refused["reason"], "decode-failed");
    }
}
