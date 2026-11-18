//! View modules.

mod chrome;
mod connections;
mod connections_home;
mod dashboards_workflows;
// The windows/dashboard subtree ports (T018: AddToDashboardDialog is its
// first); the dashboards page itself stays in dashboards_workflows.rs.
pub(crate) mod dashboard;
// The windows/onboarding subtree ports (T021: BrandSidebar is its first).
pub(crate) mod onboarding;
// The windows/schema-diff subtree ports (T025: the deploy panel is its first).
pub(crate) mod schema_diff;
mod modals;
mod navigation;
mod settings;
mod structure;
mod tools_ops;
mod tools_sync;
mod workspace;

pub(crate) use chrome::*;
pub(crate) use connections::*;
pub(crate) use connections_home::*;
pub(crate) use dashboards_workflows::*;
pub(crate) use modals::*;
pub(crate) use navigation::*;
pub(crate) use settings::*;
pub(crate) use structure::*;
pub(crate) use tools_ops::*;
pub(crate) use tools_sync::*;
pub(crate) use workspace::*;
