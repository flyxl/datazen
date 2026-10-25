//! Storage port for persisted Schema Diff profiles.
//!
//! The Schema Diff package owns the profile vocabulary and the persistence
//! boundary. A host decides where profiles live and how the organization scope
//! is isolated; the command layer only supplies the current scope and maps the
//! repository result onto its transport error.

use async_trait::async_trait;
use datazen_platform_api::ports::scope::OrganizationScope;

use crate::profile::SchemaDiffProfile;

/// Why a Schema Diff profile repository operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaDiffProfileRepositoryError {
    /// The host store could not be read or written.
    #[error("schema diff profile store unavailable: {0}")]
    Unavailable(String),

    /// The profile or profile scope is not available to the caller.
    #[error("schema diff profile not found: {0}")]
    NotFound(String),

    /// The profile failed the domain validation required before persistence.
    #[error("invalid schema diff profile: {0}")]
    Invalid(String),
}

/// Persistence boundary for reusable Schema Diff profiles.
#[async_trait]
pub trait SchemaDiffProfileRepository: Send + Sync + 'static {
    /// List profiles visible to `scope`.
    async fn list(
        &self,
        scope: &OrganizationScope,
    ) -> Result<Vec<SchemaDiffProfile>, SchemaDiffProfileRepositoryError>;

    /// Insert or replace one profile owned by `scope`.
    async fn save(
        &self,
        scope: &OrganizationScope,
        profile: SchemaDiffProfile,
    ) -> Result<(), SchemaDiffProfileRepositoryError>;

    /// Delete one profile owned by `scope`.
    async fn delete(
        &self,
        scope: &OrganizationScope,
        profile_id: &str,
    ) -> Result<(), SchemaDiffProfileRepositoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct EmptyRepository;

    #[async_trait]
    impl SchemaDiffProfileRepository for EmptyRepository {
        async fn list(
            &self,
            _scope: &OrganizationScope,
        ) -> Result<Vec<SchemaDiffProfile>, SchemaDiffProfileRepositoryError> {
            Ok(Vec::new())
        }

        async fn save(
            &self,
            _scope: &OrganizationScope,
            _profile: SchemaDiffProfile,
        ) -> Result<(), SchemaDiffProfileRepositoryError> {
            Ok(())
        }

        async fn delete(
            &self,
            _scope: &OrganizationScope,
            _profile_id: &str,
        ) -> Result<(), SchemaDiffProfileRepositoryError> {
            Ok(())
        }
    }

    #[test]
    fn the_port_is_object_safe_and_injectable() {
        let repository: Arc<dyn SchemaDiffProfileRepository> = Arc::new(EmptyRepository);
        assert!(Arc::strong_count(&repository) >= 1);
    }

    #[test]
    fn repository_errors_have_no_transport_or_runtime_dependency() {
        let source = include_str!("profile_repository.rs");
        let transport_crate = ["ta", "uri"].concat();
        let runtime_crate = ["datazen-", "runtime"].concat();
        assert!(!source.contains(&transport_crate));
        assert!(!source.contains(&runtime_crate));
    }
}
