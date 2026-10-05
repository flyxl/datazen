use super::identity::LOCAL_ORGANIZATION_ID;
use crate::{
    db::ConnectionConfig,
    store::{platform_profiles::ProfileMetadata, Store},
};
use async_trait::async_trait;
use datazen_platform_api::{
    context::RequestContext,
    dto::profile::{ProfileDraft, ProfilePatch},
    error::PortError,
    id::*,
    ports::{
        profile::{ProfileRecord, ProfileRepository, ProfileScope},
        secret::{ResolvedCredential, SecretProvider, SecretPurpose},
    },
    target::NamespaceTarget,
};
use std::{collections::BTreeMap, sync::Arc};

pub struct DesktopProfiles {
    pub(crate) store: Arc<Store>,
    create_lock: tokio::sync::Mutex<()>,
}
impl DesktopProfiles {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            create_lock: tokio::sync::Mutex::new(()),
        }
    }
    fn local(ctx: &RequestContext) -> Result<(), PortError> {
        if ctx.organization_id.as_str() != LOCAL_ORGANIZATION_ID {
            return Err(PortError::NotFound("profile".into()));
        }
        Ok(())
    }
    pub(crate) fn record(config: &ConnectionConfig, metadata: &ProfileMetadata) -> ProfileRecord {
        let mut record = ProfileRecord::new(
            OrganizationId::new(LOCAL_ORGANIZATION_ID),
            ConnectionId::new(&config.id),
            config.database_type.clone(),
        );
        record.name = config.name.clone();
        record.config_revision = ConfigRevision::new(metadata.config_revision);
        record.credential_revision = CredentialRevision::new(metadata.credential_revision);
        record.initial_namespace =
            NamespaceTarget::new(config.database.clone(), None, config.schema.clone(), vec![]);
        record.secret_ref = config
            .password
            .as_ref()
            .map(|_| SecretRef::new(format!("desktop-password:{}", config.id)));
        record.enabled = metadata.enabled;
        record.read_only = config.read_only;
        record.created_at = Timestamp::new(&metadata.created_at);
        record.updated_at = Timestamp::new(&metadata.updated_at);
        record.driver_options = BTreeMap::from([
            ("host".into(), serde_json::json!(config.host)),
            ("port".into(), serde_json::json!(config.port)),
            ("username".into(), serde_json::json!(config.username)),
            ("sslMode".into(), serde_json::json!(config.ssl_mode)),
        ]);
        record
    }
    fn options(options: &BTreeMap<String, serde_json::Value>) -> Result<(), PortError> {
        if options.keys().any(|key| {
            [
                "password",
                "secret",
                "token",
                "credential",
                "privatekey",
                "private_key",
                "apikey",
                "api_key",
            ]
            .iter()
            .any(|marker| key.to_ascii_lowercase().contains(marker))
        }) {
            return Err(PortError::BackendUnavailable(
                "sensitive options must use credentials".into(),
            ));
        }
        Ok(())
    }
    fn apply(
        config: &mut ConnectionConfig,
        namespace: Option<NamespaceTarget>,
        options: Option<BTreeMap<String, serde_json::Value>>,
        credentials: Option<BTreeMap<String, String>>,
    ) -> Result<bool, PortError> {
        if let Some(namespace) = namespace {
            config.database = namespace.database;
            config.schema = namespace.schema;
        }
        if let Some(options) = options {
            Self::options(&options)?;
            let mut value = serde_json::to_value(&*config).map_err(|_| {
                PortError::BackendUnavailable("configuration conversion failed".into())
            })?;
            for (key, option) in options {
                if !matches!(
                    key.as_str(),
                    "host"
                        | "port"
                        | "username"
                        | "sslMode"
                        | "connectionTimeout"
                        | "maxPoolSize"
                        | "readOnly"
                ) {
                    return Err(PortError::BackendUnavailable(
                        "unsupported public connection option".into(),
                    ));
                }
                value[key] = option;
            }
            *config = serde_json::from_value(value)
                .map_err(|_| PortError::BackendUnavailable("invalid connection option".into()))?;
        }
        let mut changed = false;
        if let Some(credentials) = credentials {
            for (key, value) in credentials {
                if key != "password" {
                    return Err(PortError::BackendUnavailable(
                        "unsupported credential purpose".into(),
                    ));
                }
                changed |= config.password.as_deref() != Some(&value);
                config.password = Some(value);
            }
        }
        Ok(changed)
    }
}
#[async_trait]
impl ProfileRepository for DesktopProfiles {
    async fn list(
        &self,
        ctx: &RequestContext,
        scope: ProfileScope,
    ) -> Result<Vec<ProfileRecord>, PortError> {
        if Self::local(ctx).is_err() || scope.organization_id != ctx.organization_id {
            return Ok(vec![]);
        }
        let mut records = vec![];
        for config in self.store.get_connections().await {
            if let Some((config, metadata)) = self.store.platform_profile(&config.id).await {
                if metadata.enabled {
                    records.push(Self::record(&config, &metadata));
                }
            }
        }
        Ok(records)
    }
    async fn get(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
    ) -> Result<Option<ProfileRecord>, PortError> {
        if Self::local(ctx).is_err() {
            return Ok(None);
        }
        Ok(self
            .store
            .platform_profile(id.as_str())
            .await
            .map(|(config, metadata)| Self::record(&config, &metadata)))
    }
    async fn create(
        &self,
        ctx: &RequestContext,
        draft: ProfileDraft,
        idem: &IdempotencyKey,
    ) -> Result<ProfileRecord, PortError> {
        Self::local(ctx)?;
        let _guard = self.create_lock.lock().await;
        let key = format!("{}:{}", ctx.principal_id.as_str(), idem.as_str());
        if let Some((config, metadata)) = self.store.platform_creation(&key).await {
            return Ok(Self::record(&config, &metadata));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let mut config: ConnectionConfig = serde_json::from_value(
            serde_json::json!({ "id": id, "name": draft.name, "databaseType": draft.driver_id }),
        )
        .map_err(|_| PortError::BackendUnavailable("invalid profile draft".into()))?;
        Self::apply(
            &mut config,
            Some(draft.initial_namespace),
            Some(draft.public_options),
            draft.credentials,
        )?;
        let (config, metadata) = self
            .store
            .platform_update(&id, None, |existing, metadata| {
                if existing.is_some() {
                    return Err(PortError::CasConflict {
                        entity: "profile",
                        id: id.clone(),
                    });
                }
                metadata.creation_key = Some(key);
                Ok(config)
            })
            .await?;
        Ok(Self::record(&config, &metadata))
    }
    async fn compare_and_set(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
        expected: ConfigRevision,
        patch: ProfilePatch,
    ) -> Result<ProfileRecord, PortError> {
        Self::local(ctx)?;
        let (config, metadata) = self
            .store
            .platform_update(
                id.as_str(),
                Some(expected.counter().get()),
                |config, metadata| {
                    let mut config = config.ok_or_else(|| PortError::NotFound("profile".into()))?;
                    if let Some(name) = patch.name {
                        config.name = name;
                    }
                    if Self::apply(
                        &mut config,
                        patch.initial_namespace,
                        patch.public_options,
                        patch.credentials,
                    )? {
                        metadata.credential_revision = next(metadata.credential_revision)?;
                    }
                    metadata.config_revision = next(metadata.config_revision)?;
                    Ok(config)
                },
            )
            .await?;
        Ok(Self::record(&config, &metadata))
    }
    async fn disable(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
    ) -> Result<ProfileRecord, PortError> {
        Self::local(ctx)?;
        let (config, metadata) = self
            .store
            .platform_update(id.as_str(), None, |config, metadata| {
                let config = config.ok_or_else(|| PortError::NotFound("profile".into()))?;
                metadata.enabled = false;
                metadata.config_revision = next(metadata.config_revision)?;
                Ok(config)
            })
            .await?;
        Ok(Self::record(&config, &metadata))
    }
    async fn advance_credential_revision(
        &self,
        ctx: &RequestContext,
        id: ConnectionId,
    ) -> Result<CredentialRevision, PortError> {
        Self::local(ctx)?;
        let (_, metadata) = self
            .store
            .platform_update(id.as_str(), None, |config, metadata| {
                let config = config.ok_or_else(|| PortError::NotFound("profile".into()))?;
                metadata.credential_revision = next(metadata.credential_revision)?;
                Ok(config)
            })
            .await?;
        Ok(CredentialRevision::new(metadata.credential_revision))
    }
}
fn next(value: u64) -> Result<u64, PortError> {
    value
        .checked_add(1)
        .ok_or_else(|| PortError::BackendUnavailable("revision exhausted".into()))
}

#[async_trait]
impl SecretProvider for DesktopProfiles {
    async fn resolve(
        &self,
        ctx: &RequestContext,
        secret: SecretRef,
        purpose: SecretPurpose,
    ) -> Result<ResolvedCredential, PortError> {
        Self::local(ctx)?;
        if purpose != SecretPurpose::DatabasePassword {
            return Err(PortError::NotFound("credential".into()));
        }
        let id = secret
            .as_str()
            .strip_prefix("desktop-password:")
            .ok_or_else(|| PortError::NotFound("credential".into()))?;
        let (config, _) = self
            .store
            .platform_profile(id)
            .await
            .ok_or_else(|| PortError::NotFound("credential".into()))?;
        config
            .password
            .map(|password| ResolvedCredential::new(password.into_bytes()))
            .ok_or_else(|| PortError::NotFound("credential".into()))
    }
    async fn read_versioned(
        &self,
        ctx: &RequestContext,
        secret: SecretRef,
        revision: CredentialRevision,
    ) -> Result<ResolvedCredential, PortError> {
        Self::local(ctx)?;
        let id = secret
            .as_str()
            .strip_prefix("desktop-password:")
            .ok_or_else(|| PortError::NotFound("credential".into()))?;
        let (config, metadata) = self
            .store
            .platform_profile(id)
            .await
            .ok_or_else(|| PortError::NotFound("credential".into()))?;
        if metadata.credential_revision != revision.counter().get() {
            return Err(PortError::CasConflict {
                entity: "credential",
                id: id.into(),
            });
        }
        config
            .password
            .map(|password| ResolvedCredential::new(password.into_bytes()))
            .ok_or_else(|| PortError::NotFound("credential".into()))
    }
    async fn rotate(
        &self,
        ctx: &RequestContext,
        secret: SecretRef,
        expected: CredentialRevision,
    ) -> Result<CredentialRevision, PortError> {
        Self::local(ctx)?;
        let id = secret
            .as_str()
            .strip_prefix("desktop-password:")
            .ok_or_else(|| PortError::NotFound("credential".into()))?;
        let (_, metadata) = self
            .store
            .platform_update(id, None, |config, metadata| {
                if metadata.credential_revision != expected.counter().get() {
                    return Err(PortError::CasConflict {
                        entity: "credential",
                        id: id.into(),
                    });
                }
                let config = config.ok_or_else(|| PortError::NotFound("credential".into()))?;
                metadata.credential_revision = next(metadata.credential_revision)?;
                Ok(config)
            })
            .await?;
        Ok(CredentialRevision::new(metadata.credential_revision))
    }
    async fn revision(&self, secret: SecretRef) -> Result<CredentialRevision, PortError> {
        let id = secret
            .as_str()
            .strip_prefix("desktop-password:")
            .ok_or_else(|| PortError::NotFound("credential".into()))?;
        let (_, metadata) = self
            .store
            .platform_profile(id)
            .await
            .ok_or_else(|| PortError::NotFound("credential".into()))?;
        Ok(CredentialRevision::new(metadata.credential_revision))
    }
}
