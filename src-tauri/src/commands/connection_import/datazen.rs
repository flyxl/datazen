use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use super::super::error::{CmdExt, CommandError};
use crate::db::ConnectionConfig;

pub fn derive_argon2_key(password: &str, salt: &[u8]) -> Result<[u8; 32], CommandError> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| CommandError::Internal(format!("Key derivation failed: {e}")))?;
    Ok(key)
}

pub fn encrypt_field(plaintext: &str, key: &[u8; 32]) -> Result<String, CommandError> {
    let cipher_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(cipher_key);
    let mut nonce_bytes = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| CommandError::Internal(format!("Encryption failed: {e}")))?;
    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);
    Ok(BASE64.encode(combined))
}

pub fn decrypt_field(encrypted: &str, key: &[u8; 32]) -> Result<String, CommandError> {
    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| CommandError::Internal(format!("Base64 decode failed: {e}")))?;
    if combined.len() < 12 {
        return Err(CommandError::Validation("Invalid encrypted data".into()));
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(cipher_key);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CommandError::Internal("Decryption failed: wrong password".into()))?;
    String::from_utf8(plaintext)
        .map_err(|e| CommandError::Internal(format!("UTF-8 decode failed: {e}")))
}

pub fn build_encrypted_export(
    connections: &[ConnectionConfig],
    groups: &[String],
    password: &str,
) -> Result<String, CommandError> {
    let mut salt = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    let key = derive_argon2_key(password, &salt)?;

    let mut export_conns = Vec::new();
    for conn in connections {
        let mut c = conn.clone();
        if let Some(pw) = &c.password {
            if !pw.is_empty() {
                c.password = Some(encrypt_field(pw, &key)?);
            }
        }
        if let Some(ref mut ssh) = c.ssh_tunnel {
            if let Some(pw) = &ssh.password {
                if !pw.is_empty() {
                    ssh.password = Some(encrypt_field(pw, &key)?);
                }
            }
            if let Some(pp) = &ssh.passphrase {
                if !pp.is_empty() {
                    ssh.passphrase = Some(encrypt_field(pp, &key)?);
                }
            }
        }
        export_conns.push(c);
    }

    let export_data = serde_json::json!({
        "version": 2,
        "encrypted": true,
        "salt": BASE64.encode(salt),
        "exportedAt": chrono::Utc::now().to_rfc3339(),
        "app": "DataZen",
        "connections": export_conns,
        "groups": groups,
    });

    serde_json::to_string_pretty(&export_data).cmd_err("build_encrypted_export")
}

pub fn decrypt_datazen_fields(
    data: &mut serde_json::Value,
    password: &str,
) -> Result<(), CommandError> {
    let is_encrypted = data
        .get("encrypted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !is_encrypted {
        return Ok(());
    }
    let salt_b64 = data.get("salt").and_then(|v| v.as_str()).unwrap_or("");
    let salt = BASE64
        .decode(salt_b64)
        .map_err(|e| CommandError::Internal(format!("Base64 decode failed: {e}")))?;
    let key = derive_argon2_key(password, &salt)?;

    if let Some(conns) = data.get_mut("connections").and_then(|v| v.as_array_mut()) {
        for conn in conns.iter_mut() {
            if let Some(pw) = conn
                .get("password")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
            {
                if !pw.is_empty() {
                    conn["password"] = serde_json::Value::String(decrypt_field(&pw, &key)?);
                }
            }
            if let Some(ssh) = conn.get_mut("sshTunnel") {
                if let Some(pw) = ssh
                    .get("password")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                {
                    if !pw.is_empty() {
                        ssh["password"] = serde_json::Value::String(decrypt_field(&pw, &key)?);
                    }
                }
                if let Some(pp) = ssh
                    .get("passphrase")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                {
                    if !pp.is_empty() {
                        ssh["passphrase"] = serde_json::Value::String(decrypt_field(&pp, &key)?);
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn parse(
    content: &str,
    password: &str,
) -> Result<(Vec<ConnectionConfig>, Vec<String>), CommandError> {
    let mut data: serde_json::Value = serde_json::from_str(content).cmd_err("datazen parse")?;
    if data.get("connections").is_none() {
        return Err(CommandError::Validation(
            "Invalid import file: missing 'connections' field".into(),
        ));
    }
    decrypt_datazen_fields(&mut data, password)?;
    let connections: Vec<ConnectionConfig> = serde_json::from_value(
        data.get("connections")
            .cloned()
            .ok_or_else(|| CommandError::Validation("missing connections".into()))?,
    )
    .cmd_err("datazen connections")?;
    let groups: Vec<String> = data
        .get("groups")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    Ok((connections, groups))
}
