//! Tests for the Qdrant resource provider.
//!
//! Every test here is **offline**: no Qdrant instance, no credentials. What is
//! under test is the contract surface — what the provider declares, what it
//! refuses, and which real code each answer runs — not the Qdrant wire format.
//! The only network traffic is [`support::stub_qdrant`], a loopback listener
//! that answers a fixed body, so the HTTP path is exercised for real without a
//! server or a new dependency.
//!
//! The failures asserted below are the point of the whole suite: an operation
//! this driver cannot honour must come back as `Err` (or as an explicit
//! `Unsupported` value), never as an empty success.
//!
//! The suite is split by concern rather than one long file:
//!
//! * [`support`] — the doubles and fixtures every test shares.
//! * [`wiring`] — the factory, the fail-closed accessor, the declared
//!   capabilities and their evidence.
//! * [`lifecycle`] — describe, acquire, execute, observe, close, ownership.
//! * [`unsupported`] — the half of the contract this driver must refuse, and
//!   the refusals that are values rather than errors.

mod lifecycle;
mod support;
mod unsupported;
mod wiring;
