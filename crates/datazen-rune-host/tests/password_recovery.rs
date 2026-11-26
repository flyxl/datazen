use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use datazen::connection_import::decrypt_field;
use datazen_rune_host::secrets::{load_or_create_master_key, KeyBackend};
use serde_json::{json, Value};
use tempfile::tempdir;

fn legacy_store(home: &Path) -> (PathBuf, Value) {
    let data_dir = home.join("datazen-native");
    fs::create_dir(&data_dir).unwrap();
    let path = data_dir.join("connections.json");
    let store = json!({
        "groups": ["Reporting"],
        "connections": [
            {"id": "prod-z", "name": "Warehouse", "databaseType": "postgresql", "group": "Reporting",
             "username": "analyst", "options": {"role": "reader"}, "password": "A".repeat(40)},
            {"id": "prod-a", "name": "Archive", "databaseType": "mysql", "database": "history",
             "pinned": true, "readOnly": true, "password": "B".repeat(40)}
        ]
    });
    fs::write(&path, serde_json::to_vec(&store).unwrap()).unwrap();
    (path, store)
}

fn run(home: &Path, payload: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_datazen-native"))
        .arg("--replace-saved-passwords")
        .env("XDG_DATA_HOME", home)
        .env_remove("DATAZEN_NATIVE_CONNECTIONS")
        .env("DATAZEN_NATIVE_KEYRING", "file")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(payload).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn replaces_ambiguous_passwords_together_in_the_existing_profiles() {
    let home = tempdir().unwrap();
    let (path, before) = legacy_store(home.path());
    let replacements = json!({"prod-a": "first-test-value", "prod-z": "  second test value  "});
    let output = run(home.path(), &serde_json::to_vec(&replacements).unwrap());
    assert!(output.status.success());
    assert!(output.stdout.is_empty() && output.stderr.is_empty());

    let key = load_or_create_master_key(path.parent().unwrap(), KeyBackend::File).unwrap();
    let after: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(before["groups"], after["groups"]);
    let profiles = after["connections"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    for (original, saved) in before["connections"]
        .as_array()
        .unwrap()
        .iter()
        .zip(profiles)
    {
        for (field, value) in original.as_object().unwrap() {
            if field != "password" {
                assert_eq!(&saved[field], value);
            }
        }
        assert_eq!(saved["passwordFormat"], "aes-256-gcm-v1");
        let recovered = decrypt_field(saved["password"].as_str().unwrap(), &key).unwrap();
        assert!(Some(recovered.as_str()) == replacements[saved["id"].as_str().unwrap()].as_str());
    }
}

#[test]
fn unknown_ids_and_invalid_input_leave_the_store_untouched() {
    let home = tempdir().unwrap();
    let (path, _) = legacy_store(home.path());
    let before = fs::read(&path).unwrap();
    for input in [
        r#"{"prod-z":"test-sentinel","unknown":"test-sentinel"}"#,
        r#"{"prod-z":"test-sentinel","prod-a":""}"#,
        r#"{"prod-a":{"unexpected":"test-sentinel"}}"#,
        r#"{"prod-a":"test-sentinel"} {"prod-z":"test-sentinel"}"#,
        r#"["test-sentinel"]"#,
        "{}",
    ] {
        let output = run(home.path(), input.as_bytes());
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let message = String::from_utf8(output.stderr).unwrap();
        assert!(!message.is_empty() && !message.contains("test-sentinel"));
        assert!(fs::read(&path).unwrap() == before);
    }
}
