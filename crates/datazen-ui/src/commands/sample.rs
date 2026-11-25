//! Port of `packages/frontend/src/commands/sample.ts` (T366) —
//! first-run sample dataset IPC (Rust `commands/sample.rs`).
//!
//! `seedSampleDb` seeds an English `demo_sales` SQLite playground used
//! by the onboarding "Explore with sample data" entry; idempotent and
//! returns the db path. The command name rides verbatim.
//!
//! Known deltas: none.

// No production consumer yet (the onboarding track wires the face);
// this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde_json::{json, Value};

/// The sample commands' wire face (one method; the enum keeps the
/// family's shape uniform with the other command ports).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleMethod {
    SeedSampleDb,
}

impl SampleMethod {
    /// The invoke command.
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::SeedSampleDb => "seed_sample_db",
        }
    }
}

/// One shaped transport call: the method plus its payload (the host
/// runs it; the answer is the seeded db path).
#[derive(Clone, Debug, PartialEq)]
pub struct SampleRequest {
    pub method: SampleMethod,
    pub value: Value,
}

/// `seedSampleDb()` → `seed_sample_db` (no payload; idempotent, the
/// answer is the db path).
pub fn seed_sample_db_request() -> SampleRequest {
    SampleRequest { method: SampleMethod::SeedSampleDb, value: json!({}) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_sample_db_is_a_bare_invoke() {
        let request = seed_sample_db_request();
        assert_eq!(request.method.wire_name(), "seed_sample_db");
        assert_eq!(request.value, json!({}), "no payload keys");
    }
}
