//! Ports of `packages/frontend/src/windows/main/*`, one module per React
//! file:
//!
//! - `main_page.rs` — `MainPage.tsx` (the main window's composition root).
//! - `running_execution_close_host.rs` — `RunningExecutionCloseHost.tsx`
//!   (the prompt behind "close a tab whose query is still running": the
//!   host half of the store's gate, rendered over the main window).

pub mod main_page;
pub mod running_execution_close_host;

pub use main_page::*;
pub use running_execution_close_host::*;
