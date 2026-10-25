//! AI provider contracts: trait, request/response models, factory registration.
//!
//! These definitions are the stable surface plugin crates implement; they must
//! not depend on any vendor adapter or HTTP machinery living in
//! [`super::providers`].

pub mod factory;
pub mod traits;
pub mod types;

pub use factory::*;
pub use traits::*;
pub use types::*;
