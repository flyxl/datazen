//! The standing structural rule for `Supported` capability bindings.
//!
//! **The rule.** A capability declared `Supported` must have something behind
//! it that can fail at runtime. Exactly two things qualify:
//!
//! 1. a `DatabaseDriver` trait method the driver actually implements, or
//! 2. a **branch consumer** — a `match`/`if` on the field whose body carries
//!    assertions.
//!
//! Two things that look like enforcement but are not:
//!
//! - Membership in the `cases` array of `real_driver_contract_free.rs`. That
//!   table only ever hands a binding to `verdict_for(capability, refusal)`,
//!   which compares the binding against the refusal it was handed. Both arms
//!   pass, so flipping the field changes nothing.
//! - An `eprintln!` inside a *passing* test. Cargo captures it and discards
//!   it, so the notice never reaches a human.
//!
//! **Why the rule exists.** `reset_for_reuse` was bound `Supported` in both
//! the postgres and mysql contract bindings, while all 15 driver crates
//! declare `ResetForReuse::Unsupported` in production
//! (`resource_capabilities.rs` / `resource/capabilities.rs`) and no live or
//! fault file has a `match`/`if` on the field at all — the only hit in those
//! files was a comment recording that the branch used to exist. The binding
//! was therefore unenforceable while being reported as a verified dimension.
//!
//! **Scope note.** This rule does *not* ask whether the capability ought to be
//! supported. It asks only whether a `Supported` binding can be observed. That
//! is the narrower question the P2 exit gate asks when it requires that a
//! missing capability never passes silently.
//!
//! # Known gaps this rule does NOT close
//!
//! These stay open on purpose. Recording them here rather than in `docs/`
//! keeps them next to the rule they qualify, in a file no other track edits.
//!
//! - **Gap A — `reset_for_reuse` has no trait method at all.** The rule accepts
//!   a *branch consumer* as enforcement, so it stays satisfied if someone later
//!   adds a branch — but no `DatabaseDriver` method observes this capability,
//!   so nothing outside this test suite can act on it. Closing it means adding
//!   a public trait method, which is a `PROTOCOL_VERSION` bump (currently 4) and
//!   touches every `impl DatabaseDriver` in the workspace. That is a separate,
//!   larger change and **not** authorised here.
//! - **Gap B — a partial obligation is announced where nobody can see it.**
//!   `report_partial_obligation` writes an `eprintln!`, but it is called from
//!   inside a *passing* test, so cargo captures the output and discards it;
//!   the call site also sits after the `open_live` early return, so it does not
//!   execute at all without a live server. Deleting a branch that can never be
//!   taken is not a fix — the call site is the defect.

use crate::Contract;

/// Sources that a branch consumer may live in, embedded so an anchor is
/// checked against the real text at test time rather than against a comment
/// claiming the text is there.
const LIVE: &str = include_str!("real_driver_contract_live.rs");
const LIVE_FAULTS: &str = include_str!("real_driver_contract_live_faults.rs");

fn source(label: &str) -> &'static str {
    match label {
        "live" => LIVE,
        "live_faults" => LIVE_FAULTS,
        other => panic!("unknown anchor source `{other}`; add it to `source()`"),
    }
}

/// One capability field and the branches that enforce it.
pub struct Enforcement {
    /// The `Contract` field name.
    pub capability: &'static str,
    /// Control-flow branches on the field, as `(source label, exact source
    /// text)`. The text is matched against the embedded file so a renamed or
    /// deleted branch fails instead of silently weakening the row.
    pub branches: &'static [(&'static str, &'static str)],
}

/// Every governed field that *does* have a branch consumer.
///
/// `reset_for_reuse` is deliberately absent: it has no consumer anywhere, so
/// binding it `Supported` is what this rule is built to catch.
pub const ENFORCED: &[Enforcement] = &[
    Enforcement {
        capability: "transactions",
        branches: &[("live_faults", "match crate::CONTRACT.transactions {")],
    },
    Enforcement {
        capability: "precise_cancel",
        branches: &[("live_faults", "match crate::CONTRACT.precise_cancel {")],
    },
    Enforcement {
        capability: "session_scoped_state",
        branches: &[("live", "match crate::CONTRACT.session_scoped_state {")],
    },
    Enforcement {
        capability: "read_snapshots",
        branches: &[(
            "live_faults",
            "crate::CONTRACT.read_snapshots != Capability::Supported,",
        )],
    },
    Enforcement {
        capability: "per_database_resource",
        branches: &[
            ("live", "if contract.per_database_resource {"),
            ("live_faults", "if contract.per_database_resource {"),
        ],
    },
];

/// The contract fields this rule governs, each with whether *this* driver
/// declares it present. The bool is `false` for a `Capability` bound to
/// anything other than `Supported`, and for a `bool` bound to `false`.
pub fn declared_support(contract: &Contract) -> Vec<(&'static str, bool)> {
    vec![
        ("transactions", contract.transactions.is_present()),
        ("precise_cancel", contract.precise_cancel.is_present()),
        (
            "session_scoped_state",
            contract.session_scoped_state.is_present(),
        ),
        ("read_snapshots", contract.read_snapshots.is_present()),
        ("reset_for_reuse", contract.reset_for_reuse.is_present()),
        ("per_database_resource", contract.per_database_resource),
    ]
}

/// Every `Supported` binding must have a branch consumer or a trait method
/// behind it; otherwise the declaration cannot be enforced and is, for the
/// purposes of the gate, no declaration at all.
#[test]
fn a_supported_binding_must_be_enforceable() {
    let support = declared_support(&crate::CONTRACT);

    // Anti-vacuity 1: the rule must actually be looking at the field list.
    // If a future refactor renames or drops a governed field, this fails
    // rather than letting the rule quietly govern nothing.
    assert_eq!(
        support.len(),
        6,
        "the governed field list no longer matches the `Contract` struct; update \
         `declared_support` and the anchors in `ENFORCED` together"
    );

    // Anti-vacuity 2: the enforcement table must be populated. An empty table
    // would pass this test for a contract that declares nothing.
    assert!(
        !ENFORCED.is_empty(),
        "`ENFORCED` is empty, so every rule below is vacuous"
    );

    for (name, present) in support {
        if !present {
            continue;
        }
        let row = ENFORCED
            .iter()
            .find(|e| e.capability == name)
            .unwrap_or_else(|| {
                panic!(
                    "`{name}` is declared `Supported`, but nothing behind it can fail: no \
                 `DatabaseDriver` method observes the value, and no live or fault file has a \
                 `match`/`if` on the field. Such a declaration is unenforceable, so it may not \
                 be counted as verified. Either give it a branch consumer carrying assertions, \
                 or bind it `Unsupported` to match what the driver actually does."
                )
            });
        assert!(
            !row.branches.is_empty(),
            "`{name}` has an `ENFORCED` row with no anchor, which proves nothing"
        );
        for (label, snippet) in row.branches {
            assert!(
                source(label).contains(snippet),
                "`{name}` claims enforcement in {label} at `{snippet}`, but that text is not in \
                 the file. The anchor has rotted: the row no longer proves the branch exists."
            );
        }
    }
}

/// The reverse direction: every row must name a field the rule actually
/// governs. Without this, a typo in `ENFORCED` would silently exempt a field
/// from the rule above.
#[test]
fn every_enforcement_row_names_a_governed_field() {
    let support = declared_support(&crate::CONTRACT);
    for row in ENFORCED {
        assert!(
            support.iter().any(|(name, _)| *name == row.capability),
            "`ENFORCED` has a row for `{}`, which is not a governed contract field; a typo \
             here would silently exempt that field from the rule",
            row.capability
        );
    }
}
