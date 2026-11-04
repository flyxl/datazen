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
//! # Why strict is the default
//!
//! `test result: ok. 1 passed` cannot distinguish "the check passed" from "the
//! check never ran". A test that prints a skip line to stderr and returns is
//! indistinguishable from one that really proved the property, in the one place
//! anyone actually looks — the summary line. That is how an empty fixture
//! database produced a green cross-database run that had verified nothing, and
//! it is why the old default was "lenient unless someone remembered a flag":
//! forgetting the flag was the failure mode, and forgetting is free.
//!
//! So the default now flips. **A run that cannot verify a dimension fails that
//! dimension.** The escape hatch is `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1`, and
//! it is a name you have to type on purpose, in a script or a shell, that says
//! out loud "this run has no database and I know it".
//!
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1` keeps working and still means strict. It is
//! no longer *how* you get strict — that is now the default — but every existing
//! command, CI job and `scripts/run-live-contract.sh` invocation that already
//! exports it keeps its meaning untouched. Only the exact string `1` arms it, and
//! only the exact string `1` waives it: `"0"`, `"true"`, `"yes"`, `"01"`,
//! `"11"` and a stray path all arm nothing, in either variable. An opt-in that
//! accepts near-misses is an opt-in that gets ignored by accident while still
//! reading as if it were asked for.
//!
//! When both are set, `1` in `DATAZEN_CONTRACT_REQUIRE_LIVE` wins. A demand for
//! real-protocol conclusions is not something a blanket waiver should be able to
//! cancel.
//!
//! # What this costs
//!
//! Strict-by-default cannot distinguish "I have no database here" from "the
//! server is unreachable" from "the connection hook is hung". All three now go
//! red instead of quietly green. That is the intended trade: the cost is a red
//! run on a machine that has no fixtures, and the payment is that a green run
//! means something. Operators who knowingly have no database pay it with the one
//! explicit line above; operators who *think* they have a database and do not
//! finally find out.

// This file is pulled in by `#[path]` into several different test binaries, and
// no single one of them calls all of these. Without this, every consumer pays a
// `dead_code` warning it has no way to fix. It has to sit above every item: an
// inner attribute after an item is a hard parse error, not a warning.
#![allow(dead_code)]

/// Environment variable that demands real-protocol conclusions. Still honoured;
/// strict is now what you get without it.
pub const REQUIRE_LIVE_VAR: &str = "DATAZEN_CONTRACT_REQUIRE_LIVE";

/// The explicit opt-out: "this run knowingly has no database, let the
/// unverifiable dimensions skip and say so". Nothing but the exact string `1`
/// waives strictness.
pub const ALLOW_UNVERIFIED_VAR: &str = "DATAZEN_CONTRACT_ALLOW_UNVERIFIED";

/// Pure decision: does this raw `DATAZEN_CONTRACT_REQUIRE_LIVE` value ask for
/// strict mode?
///
/// Split out from [`strict_live`] so it can be tested without mutating the
/// process environment (which is process-global and racy against every other
/// test in a binary running on its own thread).
///
/// Only the exact string `1` arms it, so an accidentally-present variable — an
/// empty value, a path, someone's `DATAZEN_CONTRACT_REQUIRE_LIVE=/opt/thing` —
/// cannot quietly turn every skipped dimension into a failure.
pub fn strict_live_requested(value: Option<String>) -> bool {
    value.map(|raw| raw.trim() == "1").unwrap_or(false)
}

/// Pure decision: does this raw `DATAZEN_CONTRACT_ALLOW_UNVERIFIED` value waive
/// strict mode? Same exact-string rule, same reason for it.
pub fn allow_unverified_requested(value: Option<String>) -> bool {
    value.map(|raw| raw.trim() == "1").unwrap_or(false)
}

/// Pure combination of both environment values, in the order the operator set
/// them, with no environment access of its own.
///
/// Strict wins: `DATAZEN_CONTRACT_REQUIRE_LIVE=1` beats
/// `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1`. Anything else — both unset, only the
/// waiver set, near-miss strings — resolves to strict. There is deliberately no
/// combination of inputs that yields "lenient because someone did not tell us".
pub fn strict_live_decision(
    allow_unverified: Option<String>,
    require_live: Option<String>,
) -> bool {
    strict_live_requested(require_live) || !allow_unverified_requested(allow_unverified)
}

/// Reads the strict/lenient decision from the process environment.
///
/// Process environment only, by construction: this module never opens a `.env`
/// file, never parses one, and never has a value to leak. The program may be
/// *given* its configuration by a launcher; that is the launcher's business.
pub fn strict_live() -> bool {
    strict_live_decision(
        std::env::var(ALLOW_UNVERIFIED_VAR).ok(),
        std::env::var(REQUIRE_LIVE_VAR).ok(),
    )
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
/// result the operator has to see. The panic names the dimension and the reason,
/// because a failure that does not say what was missing is not actionable — and
/// it names the opt-out, because the person reading a red run on a laptop with
/// no database needs to know what the run they actually wanted looks like.
pub fn unverified_or_fail(label: &str, dimension: &str, reason: &str, strict: bool) {
    report_unverified(label, dimension, reason);
    if strict {
        panic!(
            "{dimension}（{label}）未验证：{reason} — live 门禁默认严格：\
             无法验证就是失败，跳过不记为通过。\
             本地确实没有数据库时，显式声明 {ALLOW_UNVERIFIED_VAR}=1 才允许跳过。"
        );
    }
}
