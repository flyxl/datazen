//! Shared infrastructure utilities (renamed from `platform-infra`, D1.5).
//!
//! Mechanism, not policy and not domain logic: credential redaction and
//! archive path safety are the same operation on desktop, on the web build
//! and inside the headless server, so exactly one implementation lives
//! here and every form links it.
//!
//! Depends on no other DataZen crate (F-11). If a capability you are about
//! to add needs `platform-api` or anything above it, it does not belong
//! in this crate.

pub mod path_safety;
pub mod redaction;
