# Redis Driver Depth PRD

> **Status**: Active  
> **Owner**: Redis driver track  
> **Last updated**: 2026-09-14  
> **Related**: [Execution Plan](./redis-driver-depth-execution-plan.md) · PR #30 (PR-1 complete)

## 1. Background & Goals

DataZen uses **compile-time Driver injection** (`inventory` + `drivers-registry.json`). Redis lives entirely inside `packages/drivers/redis` (Rust + `ui/` + e2e + permissions). Host only sees generic contracts (`DatabaseDriver`, `DriverCommandDefinition`).

**Goal**: Close the functional gap with DBX Redis workbench while leveraging DataZen strengths (AI / Workflow / MCP / security gates / local-first).

**Non-goals**:
- Do not push Redis-specific logic into Host core.
- Do not aim for pixel-perfect UI clone of DBX; prioritize DataZen interaction model and safety.

### Success metrics (high level)
- Key browser: SCAN-only, tree/flat, incremental, type-aware summary.
- Type editors: String (incl. decompress), Hash/List/Set/ZSet (paged + search), Stream (groups + pending).
- TTL: relative / absolute / KEEPTTL / persist — already partially shipped in PR-1.
- Console + Monitor + Pub/Sub production-ready.
- Dangerous commands gated; readonly mode enforced.
- AI/Workflow/MCP can safely operate on Redis keys and streams.

## 2. Architecture Constraints & Opportunities

| Constraint | Implication |
|------------|-------------|
| Compile-time registration | All Redis depth stays in the Redis driver package |
| `DatabaseDriver` + command defs | New capabilities = new `DriverCommandDefinition`s + UI contribution |
| Generated `src/extensions/generated.ts` | Driver UI pages inject via registry |
| Existing security / readonly / MCP | Reuse Host gates; map Redis danger levels to them |

**Opportunities**:
- Independent evolution (even future split to `datazen-driver-redis` repo).
- Amplify with AI (NL → Redis command, Stream lag diagnosis) and Workflow steps.
- MCP exposure of safe read paths for agents.

## 3. Current State (as of PR-1)

### Done (PR-1 — on feature branch, ready for review)
- Absolute expiry: `EXPIREAT` (unix timestamp).
- `SET … KEEPTTL` (Redis ≥ 6.0).
- String editor: keepTtl checkbox, expireAt datetime picker, gzip/zlib/deflate decompress view + JSON pretty-print.
- Module split for maintainability (ops, commands_exec_mutate/ops, redis_driver_db/kv, value preview/rows, individual Hash/List/Set/Zset editors).
- Unit tests (ops TTL/ExpireAt, stringKeyValue decompress, keyEditorsInvokes) + e2e RD-025/RD-026.

### Existing baseline (pre-PR-1)
- Key browser (basic), command console, monitoring, Pub/Sub skeleton.
- Basic type handling for string/hash/list/set/zset.

### Gaps vs DBX (priority order)
1. Incremental SCAN + tree namespace + memory estimate + type filter.
2. Full type editors (paged HSCAN/SSCAN/ZSCAN, Stream consumer groups).
3. Console danger classification + structured results.
4. Real-time MONITOR stream.
5. Production protection (batch ops, FLUSH* gates, readonly).
6. Differentiation: AI/Workflow/MCP Redis depth.

## 4. Phased Requirements

### Phase 1 — Core Key Browse & Type Editors (highest priority)

**P1.1 Incremental Key Space**
- Force `SCAN` (MATCH / COUNT / TYPE); never default `KEYS *`.
- Tree (namespace) ↔ flat list toggle.
- Cursor-based “load more”; interruptible full scan.
- Columns: key, type, TTL, approx memory, value summary.

**P1.2 Type-specific Viewers/Editors**
- **String**: text / JSON format / binary escape / decompress (gzip/zlib/deflate) — *decompress + KEEPTTL/expireAt done in PR-1*.
- **Hash / List / Set / ZSet**: HSCAN/SSCAN/ZSCAN pagination, field/member search, sort, CRUD.
- **Stream**: XRANGE/XREVRANGE + **XINFO GROUPS/CONSUMERS**, pending, lag, delivery count.
- Optional: RedisJSON tree/source if module present.

**P1.3 TTL Fine Control** (mostly done)
- Relative seconds, absolute timestamp, KEEPTTL, persist (remove expiry).
- Default KEEPTTL on value edit.
- Batch TTL apply (Phase 3 stretch).

### Phase 2 — Console, Monitor, Pub/Sub, Instance Observation

- CLI-style console + history + structured table/JSON results.
- Danger levels: confirm / block by default (align with Host security).
- Dedicated MONITOR connection (stream last N commands, stoppable).
- Pub/Sub panel: subscribe / psubscribe, live messages, publish, clear.
- Dashboard widgets: INFO sections, Slowlog, MEMORY USAGE per key.

### Phase 3 — Safety, Batch, Production Guardrails

- Readonly connection + production environment flag.
- Multi-select delete, batch TTL, FLUSHDB/FLUSHALL with strong confirm.
- MCP permission split: normal write vs ultra-dangerous (`FLUSH*`, `CONFIG`, `SHUTDOWN`).
- DB alias (display only; does not affect SELECT).

### Phase 4 — Differentiation (DataZen advantages)

- NL → Redis command / pattern suggestions.
- Stream lag / consumer group diagnosis via AI.
- Workflow steps: periodic SCAN, lag check, expired-key cleanup, cross-DB (Redis + PG).
- Ops Dashboard widgets + threshold alerts.
- MCP safe read surface for Cursor/Claude agents.
- Smarter decompress + JSON (AI-assisted).

## 5. Non-Functional Requirements

- Large keyspace / large Stream: cursor + cancel, no UI freeze.
- All Redis-specific commands live in driver; Host remains generic.
- Tests: unit (ops / parse) + e2e (SCAN, editors, danger gates, MONITOR).
- i18n (en + zh-CN) for every new string.
- Module size discipline: prefer ≤ ~800 LOC per file; split by functional unit.

## 6. Out of Scope (for now)

- Cluster topology management UI (future).
- Redis Modules marketplace.
- Full RedisJSON / RediSearch / RedisTimeSeries first-class editors (only basic detect + view).
- Cloud Redis vendor-specific dashboards.

## 7. Acceptance Criteria (Phase 1 complete)

- [x] PR-1: KEEPTTL + EXPIREAT + decompress UI + tests.
- [ ] SCAN-only browser with tree/flat + load-more.
- [ ] Hash/List/Set/ZSet editors support pagination + search + CRUD.
- [ ] Stream viewer shows consumer groups / pending / lag.
- [ ] TTL controls available on all key types (relative + absolute + persist).
- [ ] e2e covering the above paths.
- [ ] No Host core pollution; all commands registered via driver.

## 8. References

- DBX Redis docs (feature checklist): https://dbxio.com/cn/docs/redis
- DataZen driver architecture: `docs/architecture/backend/drivers.md`, `docs/development/independent-driver-development.*.md`
- PR-1: https://github.com/flyxl/datazen/pull/30
