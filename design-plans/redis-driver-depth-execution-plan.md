# Redis Driver Depth — Execution Plan

> **Companion**: [PRD](./redis-driver-depth-prd.md)  
> **Status**: Phase 1 in progress (PR-1 complete)  
> **Last updated**: 2026-09-14

## 0. Guiding Principles

1. **Driver-first**: All Redis logic stays in `packages/drivers/redis`. Host only consumes generic contracts.
2. **Small, reviewable PRs**: Prefer ≤ ~800 LOC modules; split by functional unit (already practiced in PR-1).
3. **Safety by default**: SCAN only, danger classification, readonly gates, strong confirm for destructive ops.
4. **Test as you go**: Unit (ops / parse / invokes) + e2e (RD-xxx) for every new surface.
5. **i18n from day one**: en + zh-CN keys with every UI string.
6. **Do not merge incomplete vertical slices** to main until the slice is reviewable and tested.

## 1. Completed Work

### PR-1 — KEEPTTL / EXPIREAT / Decompress (done, ready for review)

**Branch**: `feat/redis-pr1-keepttl-expireat-decompress`  
**PR**: https://github.com/flyxl/datazen/pull/30

| Area | Deliverable |
|------|-------------|
| ops | `TtlCommand::ExpireAt`, `resolve_expire_at`, `set_expire_at`, `set_string_with_options(KEEPTTL)` + 15+ unit tests |
| commands | schema `keepTtl` / `expireAt`; mutate arms |
| driver | `plugin_set_string(..., keep_ttl)`, `plugin_set_expire_at`; `redis_driver_db` / `kv` split |
| UI | String editor: keepTtl checkbox, expireAt datetime-local, gzip/zlib/deflate decompress + pretty JSON |
| editors | Hash / List / Set / Zset split into dedicated components |
| invokes | `invokeSetString(keepTtl)`, `invokeSetExpireAt`, `invokeCreateKey` |
| locales | en + zh-CN |
| tests | stringKeyValue.test.ts, keyEditorsInvokes.test.ts, e2e RD-025 / RD-026 |

**Next action on PR-1**: Review → merge when approved. Do not block Phase 1.2+ on this merge if parallel work is clean.

## 2. Phase 1 Remaining Work (highest priority)

### PR-2 — Incremental SCAN Key Browser

**Goal**: Replace any KEYS* paths; add tree/flat, cursor pagination, type filter, summary columns.

**Tasks**:
1. Backend
   - `redis.scan_keys` command (cursor, MATCH, COUNT, TYPE).
   - Return: key, type, ttl, memory estimate (MEMORY USAGE when available), value summary.
   - Cancel / interrupt support.
2. UI
   - Key browser component: tree (namespace) ↔ flat toggle.
   - “Load more” + progress; interruptible full-scan mode.
   - Columns + type icons + TTL badge.
3. Tests
   - Unit: cursor continuation, MATCH, TYPE filter.
   - e2e: RD-030 SCAN pagination, RD-031 tree vs flat.

**Estimate**: 1–1.5 weeks (1 engineer).

### PR-3 — Collection Editors Depth (Hash / List / Set / ZSet)

**Goal**: Pagination + search + full CRUD for each collection type.

**Tasks**:
1. Backend
   - HSCAN / SSCAN / ZSCAN wrappers with cursor + COUNT + MATCH.
   - Field/member get/set/delete commands already partially present; harden and document.
2. UI
   - Enhance existing `HashEditor` / `ListEditor` / `SetEditor` / `ZsetEditor`.
   - Search box, sort, page size, inline edit.
3. Tests
   - e2e: RD-040..RD-043 per type (page + search + mutate).

**Estimate**: 1–2 weeks.

### PR-4 — Stream Viewer + Consumer Groups

**Goal**: XRANGE + XINFO GROUPS/CONSUMERS + pending + lag.

**Tasks**:
1. Backend
   - `redis.stream_info`, `redis.stream_groups`, `redis.stream_pending`, `redis.xrange`.
2. UI
   - Stream detail panel: entries table, groups table (lag, pending, consumers).
3. Tests
   - e2e RD-050 Stream groups / pending.

**Estimate**: 1–1.5 weeks.

### PR-5 — TTL Controls on All Types + Persist

**Goal**: Relative / absolute / KEEPTTL / persist available from key detail for every type.

**Tasks**:
- Extend existing TTL UI (already in string) to collection + stream key headers.
- `PERSIST` command + batch later (Phase 3).
- Tests: RD-026 already covers EXPIREAT; add persist path.

**Estimate**: 3–5 days (can be folded into PR-3/4).

## 3. Phase 2 — Console / Monitor / Pub/Sub / Observation

| PR | Scope | Notes |
|----|-------|-------|
| PR-6 | Console structured results + danger classification | Map to Host security tokens |
| PR-7 | Real-time MONITOR (dedicated conn, last-N, stop) | Stream push to UI |
| PR-8 | Pub/Sub panel (sub / psub / publish / clear) | Reuse existing skeleton |
| PR-9 | INFO sections + Slowlog + MEMORY USAGE widgets | Dashboard contribution |

**Estimate**: 3–4 weeks total.

## 4. Phase 3 — Safety & Batch

| PR | Scope |
|----|-------|
| PR-10 | Readonly mode enforcement + production flag |
| PR-11 | Multi-select delete + batch TTL + FLUSH* strong confirm |
| PR-12 | MCP permission split (normal write vs ultra-danger) |
| PR-13 | DB display alias |

**Estimate**: 2–3 weeks.

## 5. Phase 4 — Differentiation

| Item | Description |
|------|-------------|
| AI NL → Redis | Prompt templates + tool calling for SCAN / XINFO |
| Workflow steps | SCAN pattern, lag check, expire cleanup, cross-DB |
| MCP safe surface | Read-only key browser + Stream lag for agents |
| Ops alerts | Threshold widgets on INFO / lag |

**Estimate**: ongoing after Phase 2–3; can start prototypes earlier.

## 6. Sequencing & Parallelism

```
PR-1 (done) ──► merge
                 │
                 ▼
            PR-2 (SCAN browser)  ── parallel with ──  PR-3 (collection editors)
                 │                                        │
                 └────────────┬───────────────────────────┘
                              ▼
                         PR-4 (Stream)
                              │
                         PR-5 (TTL polish)
                              │
                    Phase 2 (Console / Monitor / PubSub)
                              │
                    Phase 3 (Safety / Batch)
                              │
                    Phase 4 (AI / Workflow / MCP)
```

- Prefer sequential vertical slices when they touch the same UI shell.
- Backend command work can run parallel to UI if contracts are agreed first.

## 7. Engineering Checklist (every PR)

- [ ] Commands registered only inside Redis driver.
- [ ] Schema + i18n keys added.
- [ ] Unit tests for pure logic (ops / parse / invoke helpers).
- [ ] e2e case(s) with stable RD-xxx id.
- [ ] Module size kept reasonable; split if growing.
- [ ] Danger level annotated for any mutating / destructive command.
- [ ] No `KEYS *` introduced.
- [ ] README / CHANGELOG note if user-visible.

## 8. Immediate Next Actions (this week)

1. **Land PR-1** — review + merge `feat/redis-pr1-keepttl-expireat-decompress`.
2. **Start PR-2** — design `redis.scan_keys` contract + Key browser UI skeleton.
3. Inventory existing SCAN / key-list code paths; remove any KEYS fallback.
4. Open tracking issues or project board columns for PR-2..PR-5.

## 9. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Large keyspace freezes UI | Cursor + cancel; never load all at once |
| MONITOR / PubSub connection leaks | Dedicated connection lifecycle; explicit stop |
| Danger command regression | Central danger map + e2e that asserts block/confirm |
| Scope creep into Host | Code review gate: “is this Redis-specific?” |
| Module bloat | Continue functional split pattern from PR-1 |

## 10. Definition of Done (overall Phase 1)

- SCAN-only browser with tree/flat and pagination.
- All major types have usable editors (paged + search + CRUD).
- Stream groups / pending visible.
- TTL controls consistent across types.
- PR-1 merged; subsequent PRs reviewed and on main.
- e2e suite covers the new paths; no critical open bugs on key path.
