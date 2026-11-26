use super::test_support::sqlite_config;
use super::{desktop_scope, ConnectionRepository, FileConnectionRepository};
use crate::secrets::{load_master_key, KeyBackend};
use datazen::connection_import::encrypt_field;
use futures_executor::block_on;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn open(path: &Path) -> FileConnectionRepository {
    let mut store = FileConnectionRepository::open(path).unwrap();
    store.load_key = |dir, create| load_master_key(dir, KeyBackend::File, create);
    store
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn row(id: &str, password: Option<&str>) -> Value {
    let mut config = sqlite_config(id, None);
    config.password = password.map(String::from);
    serde_json::to_value(config).unwrap()
}

#[test]
fn a_mixed_store_migrates_in_place_and_reopens_without_rewriting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let key = load_master_key(dir.path(), KeyBackend::File, true).unwrap();
    let old_cipher = encrypt_field("pre-tag fixture", &key).unwrap();
    let ambiguous = "A".repeat(40);
    let mut plain = row("plain", Some("short fixture"));
    plain["options"] = json!({"fixture": true});
    plain["driverMetadata"] = json!({"revision": 2});
    let original = json!({
        "connections": [plain, row("old", Some(&old_cipher)), row("ambiguous", Some(&ambiguous)), row("empty", Some("")), row("absent", None)],
        "groups": ["custom"],
        "schemaRevision": 3,
    });
    fs::write(&path, original.to_string()).unwrap();
    let scope = desktop_scope();
    block_on(async {
        let store = open(&path);
        let listed = store.list(&scope).await.unwrap();
        assert!(listed[0].password.as_deref() == Some("short fixture"));
        assert!(listed[1].password.as_deref() == Some("pre-tag fixture"));
        assert!(listed[2].password.is_none());
        assert!(listed[3].password.as_deref() == Some(""));
        assert!(listed[4].password.is_none());
        let migrated = read(&path);
        let rows = &migrated["connections"];
        assert!(rows[0]["password"] != original["connections"][0]["password"]);
        assert!(rows[1]["password"] == old_cipher);
        assert!(rows[0]["passwordFormat"] == "aes-256-gcm-v1");
        assert!(rows[1]["passwordFormat"] == "aes-256-gcm-v1");
        let mut expected = original.clone();
        for i in 0..2 {
            expected["connections"][i]["password"] = rows[i]["password"].clone();
            expected["connections"][i]["passwordFormat"] = rows[i]["passwordFormat"].clone();
        }
        assert!(
            migrated == expected,
            "preserve all other profile and store fields"
        );

        let reopened = open(&path);
        let original_mtime = fs::metadata(&path).unwrap().modified().unwrap();
        assert!(
            reopened
                .get(&scope, "plain")
                .await
                .unwrap()
                .unwrap()
                .password
                .as_deref()
                == Some("short fixture")
        );
        assert!(
            reopened
                .get(&scope, "old")
                .await
                .unwrap()
                .unwrap()
                .password
                .as_deref()
                == Some("pre-tag fixture")
        );
        assert!(reopened.list(&scope).await.unwrap()[2].password.is_none());
        let error = reopened
            .get(&scope, "ambiguous")
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("re-enter") && !error.contains(&ambiguous));
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            original_mtime
        );

        reopened
            .touch_last_connected_at(&scope, "ambiguous")
            .await
            .unwrap();
        reopened
            .reorder(&scope, vec!["ambiguous".into()])
            .await
            .unwrap();
        reopened
            .save_groups(&scope, vec!["custom".into(), "local".into()])
            .await
            .unwrap();
        reopened.delete(&scope, "empty").await.unwrap();
        let changed = read(&path);
        assert!(changed["connections"][0]["password"] == ambiguous);
        assert!(changed["connections"][0].get("passwordFormat").is_none());
        assert!(changed["connections"][0]["lastConnectedAt"].is_string());
        assert!(changed["connections"][1]["password"] == rows[0]["password"]);
        assert!(changed["connections"][1]["driverMetadata"]["revision"] == 2);
    });
}

#[test]
fn legacy_passphrases_and_unicode_migrate_together_on_a_fresh_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let unicode = "兼容中文连接密码".repeat(4);
    let passphrase = "a long multiword fixture that the old encoder could never produce";
    let legacy = json!({"connections": [
        row("unicode", Some(&unicode)), row("passphrase", Some(passphrase)), row("short", Some("fixture")),
    ]});
    fs::write(&path, legacy.to_string()).unwrap();
    block_on(async {
        let scope = desktop_scope();
        let listed = open(&path).list(&scope).await.unwrap();
        assert!(listed[0].password.as_deref() == Some(unicode.as_str()));
        assert!(listed[1].password.as_deref() == Some(passphrase));
        assert!(listed[2].password.as_deref() == Some("fixture"));
        let disk = read(&path);
        assert!(disk["connections"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["passwordFormat"] == "aes-256-gcm-v1"));
        let reopened = open(&path).list(&scope).await.unwrap();
        assert!(reopened
            .iter()
            .zip(&listed)
            .all(|(a, b)| a.password == b.password));
    });
}

#[test]
fn plaintext_migration_retries_after_a_failed_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let original = json!({"connections": [row("plain", Some("retry fixture"))]}).to_string();
    fs::write(&path, &original).unwrap();
    let store = open(&path);
    let obstruction = path.with_added_extension("lock");
    fs::create_dir(&obstruction).unwrap();
    block_on(async {
        let scope = desktop_scope();
        assert!(store.get(&scope, "plain").await.is_err());
        assert!(store.list(&scope).await.is_err());
        assert!(fs::read_to_string(&path).unwrap() == original);
        fs::remove_dir(obstruction).unwrap();
        assert!(
            store
                .get(&scope, "plain")
                .await
                .unwrap()
                .unwrap()
                .password
                .as_deref()
                == Some("retry fixture")
        );
        assert!(read(&path)["connections"][0]["passwordFormat"] == "aes-256-gcm-v1");
        assert!(
            open(&path)
                .get(&scope, "plain")
                .await
                .unwrap()
                .unwrap()
                .password
                .as_deref()
                == Some("retry fixture")
        );
    });
}

#[test]
fn a_missing_or_wrong_key_keeps_old_and_new_ciphertexts_private_and_recoverable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let key_path = dir.path().join(".key");
    let key = load_master_key(dir.path(), KeyBackend::File, true).unwrap();
    let key_file = fs::read(&key_path).unwrap();
    let cipher = encrypt_field("key fixture", &key).unwrap();
    let old = row("old", Some(&cipher));
    let mut tagged = row("tagged", Some(&cipher));
    tagged["passwordFormat"] = json!("aes-256-gcm-v1");
    let mixed = json!({"connections": [old, tagged, row("plain", Some("fixture"))]}).to_string();
    fs::write(&path, &mixed).unwrap();
    fs::remove_file(&key_path).unwrap();
    block_on(async {
        let scope = desktop_scope();
        for wrong_key in [false, true] {
            if wrong_key {
                load_master_key(dir.path(), KeyBackend::File, true).unwrap();
            }
            let store = open(&path);
            assert!(store
                .list(&scope)
                .await
                .unwrap()
                .iter()
                .all(|p| p.password.is_none()));
            for id in ["old", "tagged", "plain"] {
                let error = store.get(&scope, id).await.err().unwrap().to_string();
                assert!(
                    error.contains("unavailable")
                        && !error.contains(&cipher)
                        && !error.contains("fixture")
                );
            }
            let mut extra = sqlite_config("additional", None);
            extra.password = Some("fixture".into());
            assert!(store.save(&scope, extra).await.is_err());
            assert!(fs::read_to_string(&path).unwrap() == mixed);
            assert!(
                key_path.exists() == wrong_key,
                "reads never fabricate a replacement"
            );
        }
        fs::write(&key_path, key_file).unwrap();
        let restored = open(&path);
        let listed = restored.list(&scope).await.unwrap();
        assert!(listed[0].password.as_deref() == Some("key fixture"));
        assert!(listed[1].password.as_deref() == Some("key fixture"));
        assert!(listed[2].password.as_deref() == Some("fixture"));
    });
}

#[test]
fn damaged_and_unknown_formats_require_explicit_replacement() {
    for (format, damaged) in [
        (Some(json!("aes-256-gcm-v1")), "truncated"),
        (Some(json!("aes-256-gcm-v1")), ""),
        (Some(json!("unsupported")), "short fixture"),
        (Some(Value::Null), "invalid metadata"),
        (Some(json!({"version": 1})), "invalid metadata"),
        (None, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA!!="),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("connections.json");
        let _key = load_master_key(dir.path(), KeyBackend::File, true).unwrap();
        let mut record = row("damaged", Some(damaged));
        if let Some(format) = format {
            record["passwordFormat"] = json!(format);
        }
        let original = json!({"connections": [record]}).to_string();
        fs::write(&path, &original).unwrap();
        block_on(async {
            let store = open(&path);
            let scope = desktop_scope();
            assert!(store.get(&scope, "damaged").await.is_err());
            assert!(store.list(&scope).await.unwrap()[0].password.is_none());
            assert!(fs::read_to_string(&path).unwrap() == original);
            let mut replacement = sqlite_config("damaged", None);
            replacement.password = Some("re-entered fixture".into());
            store.save(&scope, replacement).await.unwrap();
            assert!(
                open(&path)
                    .get(&scope, "damaged")
                    .await
                    .unwrap()
                    .unwrap()
                    .password
                    .as_deref()
                    == Some("re-entered fixture")
            );
        });
    }
}

#[test]
fn explicit_recovery_validates_and_commits_the_whole_batch() {
    use std::collections::BTreeMap;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    let mut first = row("first", Some(&"A".repeat(40)));
    first["driverMetadata"] = json!({"revision": 3});
    let mut second = row("second", Some("truncated"));
    second["passwordFormat"] = json!("aes-256-gcm-v1");
    let original = json!({
        "connections": [first, second, row("third", None)], "groups": ["group"],
        "schemaRevision": 2,
    });
    fs::write(&path, original.to_string()).unwrap();
    let store = open(&path);
    let complete = BTreeMap::from([
        ("first".to_owned(), "replacement one".to_owned()),
        ("second".to_owned(), "replacement two".to_owned()),
    ]);

    for invalid in [
        BTreeMap::new(),
        BTreeMap::from([("first".to_owned(), "fixture".to_owned())]),
        BTreeMap::from([("second".to_owned(), String::new())]),
        BTreeMap::from([("unknown".to_owned(), "fixture".to_owned())]),
    ] {
        assert!(store.replace_saved_passwords(&invalid).is_err());
        assert!(read(&path) == original);
        assert!(!dir.path().join(".key").exists());
    }
    let obstruction = path.with_added_extension("lock");
    fs::remove_file(&obstruction).unwrap();
    fs::create_dir(&obstruction).unwrap();
    assert!(store.replace_saved_passwords(&complete).is_err());
    assert!(read(&path) == original);
    fs::remove_dir(obstruction).unwrap();
    store.replace_saved_passwords(&complete).unwrap();
    let recovered = read(&path);
    let mut expected = original;
    for i in 0..2 {
        expected["connections"][i]["password"] = recovered["connections"][i]["password"].clone();
        expected["connections"][i]["passwordFormat"] = json!("aes-256-gcm-v1");
    }
    assert!(
        recovered == expected,
        "only selected credentials may change"
    );
    block_on(async {
        let reopened = open(&path);
        let before = read(&path)["connections"][1].clone();
        let one = BTreeMap::from([("first".into(), "latest fixture".into())]);
        reopened.replace_saved_passwords(&one).unwrap();
        assert!(read(&path)["connections"][1] == before);
        let listed = open(&path).list(&desktop_scope()).await.unwrap();
        assert!(listed[0].password.as_deref() == Some("latest fixture"));
        assert!(listed[1].password.as_deref() == Some("replacement two"));
        assert!(listed[2].password.is_none());
    });
}

#[test]
fn migration_preserves_edits_committed_by_another_open_repository() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    fs::write(
        &path,
        json!({"connections": [row("legacy", Some("old pass"))]}).to_string(),
    )
    .unwrap();
    let first = open(&path);
    let second = open(&path);
    let scope = desktop_scope();
    block_on(async {
        second
            .save(&scope, sqlite_config("new", None))
            .await
            .unwrap();
        second
            .save_groups(&scope, vec!["finance".into()])
            .await
            .unwrap();
        let ids: Vec<_> = first
            .list(&scope)
            .await
            .unwrap()
            .into_iter()
            .map(|cfg| cfg.id)
            .collect();
        assert_eq!(ids, ["legacy", "new"]);
        let saved = read(&path);
        assert!(saved["connections"][0]["passwordFormat"] == "aes-256-gcm-v1");
        assert_eq!(saved["groups"], json!(["finance"]));
        second
            .reorder(&scope, vec!["new".into(), "legacy".into()])
            .await
            .unwrap();
        assert!(read(&path)["connections"][1]["passwordFormat"] == "aes-256-gcm-v1");
    });
}

#[test]
fn sparse_profiles_keep_unknown_nested_settings_through_migration_recovery_and_touch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("connections.json");
    // Deliberately hand-written, like an older store. Serializing a modern
    // ConnectionConfig here would already have filled in every default.
    let mut expected = json!({
        "connections": [{
            "id": "sparse", "name": "Sparse", "databaseType": "sqlite",
            "password": "fixture",
            "sshTunnel": {
                "host": "fixture.invalid", "port": 22, "username": "fixture",
                "futureSetting": {"policy": 7},
                "jump": {"host": "jump.invalid", "port": 22, "username": "fixture", "extensions": ["preserve"]}
            },
            "extension": {"color": 4}
        }, {"id": "other", "name": "Other", "databaseType": "sqlite"}],
        "extra": {"source": 1}
    });
    fs::write(&path, expected.to_string()).unwrap();
    let store = open(&path);
    block_on(async {
        let scope = desktop_scope();
        let mut config = store.get(&scope, "sparse").await.unwrap().unwrap();
        let migrated = read(&path);
        expected["connections"][0]["password"] = migrated["connections"][0]["password"].clone();
        expected["connections"][0]["passwordFormat"] = json!("aes-256-gcm-v1");
        assert!(
            migrated == expected,
            "migration may only change the credential"
        );

        store
            .replace_saved_passwords(&[("sparse".into(), "second fixture".into())].into())
            .unwrap();
        let recovered = read(&path);
        expected["connections"][0]["password"] = recovered["connections"][0]["password"].clone();
        assert!(
            recovered == expected,
            "explicit recovery may only replace the credential"
        );

        store
            .touch_last_connected_at(&scope, "sparse")
            .await
            .unwrap();
        let touched = read(&path);
        expected["connections"][0]["lastConnectedAt"] =
            touched["connections"][0]["lastConnectedAt"].clone();
        assert!(touched == expected, "touch may only update the timestamp");

        config.ssh_tunnel.as_mut().unwrap().port = 2222;
        config.last_connected_at = Some(
            touched["connections"][0]["lastConnectedAt"]
                .as_str()
                .unwrap()
                .into(),
        );
        store.save(&scope, config).await.unwrap();
        let edited = read(&path);
        expected["connections"][0]["password"] = edited["connections"][0]["password"].clone();
        expected["connections"][0]["sshTunnel"]["port"] = json!(2222);
        assert!(
            edited == expected,
            "editing a nested field preserves its unknown siblings"
        );
    });
}
