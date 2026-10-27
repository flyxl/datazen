//! Desktop compatibility export for the transport-neutral driver mapping.
//!
//! The implementation lives in `datazen-modules`; driver contract tests use
//! that shared module directly and never include Desktop host source files.

pub use datazen_modules::execution::runtime::driver_mapping::*;
