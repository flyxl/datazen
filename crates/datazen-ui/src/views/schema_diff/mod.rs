//! The `windows/schema-diff` subtree ports (the deploy panel is its first;
//! the wizard window's step panels land with their own tasks).

pub(crate) mod schema_diff_deploy_panel;
// T323: the limitations dialog's key list (schemaDiffLimitationKeys.ts).
pub(crate) mod schema_diff_limitation_keys;
// T363: the unified-plan object identity key (schemaDiffObjectIdentity.ts).
pub(crate) mod schema_diff_object_identity;
