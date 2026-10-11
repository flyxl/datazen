# P3 / P4 completion and P6 start

Integration branch: `codex/p3-p4-p6-runtime`. Work is in progress; these phases are not accepted.

## Active tracks

All tracks start from `246835a9c`; the coordinator handles review, integration and WDIO acceptance.

| Track | Agent | Branch | Scope | State |
| --- | --- | --- | --- | --- |
| Drivers | `/root/p346_driver_cancel_fix` | `codex/p346-drivers` | PostgreSQL/MySQL fixed physical sessions and required neutral driver hooks | Coding |
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

Coordinator review is active. The resumed tracks remain unaccepted; their working diffs are not integrated.

- Committed integration baseline `c0f15b65319eb331af8ebe11b7c86c7b05fa5460`: direct TypeScript check EXIT=0 (`/tmp/datazen-p346-root-committed-tsc.log`); HEAD unchanged and tracked diff SHA at both boundaries `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
- Runtime review requested artifact recovery count/size validation, publication failure handling, and export capability owner/TTL binding.
- Runtime review found admission cancellation can orphan a target execution before its receipt; the runtime track is implementing owned admission and cleanup.
- Cross-track contract uses typed `commandResult` artifact output for catalog commands, not fabricated row columns. Driver and runtime implementations must be checked together.
- Host resource purpose must not be guessed from arbitrary `get_`/`list_` prefixes; runtime review requested validated command policy.
- Desktop acceptance must preserve concrete sanitized database diagnostics and the existing error-to-success recovery journey.
- Result writeback review: the baseline `ResultTableView` is a read-only adapter whose edit callback does not persist. Verified mapping and original-runtime writeback are an unimplemented P4 requirement, not an established baseline writeback regression.
- Coordinator found accidental removal of terminal session/context events; runtime confirmed and is restoring them. Target admissions also need per-key coordination instead of a global lock across connection waits.
- Fixed driver sessions must use per-resource locks; holding the transaction-map lock through SQL would serialize independent editors.
- Metadata migration must preserve existing typed `list_catalog` / `read_relation_columns` / `read_relation_schema` semantics, configuration-aware cache identity and bounded batches; switching to legacy `list_tables` is insufficient.
- Driver API dependency commit: `fec6c7fe2b3b82cfe8f63b56e3de9c37b201c597` (retained-session hooks and typed output); not yet independently accepted/integrated.

- Driver fixed physical-session support for PostgreSQL/MySQL; both currently declare stateful editor sessions unsupported.
- Host resource scope must consume provider connection-cost policy rather than constant 1.
- Runtime transaction handle registration/retirement must match driver completion observations, avoiding duplicates and stale handles.
- ArtifactStore concrete production implementation, durability, TTL and bounded result consumption.
- Table reads/metadata caching, result provenance/writeback and session lifecycle compatibility.
- P6 SQL cancellation and isolation acceptance; Workflow blocks and other P6 consumers remain unimplemented.

Only WDIO tests are authorized. No Vitest/Cargo tests or computer-use testing. Never read env file contents; test programs may load configuration privately.

This ledger must be deleted before delivery/merge to main.

## Resumed integration snapshot (2026-10-12)

- Driver implementation `0c107d0a9`, runtime `45bb4492a`, desktop `cd6496c7f` are committed; compile checks passed but independent live acceptance is incomplete.
- Root Workflow diagnostics follow-up `529606200` compiled successfully, not yet independently accepted.
- Isolated combined candidate `.worktrees/datazen-p346-candidate` at `aa6fe601d` includes exact API/helper dependencies, all three implementations, Workflow diagnostics, and WDIO preparation `aad272f3a` / `2f438abf1`. Only process-ledger add/add conflicts required resolution. Production source had no merge conflicts.
- Existing independent driver tester resumed against this explicit candidate for webdriver build and actual PG/MySQL journeys. WDIO has not yet run on this candidate.
- Driver cancellation paths hold a global execution lock across control network waits; repair coder is handling per-execution coordination and neutral early statementColumns production.
- Runtime coder resumed controlled artifact export IPC follow-up. Desktop confirmed autocomplete browsing, latest target reopen, and source-context write guards in committed code.
- New tester model request GPT-6 Luna ultra was rejected by tool: supported maximum is max. User clarification is pending; do not silently substitute a different model or effort. Existing tester continues its current round.
- Root integration checkout still contains only previously integrated baseline, not unaccepted candidate implementations.

## Agent settings and scheduled continuation

- User confirmed future testers: GPT-6 Luna, max. Fresh independent tester `/root/p346_combined_verify_r2` is active.
- User requests future coders: GPT-6.1 Sol, low; preserve currently running agents' work, use this setting when spawning coder instances.
- One-time heartbeat `05-35-p3-p4-p6` is ACTIVE for 2026-10-12 05:35 Asia/Shanghai, attached to this chat; it resumes unfinished work and agents, then closes itself.
- Updated combined candidate `74c61627509103636b6c7f0c57a6f2c55efeb6b6` includes driver cancellation fix `ec83ad3c9` and runtime export/quarantine source from final `e62798e67`; production source merged without conflicts.
- Older frozen candidate `aa6fe601d` webdriver build succeeded. Initial real WDIO 0 passed / 2 failed: PG reached cancellation IPC after earlier identity/parameter/temp/schema/600-row/transaction journeys; MySQL fixture profile missing. Tester is fixing only diagnostic wrapper and fixtures before completing the round. Neither candidate is accepted.
