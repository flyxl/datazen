//! How the resource provider is tested, and what these tests are allowed to do.
//!
//! | File | Question it answers |
//! |---|---|
//! | [`wiring`] | is the provider reachable through the fail-closed accessor, and does it declare a real capability shape? |
//! | [`lifecycle`] | describe, acquire, execute, observe, close — where a mistake costs a charge or a row |
//! | [`unsupported`] | does every operation Stargate cannot honour come back as a refusal, not an empty `Ok(())` |
//! | [`support`] | the doubles and fixtures all of them share |

mod lifecycle;
mod support;
mod unsupported;
mod wiring;
