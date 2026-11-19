//! View modules.

mod ai_chat;
mod chrome;
mod connections;
mod connections_home;
mod dashboards_workflows;
mod data_sync;
mod data_transfer;
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
#[allow(unused_imports)] // the Data Sync window (T270) is the production consumer
pub(crate) use data_sync::*;
#[allow(unused_imports)] // the editor shell (T023-S2) is the production consumer
pub(crate) use data_transfer::*;
pub(crate) use modals::*;
pub(crate) use navigation::*;
pub(crate) use settings::*;
pub(crate) use structure::*;
pub(crate) use tools_ops::*;
pub(crate) use tools_sync::*;
pub(crate) use workspace::*;
