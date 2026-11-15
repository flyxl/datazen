//! View modules.

mod view_chrome;
mod view_navigation;
mod view_connections;
mod view_workspace;
mod view_structure;
mod view_settings;
mod view_tools_sync;
mod view_tools_ops;
mod view_dashboards_workflows;

pub(crate) use view_chrome::*;
pub(crate) use view_navigation::*;
pub(crate) use view_connections::*;
pub(crate) use view_workspace::*;
pub(crate) use view_structure::*;
pub(crate) use view_settings::*;
pub(crate) use view_tools_sync::*;
pub(crate) use view_tools_ops::*;
pub(crate) use view_dashboards_workflows::*;
