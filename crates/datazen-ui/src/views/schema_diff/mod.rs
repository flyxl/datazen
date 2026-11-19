//! The `windows/schema-diff` subtree ports (the deploy panel is its first;
//! the wizard window's step panels land with their own tasks).

pub(crate) mod schema_diff_deploy_panel;
// T323: the limitations dialog's key list (schemaDiffLimitationKeys.ts).
pub(crate) mod schema_diff_limitation_keys;
// T363: the unified-plan object identity key (schemaDiffObjectIdentity.ts).
pub(crate) mod schema_diff_object_identity;
// T351: the discovery run's diagnostics, host seam and table-pick pairing
// (schemaDiffObjectDiscovery.ts; the schemaDiffTableNames.ts merge rides
// along as its seam module until T374 ports the full file).
pub(crate) mod schema_diff_object_discovery;
pub(crate) mod schema_diff_table_names;
