//! Credential storage on disk: the master key, AES-256-GCM ciphertext, and
//! what happens when the ciphertext on disk is not what we wrote.
//!
//! A password that cannot be decrypted is dropped rather than surfaced, so the
//! user is asked for it again instead of being shown a value that is not the one
//! they saved. These tests pin that behaviour, because a silent fallback that
//! *keeps* the garbage would look like a successful load.

use super::super::*;
use super::fixtures::*;

#[tokio::test]
async fn file_backend_creates_and_reloads_key() {
    use_file_key_backend();
    let dir = tempfile::tempdir().unwrap();
    let k1 = Store::get_or_create_encryption_key_for_test(dir.path())
        .await
        .unwrap();
    let k2 = Store::get_or_create_encryption_key_for_test(dir.path())
        .await
        .unwrap();
    assert_eq!(k1, k2);
    assert!(key_store::key_file_path(dir.path()).is_file());
}

#[tokio::test]
#[ignore = "requires OS keychain; run with: DATAZEN_TEST_KEYRING=1 cargo test migrates_dot_key -- --ignored"]
async fn migrates_dot_key_into_keyring_and_deletes_file() {
    std::env::remove_var("DATAZEN_KEYRING");
    if !key_store::keyring_is_available() {
        eprintln!("skip: OS keychain unavailable (needs DATAZEN_TEST_KEYRING=1)");
        return;
    }
    key_store::delete_keyring_entry_for_test();

    let dir = tempfile::tempdir().unwrap();
    let known_key = [7u8; 32];
    std::fs::write(
        key_store::key_file_path(dir.path()),
        BASE64.encode(known_key),
    )
    .unwrap();

    let loaded = key_store::load_or_create_master_key(dir.path()).unwrap();
    assert_eq!(loaded, known_key);
    assert!(!key_store::key_file_path(dir.path()).exists());

    key_store::delete_keyring_entry_for_test();
}

#[tokio::test]
async fn ssh_credentials_encrypted_on_disk_and_decrypted_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;

    store
        .save_connection(sample_connection_with_ssh())
        .await
        .unwrap();

    let disk_content = tokio::fs::read_to_string(dir.path().join("connections.json"))
        .await
        .unwrap();
    assert!(
        !disk_content.contains("ssh-secret-password"),
        "SSH password must not appear plaintext on disk"
    );
    assert!(
        !disk_content.contains("key-passphrase"),
        "SSH passphrase must not appear plaintext on disk"
    );

    let loaded = store.get_connections().await;
    assert_eq!(loaded.len(), 1);
    let ssh = loaded[0].ssh_tunnel.as_ref().unwrap();
    assert_eq!(ssh.password.as_deref(), Some("ssh-secret-password"));
    assert_eq!(ssh.passphrase.as_deref(), Some("key-passphrase"));
}

#[tokio::test]
async fn ssh_credentials_roundtrip_after_reload() {
    let dir = tempfile::tempdir().unwrap();

    {
        let store = init_store_for_test(dir.path()).await;
        store
            .save_connection(sample_connection_with_ssh())
            .await
            .unwrap();
    }

    let store = init_store_for_test(dir.path()).await;
    let loaded = store.get_connections().await;
    assert_eq!(loaded.len(), 1);
    let ssh = loaded[0].ssh_tunnel.as_ref().unwrap();
    assert_eq!(ssh.password.as_deref(), Some("ssh-secret-password"));
    assert_eq!(ssh.passphrase.as_deref(), Some("key-passphrase"));
}

#[tokio::test]
async fn reload_clears_bad_encrypted_password() {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = init_store_for_test(dir.path()).await;
        store
            .save_connection(sample_connection_with_ssh())
            .await
            .unwrap();
    }

    let mut raw: serde_json::Value = serde_json::from_str(
        &tokio::fs::read_to_string(dir.path().join("connections.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    raw[0]["password"] = serde_json::json!("not-valid-ciphertext");
    tokio::fs::write(
        dir.path().join("connections.json"),
        serde_json::to_string_pretty(&raw).unwrap(),
    )
    .await
    .unwrap();

    let store = init_store_for_test(dir.path()).await;
    let conn = store.get_connections().await.into_iter().next().unwrap();
    assert!(conn.password.is_none());
}

#[tokio::test]
async fn reload_clears_bad_encrypted_ssh_password() {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = init_store_for_test(dir.path()).await;
        store
            .save_connection(sample_connection_with_ssh())
            .await
            .unwrap();
    }

    let mut raw: serde_json::Value = serde_json::from_str(
        &tokio::fs::read_to_string(dir.path().join("connections.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    raw[0]["sshTunnel"]["password"] = serde_json::json!("not-valid-ciphertext");
    tokio::fs::write(
        dir.path().join("connections.json"),
        serde_json::to_string_pretty(&raw).unwrap(),
    )
    .await
    .unwrap();

    let store = init_store_for_test(dir.path()).await;
    let conn = store.get_connections().await.into_iter().next().unwrap();
    let ssh = conn.ssh_tunnel.as_ref().unwrap();
    assert!(ssh.password.is_none());
}

#[test]
fn decrypt_rejects_short_payload() {
    use_file_key_backend();
    let dir = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let store = rt.block_on(Store::init_with_path(dir.path())).unwrap();
    let err = store.decrypt_password("AAAA").unwrap_err();
    assert!(matches!(err, StoreError::EncryptionError(_)));
}

#[tokio::test]
async fn store_data_dir_and_encryption_key_b64() {
    let dir = tempfile::tempdir().unwrap();
    let store = init_store_for_test(dir.path()).await;
    assert_eq!(store.data_dir(), &dir.path().to_path_buf());
    let b64 = store.encryption_key_b64();
    assert!(!b64.is_empty());
    let roundtrip = store
        .decrypt_password(&store.encrypt("secret").unwrap())
        .unwrap();
    assert_eq!(roundtrip, "secret");
}
