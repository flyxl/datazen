//! The one skip-or-fail gate shared by every live test.
//!
//! This module is deliberately standalone — no driver contract, no `CONTRACT`
//! label, no driver imports — so that *any* integration test can reach it with
//! `#[path]` without dragging in the contract template. That matters because the
//! tests that most need it are the ones that were never part of the template:
//! the migration suites and the resource-budget suite sat behind a static
//! `#[ignore]` for their whole life, which cargo reports as neither passing nor
//! failing. A gate nobody can call is a gate that cannot work.
//!
//! # Why a skip must be able to become a failure
//!
//! `test result: ok. 1 passed` cannot distinguish "the check passed" from "the
//! check never ran". A test that prints a skip line to stderr and returns is
//! indistinguishable from one that really proved the property, in the one place
//! anyone actually looks — the summary line. That is how an empty fixture
//! database produced a green cross-database run that had verified nothing.
//!
//! So the default stays lenient, because CI has no fixtures and a machine
//! without a database must not fail the whole suite. Setting
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1` is what makes "green" mean "verified":
//! under it, an unverifiable dimension panics instead of returning.
//!
//! The exact string `1` is the only thing that arms it. `"0"`, `"true"`,
//! `"yes"`, `"01"` and `"11"` must all stay lenient — an opt-in that accepts
//! near-misses is an opt-in that gets ignored by accident while still reading
//! as if it were asked for.

// This file is pulled in by `#[path]` into several different test binaries, and
// no single one of them calls all four functions. Without this, every consumer
// pays a `dead_code` warning it has no way to fix.
#![allow(dead_code)]

/// Pure decision behind strict mode, split out so it can be tested without
/// mutating the process environment (which is process-global and racy against
/// every other test in a binary running on its own thread).
pub fn strict_live_requested(value: Option<String>) -> bool {
    value.map(|raw| raw.trim() == "1").unwrap_or(false)
}

/// Reads the strict-mode flag from the process environment.
///
/// Process environment only, by construction: this module never opens a `.env`
/// file, never parses one, and never has a value to leak. The program may be
/// *given* its configuration by a launcher; that is the launcher's business.
pub fn strict_live() -> bool {
    strict_live_requested(std::env::var("DATAZEN_CONTRACT_REQUIRE_LIVE").ok())
}

/// Prints the honest skip line: which dimension is therefore **unverified** and
/// why. No fake stands in for a real-protocol conclusion.
pub fn report_unverified(label: &str, dimension: &str, reason: &str) {
    eprintln!(
        "⏭  {dimension} 未验证（{label}）：{reason} — 该维度需要真实数据库，本次运行不产生任何真实协议结论。"
    );
}

/// The choke point every skip site must route through.
///
/// Under strict mode this panics, so an unverifiable dimension becomes a red
/// result the operator has to see. The panic names the dimension and the
/// reason, because a failure that does not say what was missing is not
/// actionable.
pub fn unverified_or_fail(label: &str, dimension: &str, reason: &str, strict: bool) {
    report_unverified(label, dimension, reason);
    if strict {
        panic!(
            "{dimension}（{label}）未验证：{reason} — 已显式要求真实协议结论 \
             （DATAZEN_CONTRACT_REQUIRE_LIVE=1），因此跳过不是一个结果。"
        );
    }
}
