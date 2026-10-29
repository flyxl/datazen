//! Tests for the encryption key export.

use super::*;
#[test]
fn encryption_key_export_bytes_trims() {
    let bytes = encryption_key_export_bytes("  abc==  \n");
    assert_eq!(bytes, b"abc==");
}

#[test]
fn encryption_key_export_bytes_roundtrip_write() {
    let key_b64 = "  dGVzdGtleQ==  \n";
    let bytes = encryption_key_export_bytes(key_b64);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("datazen.key");
    std::fs::write(&path, &bytes).unwrap();
    let read_back = std::fs::read_to_string(&path).unwrap();
    assert_eq!(read_back, "dGVzdGtleQ==");
}
