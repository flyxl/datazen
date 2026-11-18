//! View modules.

mod ai_chat;
mod chrome;
mod connections;
mod connections_home;
mod dashboards_workflows;
mod modals;
mod navigation;
mod settings;
mod structure;
mod tools_ops;
mod tools_sync;
mod workspace;

pub(crate) use ai_chat::*;
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
