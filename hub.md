# P3 / P4 completion and P6 start

Integration branch: `codex/p3-p4-p6-runtime`. Work is in progress; these phases are not accepted.

## Active tracks

All tracks start from `246835a9c`; the coordinator handles review, integration and WDIO acceptance.

| Track | Agent | Branch | Scope | State |
| --- | --- | --- | --- | --- |
| Drivers | `/root/p346_drivers` | `codex/p346-drivers` | PostgreSQL/MySQL fixed physical sessions and required neutral driver hooks | Coding |
| Runtime | `/root/p346_runtime` | `codex/p346-runtime` | P3 production ArtifactStore, resource policy and transaction handle lifecycle | Coding |
| Desktop | `/root/p346_desktop` | `codex/p346-desktop` | P4 query/table/metadata/result consumers and existing WDIO assertions | Coding |

Worktrees are `.worktrees/datazen-p346-{drivers,runtime,desktop}`. No track may edit another checkout. Review/verification must use a separate worktree from the coder.

## Existing implementation changes

- Query execution, transactions and cancellation now use pane-owned session APIs.
- Closing query tabs preserves UI ownership when backend closure is refused.
- Runtime event delivery and replay are bounded; expired settled executions and issued tokens are reclaimed.
- Cancel submission tokens are checked at the desktop IPC boundary.
- Table ChangeSets use an independent session, parameterized writes, short transaction and explicit cleanup.
- Table previews bind configuration revision, rechecked before writes.
- Workflow SQL steps use executeAtTarget and request cancellation when their future is dropped.

## Evidence

- Rust Host check: EXIT=0 in `/tmp/datazen-p346-final-check.log`.
- Frontend typecheck: EXIT=0 in `/tmp/datazen-p346-final-tsc.log`.
- Check HEAD: `d5d3fdaa2cf73956d79c4f9bd42e5b310b021722`; start/end source SHA: `50086c13d6925886a096b5208383613486583bbcbaee078cebaa966b57451bf6`.
- WDIO build with basic drivers and webdriver: EXIT=0 in `/tmp/datazen-p346-wdio-build.log`; start/end source SHA: `5139052717e502b0e6b1c8b1ce3cddc73e96e4a6e6f887072e67bcf9129d488c`.
- Selected SQL WDIO: EXIT=0, 1 passing. Its body-text assertion can match SQL editor text, so this is NOT proof that the new execution route worked.

## Remaining acceptance work

- Driver fixed physical-session support for PostgreSQL/MySQL; both currently declare stateful editor sessions unsupported.
- Host resource scope must consume provider connection-cost policy rather than constant 1.
- Runtime transaction handle registration/retirement must match driver completion observations, avoiding duplicates and stale handles.
- ArtifactStore concrete production implementation, durability, TTL and bounded result consumption.
- Table reads/metadata caching, result provenance/writeback and session lifecycle compatibility.
- P6 SQL cancellation and isolation acceptance; Workflow blocks and other P6 consumers remain unimplemented.

Only WDIO tests are authorized. No Vitest/Cargo tests or computer-use testing. Never read env file contents; test programs may load configuration privately.

This ledger must be deleted before delivery/merge to main.
