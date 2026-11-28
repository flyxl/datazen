//! Native file adapter for saved HTTP proxy definitions. Its JSON records use
//! the existing saved-tunnel contract; other records and untouched fields
//! survive edits verbatim at the value level. No transport is opened here.
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
};

use datazen::connection_import::{decrypt_field, encrypt_field};
use datazen_ui_api::{
    types::SavedTunnelSummary, HttpProxyTunnelConfig, NativeHttpProxyDefinition,
    NativeTunnelPassword, NativeTunnelStoreError as Error, SavedTunnelKind,
};
use serde::Deserialize;
use serde_json::{Map, Value};

pub(crate) struct NativeTunnelFile {
    path: PathBuf,
    keys: crate::secrets::KeyBackend,
}

impl NativeTunnelFile {
    pub(crate) fn new(data: &Path) -> Self {
        Self {
            path: data.join("tunnels.json"),
            keys: crate::secrets::resolve_key_backend(),
        }
    }

    fn document(&self) -> Result<Vec<Value>, Error> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(Error::Unreadable),
        };
        let rows: Vec<Value> = serde_json::from_slice(&bytes).map_err(|_| Error::Unreadable)?;
        let mut ids = HashSet::new();
        for row in &rows {
            let summary = summary(row)?;
            if !ids.insert(summary.id) {
                return Err(Error::Unreadable);
            }
        }
        Ok(rows)
    }

    pub(crate) fn summaries(&self) -> Result<Vec<SavedTunnelSummary>, Error> {
        // This path does not unlock the keyring or read any decrypted row.
        self.document()?.iter().map(summary).collect()
    }

    pub(crate) fn proxy(&self, id: &str) -> Result<NativeHttpProxyDefinition, Error> {
        let rows = self.document()?;
        let row = rows.iter()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
            .ok_or(Error::MissingOrUnsupported)?;
        let s = summary(row)?;
        let raw = proxy_fields(row, s.kind)?;
        let mut public = Map::new();
        for name in ["enabled", "host", "port", "scheme", "username", "connectTimeoutSecs"] {
            if let Some(value) = raw.get(name) {
                public.insert(name.to_owned(), value.clone());
            }
        }
        public.entry("enabled".to_owned()).or_insert(Value::Bool(true));
        let proxy = HttpProxyTunnelConfig::deserialize(Value::Object(public))
            .map_err(|_| Error::Unreadable)?;
        Ok(NativeHttpProxyDefinition {
            id: s.id, name: s.name, proxy,
            password_configured: field_password(raw)?.is_some(),
        })
    }

    pub(crate) fn save_proxy(
        &self,
        id: Option<&str>,
        name: &str,
        proxy: HttpProxyTunnelConfig,
        password: NativeTunnelPassword,
        no_protected_connections: impl FnOnce() -> bool,
    ) -> Result<(), Error> {
        let name = name.trim();
        if name.is_empty() || proxy.host.trim().is_empty() || proxy.port == 0
            || proxy.connect_timeout_secs == Some(0)
            || proxy.password.is_some() || proxy.headers.is_some()
        {
            return Err(Error::InvalidDefinition);
        }
        if matches!(&password, NativeTunnelPassword::Replace(value) if value.is_empty()) {
            return Err(Error::InvalidDefinition);
        }
        let parent = self.path.parent().ok_or(Error::Unwritable)?;
        fs::create_dir_all(parent).map_err(|_| Error::Unwritable)?;
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(self.path.with_added_extension("lock")).map_err(|_| Error::Unwritable)?;
        lock.try_lock().map_err(|_| Error::Unwritable)?;
        let mut rows = self.document()?;
        let index = id.map(|id| {
            rows.iter().position(|row| row.get("id").and_then(Value::as_str) == Some(id))
                .ok_or(Error::MissingOrUnsupported)
        }).transpose()?;
        if let Some(i) = index {
            field_password(proxy_fields(&rows[i], summary(&rows[i])?.kind)?)?;
        }
        let encrypted = match password {
            NativeTunnelPassword::Keep => None,
            NativeTunnelPassword::Clear => Some(None),
            NativeTunnelPassword::Replace(value) => {
                let mut ciphertexts = Vec::new();
                for row in &rows {
                    collect_ciphertexts(row, &mut ciphertexts)?;
                }
                let key = match crate::secrets::load_master_key(parent, self.keys, false) {
                    Ok(key) => key,
                    Err(_) if ciphertexts.is_empty() && no_protected_connections() =>
                        crate::secrets::load_master_key(parent, self.keys, true)
                            .map_err(|_| Error::Credential)?,
                    Err(_) => return Err(Error::Credential),
                };
                // A mismatched key may read but must not split the store into
                // records requiring different keys. Plaintext stays in-memory.
                if ciphertexts.iter().any(|cipher| decrypt_field(cipher, &key).is_err()) {
                    return Err(Error::Credential);
                }
                Some(Some(encrypt_field(&value, &key).map_err(|_| Error::Credential)?))
            }
        };

        let mut created = serde_json::json!({
            "kind": "httpProxy", "id": uuid::Uuid::new_v4().to_string(), "httpProxy": {"enabled": true},
        });
        let row = match index {
            Some(i) => &mut rows[i],
            None => &mut created,
        };
        let record = row.as_object_mut().ok_or(Error::Unreadable)?;
        record.insert("name".into(), Value::String(name.to_owned()));
        let fields = record.get_mut("httpProxy").and_then(Value::as_object_mut)
            .ok_or(Error::Unreadable)?;
        fields.insert("host".into(), Value::String(proxy.host.trim().to_owned()));
        fields.insert("port".into(), proxy.port.into());
        fields.insert("scheme".into(), serde_json::to_value(proxy.scheme).map_err(|_| Error::InvalidDefinition)?);
        replace_optional(fields, "username", proxy.username.map(Value::String));
        replace_optional(fields, "connectTimeoutSecs", proxy.connect_timeout_secs.map(Value::from));
        if let Some(password) = encrypted {
            replace_optional(fields, "password", password.map(Value::String));
        }
        if index.is_none() {
            rows.push(created);
        }
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|_| Error::Unwritable)?;
        serde_json::to_writer_pretty(temp.as_file_mut(), &rows).map_err(|_| Error::Unwritable)?;
        temp.as_file().sync_all().map_err(|_| Error::Unwritable)?;
        temp.persist(&self.path).map_err(|_| Error::Unwritable)?;
        Ok(())
    }
}

fn summary(row: &Value) -> Result<SavedTunnelSummary, Error> {
    let s = SavedTunnelSummary::deserialize(row).map_err(|_| Error::Unreadable)?;
    if s.id.trim().is_empty() || s.name.trim().is_empty() {
        return Err(Error::Unreadable);
    }
    Ok(s)
}

fn proxy_fields(row: &Value, kind: SavedTunnelKind) -> Result<&Map<String, Value>, Error> {
    if kind != SavedTunnelKind::HttpProxy {
        return Err(Error::MissingOrUnsupported);
    }
    row.get("httpProxy").and_then(Value::as_object).ok_or(Error::Unreadable)
}

fn field_password(fields: &Map<String, Value>) -> Result<Option<&str>, Error> {
    match fields.get("password") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        _ => Err(Error::Unreadable),
    }
}

fn collect_ciphertexts<'a>(v: &'a Value, out: &mut Vec<&'a str>) -> Result<(), Error> {
    if let Some(object) = v.as_object() {
        for (key, value) in object {
            match key.as_str() {
                "password" | "authToken" | "passphrase" => {
                    if value.is_null() {
                        continue;
                    }
                    let data = value.as_str().ok_or(Error::Unreadable)?;
                    if !data.is_empty() {
                        out.push(data);
                    }
                }
                _ if key != "headers" => collect_ciphertexts(value, out)?,
                _ => (),
            }
        }
    }
    Ok(())
}

fn replace_optional(fields: &mut Map<String, Value>, name: &str, value: Option<Value>) {
    match value {
        Some(value) => { fields.insert(name.to_owned(), value); }
        None => { fields.remove(name); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_ui_api::HttpProxyScheme;

    fn store(root: &Path) -> NativeTunnelFile {
        NativeTunnelFile { path: root.join("tunnels.json"), keys: crate::secrets::KeyBackend::File }
    }

    fn proxy(port: u16) -> HttpProxyTunnelConfig {
        HttpProxyTunnelConfig {
            enabled: true, host: "127.0.0.1".to_owned(), port, scheme: HttpProxyScheme::Http,
            username: Some("fixture-user".to_owned()), password: None,
            headers: None, connect_timeout_secs: Some(30),
        }
    }

    #[test]
    fn create_reopen_edit_and_clear_preserve_ciphertext_and_other_records() {
        let root = tempfile::tempdir().unwrap();
        let backend = store(root.path());
        let key = crate::secrets::load_master_key(root.path(), backend.keys, true).unwrap();
        let encoded = encrypt_field("other fixture secret", &key).unwrap();
        let legacy = serde_json::json!({
            "id":"other", "name":"Legacy SSH", "kind":"ssh",
            "ssh":{"password":encoded}, "future":{"flag":true},
        });
        fs::write(&backend.path, serde_json::to_vec(&[&legacy]).unwrap()).unwrap();

        backend.save_proxy(None, " New Proxy ", proxy(18080),
            NativeTunnelPassword::Replace("fixture proxy secret".into()), || false).unwrap();
        let summaries = backend.summaries().unwrap();
        assert_eq!(summaries.len(), 2);
        let id = &summaries[1].id;
        assert_eq!(summaries[1].name, "New Proxy");
        assert_eq!(serde_json::to_value(&summaries[1]).unwrap(),
            serde_json::json!({"id":id,"name":"New Proxy","kind":"httpProxy"}));

        let disk = backend.document().unwrap();
        assert_eq!(disk[0], legacy);
        let cipher = disk[1]["httpProxy"]["password"].as_str().unwrap();
        assert_ne!(cipher, "fixture proxy secret");
        assert_eq!(decrypt_field(cipher, &key).unwrap(), "fixture proxy secret");

        let fresh = store(root.path());
        let saved = fresh.proxy(id).unwrap();
        assert_eq!(saved.proxy, proxy(18080));
        assert!(saved.password_configured && saved.proxy.password.is_none());
        assert_eq!(fresh.proxy("other"), Err(Error::MissingOrUnsupported));
        fresh.save_proxy(Some(id), "Rename", proxy(18443), NativeTunnelPassword::Keep, || false).unwrap();
        let changed = fresh.document().unwrap();
        assert_eq!(changed[0], legacy);
        assert_eq!(changed[1]["httpProxy"]["password"], cipher);
        assert_eq!(fresh.proxy(id).unwrap().proxy.port, 18443);

        fresh.save_proxy(Some(id), "Rename", proxy(18443), NativeTunnelPassword::Clear, || false).unwrap();
        assert!(!store(root.path()).proxy(id).unwrap().password_configured);
        assert_eq!(fresh.document().unwrap()[0], legacy);
    }

    #[test]
    fn invalid_or_locked_store_and_missing_id_cannot_replace_saved_records() {
        let root = tempfile::tempdir().unwrap();
        let backend = store(root.path());
        let src = b"[{\"id\":\"same\",\"name\":\"a\",\"kind\":\"ssh\"},{\"id\":\"same\",\"name\":\"b\",\"kind\":\"ssh\"}]";
        fs::write(&backend.path, src).unwrap();
        assert_eq!(backend.summaries(), Err(Error::Unreadable));
        assert_eq!(backend.save_proxy(None, "Proxy", proxy(18080), NativeTunnelPassword::Keep, || false),
            Err(Error::Unreadable));
        assert_eq!(fs::read(&backend.path).unwrap(), src);
        fs::write(&backend.path, b"[]").unwrap();
        assert_eq!(backend.save_proxy(Some("deleted"), "Proxy", proxy(18080), NativeTunnelPassword::Keep, || false),
            Err(Error::MissingOrUnsupported));
        let lock = OpenOptions::new().read(true).write(true)
            .open(backend.path.with_added_extension("lock")).unwrap();
        lock.lock().unwrap();
        assert_eq!(backend.save_proxy(None, "Proxy", proxy(18080), NativeTunnelPassword::Keep, || false),
            Err(Error::Unwritable));
        assert_eq!(fs::read(&backend.path).unwrap(), b"[]");
    }

    #[test]
    fn saved_password_requires_original_key_and_existing_connection_keys_are_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let backend = store(root.path());
        assert_eq!(backend.save_proxy(None, "Proxy", proxy(18080),
            NativeTunnelPassword::Replace("fixture".into()), || false), Err(Error::Credential));
        assert!(!backend.path.exists());
        assert!(!root.path().join(".key").exists());
        backend.save_proxy(None, "Proxy", proxy(18080),
            NativeTunnelPassword::Replace("fixture".into()), || true).unwrap();
        let bytes = fs::read(&backend.path).unwrap();
        let original_key = root.path().join(".key");
        fs::rename(&original_key, root.path().join("original.test-only-key")).unwrap();
        let _other_key = crate::secrets::load_master_key(root.path(), backend.keys, true).unwrap();
        assert_eq!(backend.save_proxy(None, "Not saved", proxy(18080),
            NativeTunnelPassword::Replace("fixture".into()), || false), Err(Error::Credential));
        assert_eq!(fs::read(&backend.path).unwrap(), bytes);
        assert_eq!(backend.summaries().unwrap()[0].name, "Proxy");
    }
}
