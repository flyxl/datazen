//! Ports of `packages/frontend/src/windows/main/*` (T020), one module per
//! React file:
//!
//! - `main_page.rs` — `MainPage.tsx` (the main window's composition root).
//!
//! `RunningExecutionCloseHost.tsx` is this directory's other React file; it
//! belongs to T071 and is not ported here.

pub(crate) mod main_page;

pub(crate) use main_page::*;
