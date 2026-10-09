//! Reusable real-driver contract template (shared by every `path` driver).
//!
//! This file is **not** a Cargo test target on its own — `packages/drivers/<id>/tests/`
//! has no `support/main.rs`, so `cargo test -p datazen-driver-http-support` ignores it.
//! A driver opts in with a thin binding, e.g. `packages/drivers/postgres/tests/real_driver_contract.rs`:
//!
//! ```ignore
//! #[path = "../../http-support/tests/support/real_driver_contract.rs"]
//! mod support;
//!
//! use support::{Capability, Contract, Dialect};
//!
//! const CONTRACT: Contract = Contract { /* this driver's declared expectations */ };
//! fn contract_driver() -> PostgresDriver { PostgresDriver::new() }
//! ```
//!
//! Why `http-support`: it is the one crate under `packages/drivers/` documented as a
//! *shared helper* rather than a driver (`driver-capability-migration.md` §1.2), so no
//! driver crate owns the contract and every driver pulls it in as a peer file. `#[path]`
//! keeps the template inside `tests/**` (AGENTS.md「驱动测试落点」: driver tests never move
//! to Host) and avoids needing a `[dev-dependencies]` entry in each driver `Cargo.toml`.
//! Every line of real dialect SQL lives in the driver's own crate, never here.
//!
//! Two tiers:
//!
//! * **Free tier** — real assertions that need no server. They run everywhere, including
//!   CI, and are what actually proves the *declared* contract is self-consistent.
//! * **Live tier** — one named `#[tokio::test]` per CM dimension. Each needs a real
//!   server and **dedicated** fixture databases; without them it skips and reports the
//!   dimension it could not verify. `fake-runtime-fixtures.md` §10.4 forbids substituting
//!   a fake runtime for a real-protocol conclusion, so a skip is reported as unverified,
//!   never as "verified".
//!
//! Credentials come from the **process environment only** (CI secret or developer shell),
//! per `fake-runtime-fixtures.md` §10.2 rule 5 and AGENTS.md「本地环境变量文件保护」.
//! `test_sources_never_read_env_files` enforces that as a regression guard.
//!
//! Add `-- --nocapture` to see the skip reasons printed by the live tier.
//!
//! What the live tier needs before it runs a single assertion:
//!
//! ```text
//! TEST_PG_HOST=127.0.0.1 TEST_PG_PORT=5432 TEST_PG_USER=postgres \
//! TEST_PG_DATABASE=dz_fixture_pg_a TEST_PG_DATABASE_B=dz_fixture_pg_b \
//! cargo test -p datazen-driver-postgres --test real_driver_contract -- --nocapture
//! ```
//!
//! `Contract::availability` is the gate, and it is deliberately strict: both
//! target names must start with `dz_fixture_` and must differ. A developer with
//! no dedicated fixture databases therefore gets a *skip with the reason*, not a
//! run against whatever happens to be installed — point the two keys at two of
//! your own prefixed databases to get a live tier.
//!
//! **Layout.** One file per tier, because one file per tier is what keeps every
//! file under the 800-line ceiling *and* keeps the responsibilities separable:
//!
//! | file | responsibility |
//! | --- | --- |
//! | `real_driver_contract_plumbing.rs` | shared vocabulary: env-file guard, dimension matrix, profile, fixtures, the withheld-capability wrapper, directory scans |
//! | `real_driver_contract_capability.rs` | the declared contract: `Capability`, `Verdict`, `Dialect`, `Contract`, the availability gate |
//! | `real_driver_contract_free.rs` | free tier — real assertions, no server |
//! | `real_driver_contract_live.rs` | live tier, journey half (CM-08/09/10/13/14/16) |
//! | `real_driver_contract_live_faults.rs` | live tier, failure half (CM-17/18/22/24/26/48/69) |
//! | `real_driver_contract_refusal.rs` | refusal tier — a withheld capability, proved with a real driver behind the branch |
//! | `real_driver_contract_probe.rs` | the instrument the refusal tier attacks itself with: a real driver with exactly one serverless method's return value changed |
//! | `real_driver_contract_scope.rs` | scope tier — what the guards cover, and the unverified-scope report |
//! | this file | the template's own entry point: module wiring and the source-level env-file guard |
//!
//! The env-file guard scans the **whole directory**, so a file added here is
//! covered by the guard on the same commit that adds it.

#![allow(dead_code)]

use std::path::PathBuf;

#[path = "real_driver_contract_plumbing.rs"]
mod plumbing;
pub use plumbing::*;

#[path = "real_driver_contract_capability.rs"]
mod capability;
pub use capability::*;

#[path = "real_driver_contract_free.rs"]
mod free;

#[path = "real_driver_contract_scope.rs"]
mod scope;

#[path = "real_driver_contract_rule.rs"]
mod rule;

#[path = "real_driver_contract_live.rs"]
mod live;

#[path = "real_driver_contract_live_faults.rs"]
mod live_faults;

#[path = "real_driver_contract_probe.rs"]
mod probe;
pub use probe::*;

#[path = "real_driver_contract_refusal.rs"]
mod refusal;

/// §10.2 rule 5 as a regression guard: no Rust source under this driver crate's
/// `tests/`, and no shared template source, may name an env file as a **string
/// literal** or call a dotenv-style loader. Prose that mentions such a file in
/// backticks stays allowed, so the guard cannot be satisfied by deleting words
/// from the docs, and it cannot be defeated by a comment.
#[test]
fn test_sources_never_read_env_files() {
    assert!(
        file!().ends_with("http-support/tests/support/real_driver_contract.rs"),
        "this file is not the shared template (file!() = {})",
        file!()
    );

    let mut sources = template_sources();
    assert!(
        sources.iter().all(|path| path.is_file()),
        "the shared template must be reachable from the driver crate: {sources:?}"
    );
    let crate_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    let scanned = collect_rs(&crate_tests).unwrap_or_else(|e| {
        panic!(
            "the env-file guard could not scan {}: {} — an unreadable directory is \
             never reported as a clean one",
            crate_tests.display(),
            describe_scan_failure(&crate_tests, &e)
        )
    });
    sources.extend(scanned);
    assert!(
        sources.len() > 3,
        "the guard found no driver sources to scan"
    );

    for path in &sources {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        if let Some(token) = env_guard_violation(&content) {
            panic!(
                "{} must not contain {token:?} — credentials come from the process environment only",
                path.display()
            );
        }
    }
}
