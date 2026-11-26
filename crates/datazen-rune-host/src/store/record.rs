use super::{ConnectionConfig, ConnectionRepositoryError};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

pub(super) const PASSWORD_FORMAT: &str = "aes-256-gcm-v1";
const FORMAT_FIELD: &str = "passwordFormat";

/// Read through the driver API's validated config, write the original JSON
/// object. Older sparse configs and nested fields from newer clients survive
/// updates to individual connection properties.
#[derive(Clone, Serialize)]
#[serde(transparent)]
pub(super) struct StoredConnection {
    raw: Value,
    #[serde(skip)]
    config: ConnectionConfig,
}

impl<'de> Deserialize<'de> for StoredConnection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(deserializer)?;
        if !raw.is_object() {
            return Err(serde::de::Error::custom("expected a connection object"));
        }
        let config = ConnectionConfig::deserialize(&raw).map_err(serde::de::Error::custom)?;
        Ok(Self { raw, config })
    }
}

impl StoredConnection {
    pub(super) fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    pub(super) fn format(&self) -> Option<&Value> {
        self.raw.get(FORMAT_FIELD)
    }

    pub(super) fn set_sealed_password(&mut self, sealed: String) {
        self.raw["password"] = Value::String(sealed.clone());
        self.raw[FORMAT_FIELD] = PASSWORD_FORMAT.into();
        self.config.password = Some(sealed);
    }

    pub(super) fn set_last_connected_at(&mut self, timestamp: String) {
        self.raw["lastConnectedAt"] = Value::String(timestamp.clone());
        self.config.last_connected_at = Some(timestamp);
    }

    pub(super) fn from_config(config: ConnectionConfig) -> Result<Self, ConnectionRepositoryError> {
        let mut raw = config_json(&config)?;
        if config.password.as_deref().is_some_and(|s| !s.is_empty()) {
            raw[FORMAT_FIELD] = PASSWORD_FORMAT.into();
        }
        Ok(Self { raw, config })
    }

    pub(super) fn replace_config(
        &mut self,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        let before = config_json(&self.config)?;
        let after = config_json(&config)?;
        apply_changes(&mut self.raw, &before, &after);
        if config.password.as_deref().is_some_and(|s| !s.is_empty()) {
            self.raw[FORMAT_FIELD] = PASSWORD_FORMAT.into();
        } else if let Some(raw) = self.raw.as_object_mut() {
            raw.remove(FORMAT_FIELD);
        }
        self.config = config;
        Ok(())
    }
}

fn config_json(config: &ConnectionConfig) -> Result<Value, ConnectionRepositoryError> {
    serde_json::to_value(config).map_err(|_| {
        ConnectionRepositoryError::BackendUnavailable(
            "could not encode the connection profile".into(),
        )
    })
}

/// Apply the caller's changes to known fields. Preserve missing defaults and
/// unknown fields at every object depth, including nested tunnel settings.
fn apply_changes(raw: &mut Value, before: &Value, after: &Value) {
    if before == after {
        return;
    }
    if let (Some(target), Some(old), Some(new)) =
        (raw.as_object_mut(), before.as_object(), after.as_object())
    {
        for field in old.keys().filter(|field| !new.contains_key(*field)) {
            target.remove(field);
        }
        for (field, value) in new {
            if old.get(field) == Some(value) {
                continue;
            }
            match (target.get_mut(field), old.get(field)) {
                (Some(original), Some(previous)) => apply_changes(original, previous, value),
                _ => {
                    target.insert(field.clone(), value.clone());
                }
            }
        }
    } else {
        *raw = after.clone();
    }
}
