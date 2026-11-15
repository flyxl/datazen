//! View modules.

mod chrome;
mod navigation;
mod connections;
mod workspace;
mod structure;
mod settings;
mod tools_sync;
mod tools_ops;
mod dashboards_workflows;

pub(crate) use chrome::*;
pub(crate) use navigation::*;
pub(crate) use connections::*;
pub(crate) use workspace::*;
pub(crate) use structure::*;
pub(crate) use settings::*;
pub(crate) use tools_sync::*;
pub(crate) use tools_ops::*;
pub(crate) use dashboards_workflows::*;
