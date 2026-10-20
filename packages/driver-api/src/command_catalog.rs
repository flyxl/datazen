//! The command-description contract.
//!
//! A driver declares which commands it exposes and what target each command
//! requires. That is *description*, and it is deliberately separate from
//! execution: nothing here runs a command, acquires a resource, opens a
//! session or touches cancellation. Execution keeps its existing lifecycle
//! (the driver's command entry point today, `ResourceProvider` for the
//! migrated path), which is the only place resource ownership is enforced.
//!
//! Splitting this out means a host surface that only needs to *list* commands
//! (target metadata, workflow validation, the command catalogue) depends on
//! [`CommandCatalog`] instead of the whole driver, and a driver cannot end up
//! with a second execution lifecycle hidden inside its description.
//!
//! [`LegacyCommandCatalog`] forwards to the two `DatabaseDriver` methods of
//! the same name, one way. A driver keeps exactly one implementation of each.

use crate::namespace::CommandTargetRequirements;
use crate::traits::DatabaseDriver;
use crate::DriverCommandDefinition;

/// Which commands a driver exposes, and what each one needs from a target.
pub trait CommandCatalog: Send + Sync {
    /// Commands this driver supports. SQL drivers get the standard `query` and
    /// `execute` plus the schema catalog commands through the driver default.
    fn command_definitions(&self) -> Vec<DriverCommandDefinition>;

    /// Target requirements for one command, or `None` when the command is not
    /// registered. `None` is how the platform rejects an unknown command
    /// before acquiring a resource.
    fn target_requirements(&self, command: &str) -> Option<CommandTargetRequirements>;
}

/// Borrowing adapter from an existing [`DatabaseDriver`] to [`CommandCatalog`].
///
/// One-way, like the other migration seams: every method forwards, so a driver
/// override keeps working and there is nothing to keep in sync beyond the two
/// signatures.
#[derive(Debug, Clone, Copy)]
pub struct LegacyCommandCatalog<'a, D: DatabaseDriver + ?Sized> {
    driver: &'a D,
}

impl<'a, D: DatabaseDriver + ?Sized> LegacyCommandCatalog<'a, D> {
    pub fn new(driver: &'a D) -> Self {
        Self { driver }
    }

    /// The wrapped driver, for callers that still need a driver-only rule at
    /// the boundary.
    pub fn driver(&self) -> &'a D {
        self.driver
    }
}

impl<D: DatabaseDriver + ?Sized> CommandCatalog for LegacyCommandCatalog<'_, D> {
    fn command_definitions(&self) -> Vec<DriverCommandDefinition> {
        self.driver.command_definitions()
    }

    fn target_requirements(&self, command: &str) -> Option<CommandTargetRequirements> {
        self.driver.target_requirements(command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{DriverCommandDefinition, DriverCommandMetadata};
    use crate::mock_driver::{MockDriver, MockDriverOptions};
    use std::sync::Arc;

    fn mock(extra: Vec<DriverCommandDefinition>) -> Arc<MockDriver> {
        MockDriver::new(
            "postgres",
            MockDriverOptions {
                extra_commands: extra,
                ..MockDriverOptions::default()
            },
        )
    }

    fn extra_definition(id: &str) -> DriverCommandDefinition {
        DriverCommandDefinition {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            permissions: Vec::new(),
            metadata: DriverCommandMetadata::default(),
        }
    }

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn catalog_is_object_safe_and_thread_safe() {
        assert_send_sync::<dyn CommandCatalog>();
        let driver = mock(Vec::new());
        let catalog: &dyn CommandCatalog = &LegacyCommandCatalog::new(driver.as_ref());
        assert!(catalog
            .command_definitions()
            .iter()
            .any(|definition| definition.id == "query"));
    }

    #[test]
    fn adapter_forwards_driver_definitions_and_extras() {
        let driver = mock(vec![extra_definition("workbench_status")]);
        let catalog = LegacyCommandCatalog::new(driver.as_ref());
        let ids: Vec<String> = catalog
            .command_definitions()
            .into_iter()
            .map(|definition| definition.id)
            .collect();
        assert!(ids.iter().any(|id| id == "query"));
        assert!(ids.iter().any(|id| id == "execute"));
        assert!(ids.iter().any(|id| id == "workbench_status"));
    }

    #[test]
    fn adapter_forwards_target_requirements_and_refuses_unknown_commands() {
        let driver = mock(vec![extra_definition("workbench_status")]);
        let catalog = LegacyCommandCatalog::new(driver.as_ref());

        let query = catalog
            .target_requirements("query")
            .expect("query is registered");
        assert!(query.allows_object, "query may address an object target");

        let schema = catalog
            .target_requirements("get_table_schema")
            .expect("get_table_schema is registered");
        assert!(schema.allows_object && schema.requires_object);

        assert!(
            catalog.target_requirements("not_a_command").is_none(),
            "an unregistered command must be refused before resource acquisition"
        );
    }
}
