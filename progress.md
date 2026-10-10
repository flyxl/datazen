# P2 handle-closer — progress ledger

Track: host per-handle closing capability at the **driver-api contract layer**, scoped to the
basic four drivers (postgres, mysql, sqlite, redis).
Base: `f5a33b7dc2d4cfd1f033686bdd33a9ebd8aad0b5` (`feat/p2-p3-p4-completion`).
Worktree: `.worktrees/datazen-p2-handle-closer` (`feature/p2-handle-closer`).
**This ledger is committed with the branch and must be deleted by the coordinator at merge.**

## Done

### Contract

- `packages/driver-api/src/lib.rs:109` — `PROTOCOL_VERSION` 4 → 5. `:115` `MIN_PROTOCOL_VERSION`
  stays 1 (rationale in "Pending adjudication").
- `packages/driver-api/src/resource.rs:643` — `async fn close_handle(&self, handle: &ResourceHandle,
  session_handle: &SessionHandleRef) -> Result<(), ResourceError>` added to `trait ResourceProvider`
  **with no default body**, so every out-of-tree `impl` must answer it. Trait is now 15 required
  methods = 12 async + 3 accessors.
- `packages/driver-api/src/resource_no_default_bodies_tests.rs` — `EXPECTED_METHODS` 14 → 15,
  `EXPECTED_ASYNC_METHODS` 11 → 12. This is the machine check that keeps the no-default rule.
- `packages/driver-api/Cargo.toml` — `0.0.10` → `0.0.11`.

### Basic four drivers

| Driver | `close_handle` | What it does | Capability evidence |
| --- | --- | --- | --- |
| postgres | `src/resource/provider/contract.rs:372` | **Really closes**: checks ownership, then takes the named transaction out of the live record and awaits `rollback_impl` on the pinned connection. Only `Transaction` handles exist, so `Cursor`/`ServerPrepared` are refused by name. | `session_scoped_handles = Supported` (`src/resource/capabilities.rs:134`) |
| mysql | `src/resource.rs:796` | **Refuses** — issues no session-scoped handle at all. | `Unsupported` (`src/resource.rs:278`) |
| sqlite | `src/resource.rs:702` | **Refuses** — same shape. | `Unsupported` |
| redis | `src/resource/provider/contract.rs:388` | **Refuses** — no session-scoped handle. | `Unknown` (`src/resource/capabilities.rs:112`) — the refusal text says `Unknown`, not `Unsupported` |

Every impl calls `handle.check(...)` **first**, so a foreign handle yields
`ResourceOwnershipMismatch` rather than a generic "unsupported". Every refusal names the
`handle_id` and the `kind` and explains that `Ok(())` there would read as "closed".

### Forced by the no-default rule (minimum honest edit, not the P2 migration)

`resource_adapter.rs:644` (legacy adapter), `tests/support/mod.rs:576` (`FakeProvider`), and
mongodb / hbase / vector impls had to grow the method. Those three non-basic-four drivers got the
same "I issue no such handle" refusal, nothing more. The full 15-driver `ResourceProvider`
migration is **still outstanding** and is out of this track.

### Version bumps forced by the release gate

All 15 path-driver `Cargo.toml`s went `0.0.8` → `0.0.9`, because **no path driver overrides
`DatabaseDriverFactory::protocol_version()`** — the default body returns
`crate::PROTOCOL_VERSION`, so bumping the protocol made all 15 "recompiled" and therefore
"released-in-this-commit" for the gate. Without the bumps the release gate fails with
`protocol-moved-without-driver-release` + `recompiled-driver-not-released`.

## Gates — commands and verbatim conclusion lines

Worktree sha guard, HEAD and `git diff | shasum` taken before and after every gate run; identical
at both ends (`e391f9dfe4873c060a576de95e41bf75882368a1`), HEAD `f5a33b7d…` throughout.

| Gate | Command | Exit | Conclusion |
| --- | --- | --- | --- |
| A1 | `cargo check --all-targets` | 0 | `Finished dev profile [unoptimized + debuginfo] target(s) in 7.73s` |
| A2 | `cargo fmt --all -- --check` | 0 | no output |
| B1 | `node scripts/check-driver-protocol-compat.mjs` | 0 | `[breaking] resource-provider: required trait item(s) added to pub trait ResourceProvider with no default body: close_handle` / `[check-driver-protocol-compat] ok (base f5a33b7…; PROTOCOL_VERSION 4 -> 5, MIN_PROTOCOL_VERSION 1 -> 1, crate 0.0.10 -> 0.0.11).` |
| B2 | `node scripts/check-driver-protocol-release.mjs` | 0 | `[check-driver-protocol-release] ok (base f5a33b7…; PROTOCOL_VERSION 4 -> 5, MIN_PROTOCOL_VERSION 1 -> 1, datazen-driver-api 0.0.10 -> 0.0.11; window MIN_PROTOCOL_VERSION 1..=PROTOCOL_VERSION 5; driver releases: 15)` |
| C | `cargo test -p datazen-driver-api` | 0 | `ok. 332 passed; 0 failed` (lib) · `ok. 14 passed; 0 failed` (`--test resource_contract`) |
| C | `cargo test -p datazen-driver-mysql` | 0 | `ok. 182 passed; 0 failed` + 9 binaries ok |
| C | `cargo test -p datazen-driver-sqlite` | 0 | `ok. 93 passed; 0 failed` + 6 binaries ok |
| C | `cargo test -p datazen-driver-redis` | 0 | `ok. 405 passed; 0 failed; 3 ignored` + 8 binaries ok |
| C | `cargo test -p datazen-driver-postgres` | **101** | 19 of 20 binaries `ok`, incl. `postgres_resource_close_handle` `ok. 9 passed; 0 failed`; **one pre-existing live failure** — see below |
| D | `cargo test -p datazen --lib` | 0 | `ok. 1810 passed; 0 failed; 6 ignored` — identical to the pre-change baseline, no new failures |
| F | `git status --porcelain` | — | only the 42 intended files + the 3 new test files. `src-tauri/Cargo.toml` untouched. `Cargo.lock` has **16 changed lines, all `version = `** — no `resolve-drivers.mjs` codegen pollution |

### The one postgres failure is pre-existing, proven at the base commit

`postgres_cross_database::cross_database_reads_never_move_the_session` panics at
`packages/drivers/postgres/tests/postgres_cross_database.rs:305`:

```
seed probe table dz_cross_db_probe_… in dz_fixture_pg_a: Query failed: error returned from
database: permission denied for schema public
```

Proven pre-existing, not a regression: the same test was run from a throwaway detached worktree
at `f5a33b7dc2d4cfd1f033686bdd33a9ebd8aad0b5` (`git worktree add --detach /tmp/datazen-p2-base-check`,
same `CARGO_TARGET_DIR`) and failed with the byte-identical message; the worktree was then removed.
It is a live test whose credentials come from `TEST_PG_*` in the process environment, and the local
Postgres role cannot `CREATE` in `schema public`. It never touches `close_handle`.

## Tests added (criterion E)

- `packages/drivers/postgres/tests/postgres_resource_close_handle.rs` (new, 597 lines) — 9 offline
  tests + a gated live module. Covers: refusal when no resource is open, refusal by kind name,
  foreign-provider handle refused before the ref is read, retired-epoch handle, retired-epoch ref,
  ref naming a different resource, **closed resource handle ⇒ no-op success**, every refusal leaves
  the handle open.
- `packages/drivers/redis/src/resource/tests_close_handle.rs` (new, 157 lines) — 4 tests: the
  refusal is `OperationNotSupported` naming the id and kind and the handle stays open, refusal for
  every `SessionHandleKind`, ownership checked before the refusal, budget untouched.
- `packages/drivers/mysql/src/resource_tests.rs` — 3 tests; also asserts nothing reached the wire
  (`wire.statements.is_empty() && wire.commands.is_empty()`).
- `packages/drivers/sqlite/src/resource_tests.rs` — 3 tests; asserts `wire.statements.is_empty()`.
- `packages/driver-api/src/resource_adapter_close_handle_tests.rs` (new, 98 lines) — the legacy
  adapter refuses instead of faking, checks ownership before refusing, and refuses every kind.
- `packages/driver-api/tests/resource_contract.rs` — 6 new tests for the `FakeProvider` reference
  implementation, including "close after the resource closed is a no-op success".

## Pending adjudication

1. **`MIN_PROTOCOL_VERSION` left at 1.** No gate rule requires MIN to move with PROTOCOL;
   `docs/architecture/platform/driver-capability-migration.md:362` treats them as separate; the
   number already went 1→2→3→4 with MIN pinned. Consequence: **a driver declaring protocol 1–4 still
   loads in degraded mode despite having no `close_handle` semantics at all**, so the runtime wiring
   track must treat a missing/unknown closer as fail-closed — which is exactly what `NoHandleCloser`
   already does, and runtime wiring is out of scope here. Raising MIN is a product decision about
   which old plugins keep loading.
2. **Protocol bump vs. "source-only break".** `driver-capability-migration.md:413-421` boundary one
   arguably says a source-only break should *not* move the protocol number — but the gate's
   `resource-provider` rule classifies a required trait item as `breaking` and CI is a hard gate, so
   the bump is forced. Doc and gate disagree; **the doc was not "fixed"** (out of scope, and the
   coordinator owns the doc).
3. **`datazen-driver-api` 0.0.10 → 0.0.11 is a judgment call.** The `additive-change-requires-crate-bump`
   rule fires only for the `additive` class; for `breaking` only the protocol bump is required. The
   crate bump is conservative because the crate is published.
4. **Interface mismatch for the next track.** `HandleCloser::close(&SessionHandleRef)`
   (`packages/runtime/src/resource/lease_manager.rs:65-68`) is **sync and ref-only**, while
   `ResourceProvider::close_handle(&ResourceHandle, &SessionHandleRef)` is **async and needs the
   resource handle**. The wiring must obtain the `ResourceHandle` for the lease to build the closer
   closure. Note `LeaseFacade::release` (`:431-455`) loops `closer.close(&handle)?` — any `Err`
   aborts the whole release and deregistration happens only after a successful close, so
   "already closed ⇒ `Err`" is safe from the host side.

## Leftovers / explicitly out of scope (not done, not faked)

- src-tauri `with_live_connections` / `start_idle_reclaimer` have 0 callers.
- The 15 path drivers still have no real `ResourceProvider` migration (deferred to P10); three of
  them got the minimum refusal here only so the workspace compiles.
- Whether `ResourceError` needs an `Unsupported` variant.
- `src-tauri/src/db/registry.rs:134-140` only **warns** when `pv > PROTOCOL_VERSION` instead of
  refusing, so "a newer driver is rejected" is not currently true.
- `connection-management.md:657` second half ("reset unsupported, failed or timed out ⇒ close")
  is not implemented.

## Known process caveat

The parent agent independently ran cargo commands in this same worktree during development
(`cargo check --workspace --all-targets` EXIT=0, `cargo test -p datazen-driver-api` 329 passed).
Per AGENTS.md a worktree must not be shared by committer and verifier. Mitigated by recording HEAD
and the working-tree sha before and after **every** gate run in this ledger; they matched at both
ends of every run.
