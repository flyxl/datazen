//! React `components/connection/` ports, at the same path as the source tree.
//!
//! One module per React file. Parallel migration tracks add one `mod` line each
//! to this file rather than sharing a component file.

pub(crate) mod batch_export_dialog;
pub(crate) mod batch_export_parts;
pub(crate) mod connection_advanced_settings;
pub(crate) mod connection_ai_notes;
pub(crate) mod connection_form_body;
pub(crate) mod connection_import_fields;
pub(crate) mod connection_share_dialog;
pub(crate) mod new_connection_dialog;
pub(crate) mod connection_share_dialog_host;
pub(crate) mod file_connection_fields;
pub(crate) mod http_proxy_tunnel_fields;
pub(crate) mod index_connection_fields;
pub(crate) mod object_filter_dialog;
pub(crate) mod save_tunnel_dialog;
pub(crate) mod shared;
pub(crate) mod ssh_tunnel_fields;
pub(crate) mod standard_connection_fields;
pub(crate) mod tunnel_field_contracts;
pub(crate) mod use_connection_clipboard_fill;
pub(crate) mod websocket_tunnel_fields;
