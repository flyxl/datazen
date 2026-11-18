//! View modules.

mod chrome;
mod connections;
mod connections_home;
mod dashboards_workflows;
// The windows/dashboard subtree ports (T018: AddToDashboardDialog is its
// first); the dashboards page itself stays in dashboards_workflows.rs.
pub(crate) mod dashboard;
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
