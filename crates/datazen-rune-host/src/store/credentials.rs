use super::record::{StoredConnection, PASSWORD_FORMAT};
use super::{ConnectionConfig, ConnectionRepositoryError, FileConnectionRepository, StoreFile};
use datazen::connection_import::{decrypt_field, encrypt_field};
use std::cell::OnceCell;
use std::collections::BTreeMap;
use std::path::Path;

const RECOVERY: &str =
    "restore the original encryption key or re-enter the affected passwords with --replace-saved-passwords";

impl StoredConnection {
    fn may_be_encrypted(&self) -> bool {
        // The old writer encoded a 12-byte nonce, 16-byte GCM tag and at
        // least one password byte into a single ASCII base64 token. Also
        // protect damaged tokens (bad padding, line breaks, or punctuation).
        self.format().is_some()
            || self
                .config()
                .password
                .as_ref()
                .is_some_and(|s| s.len() >= 40 && s.is_ascii() && !s.trim().contains(' '))
    }

    fn decrypt(&self, key: &[u8; 32]) -> Result<String, String> {
        if self
            .format()
            .is_some_and(|v| v.as_str() != Some(PASSWORD_FORMAT))
        {
            return Err("unsupported saved password format".into());
        }
        let encrypted = self.config().password.as_deref().ok_or(RECOVERY)?;
        decrypt_field(encrypted, key).map_err(|_| RECOVERY.into())
    }
}

impl FileConnectionRepository {
    fn master_key(&self, allow_create: bool) -> Result<[u8; 32], String> {
        let mut cached = self
            .key
            .lock()
            .map_err(|_| "the password key lock is unavailable")?;
        if let Some(key) = *cached {
            return Ok(key);
        }
        let dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        let key = (self.load_key)(dir, allow_create)?;
        *cached = Some(key);
        Ok(key)
    }

    /// Before sealing with an existing key, authenticate it against at least
    /// one surviving ciphertext. An explicit replacement can recover the last
    /// unreadable password without a key for the discarded value.
    fn sealing_key(&self, state: &StoreFile, replacing: Option<&str>) -> Result<[u8; 32], String> {
        let mut protected = state
            .connections
            .iter()
            .filter(|row| replacing != Some(row.config().id.as_str()) && row.may_be_encrypted())
            .peekable();
        let fresh = protected.peek().is_none();
        let key = self.master_key(fresh)?;
        if !fresh && !protected.any(|row| row.decrypt(&key).is_ok()) {
            return Err(RECOVERY.into());
        }
        Ok(key)
    }

    /// Explicit recovery for profiles whose original key or legacy format is
    /// unavailable. Change only the selected password fields in one store write.
    pub fn replace_saved_passwords(
        &self,
        replacements: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        if replacements.is_empty()
            || replacements
                .iter()
                .any(|(id, password)| id.is_empty() || password.is_empty())
        {
            return Err(
                "provide an existing connection ID and a nonempty password for every replacement"
                    .into(),
            );
        }
        self.update(|state| {
            for id in replacements.keys() {
                if state.connections.iter().filter(|row| &row.config().id == id).count() != 1 {
                    return Err(ConnectionRepositoryError::BackendUnavailable(
                        "each replacement must match exactly one saved connection".into()
                    ));
                }
            }
            let mut retained = state.connections.iter().filter(|row| {
                !replacements.contains_key(&row.config().id) && row.may_be_encrypted()
            }).peekable();
            let key = self.master_key(retained.peek().is_none()).map_err(ConnectionRepositoryError::BackendUnavailable)?;
            if retained.any(|row| row.decrypt(&key).is_err()) {
                return Err(ConnectionRepositoryError::BackendUnavailable(
                    "restore the original key or include every unreadable password in the replacement".into()
                ));
            }
            for row in &mut state.connections {
                if let Some(password) = replacements.get(&row.config().id) {
                    row.set_sealed_password(seal(password, &key).map_err(ConnectionRepositoryError::BackendUnavailable)?);
                }
            }
            Ok(true)
        }).map_err(|err| err.to_string())
    }

    pub(super) fn save_config(
        &self,
        mut config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        self.update(|state| {
            if let Some(password) = config.password.as_deref().filter(|p| !p.is_empty()) {
                let key = self
                    .sealing_key(state, Some(&config.id))
                    .map_err(ConnectionRepositoryError::BackendUnavailable)?;
                config.password = Some(
                    seal(password, &key).map_err(ConnectionRepositoryError::BackendUnavailable)?,
                );
            }
            if let Some(existing) = state
                .connections
                .iter_mut()
                .find(|row| row.config().id == config.id)
            {
                existing.replace_config(config)?;
            } else {
                state
                    .connections
                    .push(StoredConnection::from_config(config)?);
            }
            Ok(true)
        })
    }

    /// Return the password for the caller and any ciphertext to publish atomically.
    fn read_password(
        &self,
        row: &StoredConnection,
        write_key: impl FnOnce() -> Result<[u8; 32], String>,
    ) -> Result<(Option<String>, Option<String>), String> {
        let stored = &row.config().password;
        if row.may_be_encrypted() {
            let plain = row.decrypt(&self.master_key(false)?)?;
            let upgrade = if row.format().is_some() {
                None
            } else {
                stored.clone()
            };
            return Ok((Some(plain), upgrade));
        }
        let upgrade = match stored.as_deref() {
            Some(plain) if !plain.is_empty() => Some(seal(plain, &write_key()?)?),
            _ => None,
        };
        Ok((stored.clone(), upgrade))
    }

    pub(super) fn read_connections(
        &self,
        id: Option<&str>,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        let mut snapshot = StoreFile::read(&self.path)?;
        // Tagged and passwordless records are read-only, including on a read-only volume.
        let has_legacy = snapshot.connections.iter().any(|row| {
            id.is_none_or(|id| id == row.config().id)
                && row.format().is_none()
                && row
                    .config()
                    .password
                    .as_deref()
                    .is_some_and(|p| !p.is_empty())
        });
        if has_legacy {
            return self.transaction(|state| self.reveal_connections(state, id));
        }
        self.reveal_connections(&mut snapshot, id)
            .map(|(rows, _)| rows)
    }

    fn reveal_connections(
        &self,
        current: &mut StoreFile,
        id: Option<&str>,
    ) -> Result<(Vec<ConnectionConfig>, bool), ConnectionRepositoryError> {
        let mut upgrades = Vec::new();
        let mut result = Vec::new();
        let write_key = OnceCell::new();
        for (index, row) in current.connections.iter().enumerate() {
            if id.is_some_and(|id| id != row.config().id) {
                continue;
            }
            let (password, migration) = match self.read_password(row, || {
                write_key
                    .get_or_init(|| self.sealing_key(current, None))
                    .clone()
            }) {
                Ok(values) => values,
                Err(error) if id.is_some() => {
                    return Err(ConnectionRepositoryError::BackendUnavailable(format!(
                        "the saved connection password is unavailable ({error})"
                    )))
                }
                Err(_) => (None, None),
            };
            if let Some(sealed) = migration {
                upgrades.push((index, sealed));
            }
            let mut config = row.config().clone();
            config.password = password;
            result.push(config);
        }
        let changed = !upgrades.is_empty();
        for (index, sealed) in upgrades {
            let saved = &mut current.connections[index];
            saved.set_sealed_password(sealed);
        }
        Ok((result, changed))
    }
}

fn seal(password: &str, key: &[u8; 32]) -> Result<String, String> {
    encrypt_field(password, key).map_err(|_| "could not encrypt the connection password".into())
}
