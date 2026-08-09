# Task 12 Report — Stream detail + Monitor overview

**Branch:** `feat/redis-deep-ops-e2-e4`  
**Spec:** §7.3 Stream  
**Commit message:** `feat(redis): Stream groups UI and overview components`

## Summary

Implemented Redis Stream backend helpers, plugin commands, a full consumer-groups editor, and a Monitor overview subcomponent. Wiring into `KeyEditors.tsx` and `MonitorPanel.tsx` is deferred per file partition.

## Backend

| Command | Args | Returns |
|---------|------|---------|
| `xrange` | `connectionId`, `dbIndex`, `key`, `start`, `end`, `count?` | `{ entries: [{ id, fields }] }` |
| `xadd` | `connectionId`, `dbIndex`, `key`, `fields`, `id?` | `{ id }` |
| `xgroup_create` | `connectionId`, `dbIndex`, `key`, `group`, `startId?` | `()` |
| `xgroup_destroy` | `connectionId`, `dbIndex`, `key`, `group` | `()` |
| `xinfo_groups` | `connectionId`, `dbIndex`, `key` | `StreamGroupInfo[]` |
| `xpending` | `connectionId`, `dbIndex`, `key`, `group`, `start?`, `end?`, `count?`, `consumer?` | `{ total, entries }` |
| `xack` | `connectionId`, `dbIndex`, `key`, `group`, `ids[]` | `number` |
| `stream_overview` | `connectionId`, `dbIndex`, `limit?` | `{ rows, truncated }` |

- **`ops_stream.rs`**: XRANGE/XADD, XGROUP CREATE/DESTROY, XINFO GROUPS, XPENDING, XACK, SCAN-based stream overview sampling (default limit 100).
- **`validate_xgroup_name`**: rejects empty/whitespace-only group names; trims accepted names.
- **`redis_driver.rs` / `plugin.rs`**: wired via `plugin_on_db!` and explicit helpers for HashMap/optional args.
- **Permissions**: `allow-xrange`, `allow-xadd`, `allow-xgroup-create`, `allow-xgroup-destroy`, `allow-xinfo-groups`, `allow-xpending`, `allow-xack`, `allow-stream-overview`.

## Frontend

- **`StreamEditor.tsx`**: props `{ connectionId, dbIndex, key }`. Entries tab (XRANGE table + XADD form); Groups tab (XINFO GROUPS table, create/destroy group, pending list with multi-select XACK).
- **`StreamOverview.tsx`**: standalone Monitor sub-page component calling `stream_overview`; table of key / length / group count / pending total.

## i18n

26 new `redis.stream*` keys in all 10 locale files (en/zh-CN/zh-TW translated; Beta locales mirror English).

## Tests

```
cargo test -p datazen-driver-redis --features tauri-plugin --lib ops_stream
```

Unit tests (no live Redis):

- `validate_xgroup_name_rejects_empty` / `validate_xgroup_name_trims_and_accepts`
- `resolve_stream_overview_limit_defaults`
- `parse_stream_entry_basic`
- `parse_xinfo_groups_from_array_pairs`
- `parse_xpending_entries_from_detail_rows`

## Out of scope (partition)

- `KeyEditors.tsx` stream wiring (replace read-only JSON with `StreamEditor`)
- `MonitorPanel.tsx` Stream overview tab import
- E2E stream smoke

## Manual verification checklist

- [ ] Open stream key detail → `StreamEditor` shows entries and groups tabs
- [ ] XADD new field/value → entry appears after refresh
- [ ] XGROUP CREATE → group listed; DESTROY removes it
- [ ] XPENDING entries selectable → XACK clears them
- [ ] `StreamOverview` in Monitor lists stream keys with length/group/pending counts
