use super::{AppState, log_err};
use crate::store::AppSettings;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Sha256, Digest};
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn get_groups(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.store.get_groups().await)
}

#[tauri::command]
pub async fn save_groups(state: State<'_, AppState>, groups: Vec<String>) -> Result<(), String> {
    tracing::info!(count = groups.len(), "save_groups");
    state
        .store
        .save_groups(groups)
        .await
        .map_err(|e| log_err("save_groups", &e))
}
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.store.get_settings().await)
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    tracing::debug!(theme = %settings.theme, "save_settings");
    state
        .store
        .save_settings(settings)
        .await
        .map_err(|e| log_err("save_settings", &e))
}

fn derive_key_from_password(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(password.as_bytes());
    let round1 = hasher.finalize();
    // Second round for extra stretching
    let mut hasher2 = Sha256::new();
    hasher2.update(&round1);
    hasher2.update(salt);
    hasher2.update(password.as_bytes());
    let result = hasher2.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

fn encrypt_with_key(plaintext: &str, key: &[u8; 32]) -> Result<String, String> {
    let cipher_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(cipher_key);
    let mut nonce_bytes = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;
    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);
    Ok(BASE64.encode(combined))
}

fn decrypt_with_key(encrypted: &str, key: &[u8; 32]) -> Result<String, String> {
    let combined = BASE64.decode(encrypted)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    if combined.len() < 12 {
        return Err("Invalid encrypted data".to_string());
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(cipher_key);
    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed: wrong password".to_string())?;
    String::from_utf8(plaintext).map_err(|e| format!("UTF-8 decode failed: {}", e))
}

#[tauri::command]
pub async fn export_connections(
    state: State<'_, AppState>,
    path: String,
    password: String,
) -> Result<u32, String> {
    tracing::info!(%path, "export_connections");
    let connections = state.store.get_connections().await;
    let groups = state.store.get_groups().await;
    let count = connections.len() as u32;

    let mut salt = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    let key = derive_key_from_password(&password, &salt);

    let mut export_conns = Vec::new();
    for conn in &connections {
        let mut c = conn.clone();
        if let Some(pw) = &c.password {
            if !pw.is_empty() {
                c.password = Some(encrypt_with_key(pw, &key)
                    .map_err(|e| log_err("export_connections", &e))?);
            }
        }
        if let Some(ref mut ssh) = c.ssh_tunnel {
            if let Some(pw) = &ssh.password {
                if !pw.is_empty() {
                    ssh.password = Some(encrypt_with_key(pw, &key)
                        .map_err(|e| log_err("export_connections", &e))?);
                }
            }
            if let Some(pp) = &ssh.passphrase {
                if !pp.is_empty() {
                    ssh.passphrase = Some(encrypt_with_key(pp, &key)
                        .map_err(|e| log_err("export_connections", &e))?);
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

    let json = serde_json::to_string_pretty(&export_data)
        .map_err(|e| log_err("export_connections", &e))?;

    tokio::fs::write(PathBuf::from(&path), json.as_bytes())
        .await
        .map_err(|e| log_err("export_connections", &e))?;

    tracing::info!(%path, count, "export_connections OK");
    Ok(count)
}

#[tauri::command]
pub async fn import_connections_preview(
    path: String,
    password: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(%path, "import_connections_preview");
    let content = tokio::fs::read_to_string(PathBuf::from(&path))
        .await
        .map_err(|e| log_err("import_connections_preview", &e))?;

    let mut data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| log_err("import_connections_preview", &e))?;

    if data.get("connections").is_none() {
        return Err("Invalid import file: missing 'connections' field".to_string());
    }

    let is_encrypted = data.get("encrypted").and_then(|v| v.as_bool()).unwrap_or(false);

    if is_encrypted {
        let salt_b64 = data.get("salt").and_then(|v| v.as_str()).unwrap_or("");
        let salt = BASE64.decode(salt_b64)
            .map_err(|e| log_err("import_connections_preview", &e))?;
        let key = derive_key_from_password(&password, &salt);

        if let Some(conns) = data.get_mut("connections").and_then(|v| v.as_array_mut()) {
            for conn in conns.iter_mut() {
                if let Some(pw) = conn.get("password").and_then(|v| v.as_str()).map(|s| s.to_string()) {
                    if !pw.is_empty() {
                        let decrypted = decrypt_with_key(&pw, &key)?;
                        conn["password"] = serde_json::Value::String(decrypted);
                    }
                }
                if let Some(ssh) = conn.get_mut("sshTunnel") {
                    if let Some(pw) = ssh.get("password").and_then(|v| v.as_str()).map(|s| s.to_string()) {
                        if !pw.is_empty() {
                            let decrypted = decrypt_with_key(&pw, &key)?;
                            ssh["password"] = serde_json::Value::String(decrypted);
                        }
                    }
                    if let Some(pp) = ssh.get("passphrase").and_then(|v| v.as_str()).map(|s| s.to_string()) {
                        if !pp.is_empty() {
                            let decrypted = decrypt_with_key(&pp, &key)?;
                            ssh["passphrase"] = serde_json::Value::String(decrypted);
                        }
                    }
                }
            }
        }
    }

    Ok(data)
}
