# Task 7 Report — Cluster / Sentinel / mTLS connect paths

**Branch:** `feat/redis-deep-ops-e2-e4`  
**Date:** 2026-08-09

## Summary

Implemented `build_connection_plan()` and wired `RedisDriver::connect` / `test_connection` to open standalone, Redis Cluster, or Sentinel-backed master connections with optional TLS/mTLS material from `ConnectionConfig.options`.

## What works

| Area | Status |
|------|--------|
| **Standalone** | Default when `options.topology` absent; same host/port/auth/DB URL behavior as E1/E2 (backward compatible). |
| **Cluster** | `options.topology = "cluster"` + `clusterNodes[]`; uses `redis` crate `ClusterClient` / async cluster connection; ops route through generic `AsyncCommands` dispatch. |
| **Sentinel** | `topology = "sentinel"` + `sentinelMasterName` + `sentinelNodes[]`; resolves master via `SentinelClient`; stores client for **one-shot rediscover** on connection-loss errors in `with_live_op` / `with_live_any_op`. |
| **TLS plan parsing** | `options.tls.enabled`, `caPath`, `certPath`, `keyPath`, `keyPassphrase`, `insecureSkipVerify`; also enabled when `sslMode` is Prefer/Require/VerifyCa/VerifyFull. URLs use `rediss://` when TLS is on. |
| **mTLS file plumbing** | PEM paths read at connect time; passed to `Client::build_with_tls` (standalone) and `ClusterClientBuilder::certs` (cluster). |
| **Unit tests** | `connect.rs`: default standalone, cluster nodes, TLS flag, sslMode, sentinel validation (no network). All `datazen-driver-redis` tests pass (19). |

## Cargo / features

```toml
redis = { version = "0.27", features = [
  "tokio-comp", "aio", "cluster-async", "sentinel",
  "tls-rustls", "tls-rustls-insecure", "tokio-rustls-comp"
] }
```

## Options keys (locked with Task 6 / frontend tests)

- `topology`: `standalone` | `cluster` | `sentinel`
- `clusterNodes`: `string[]` (`host:port`)
- `sentinelMasterName`, `sentinelNodes`, `sentinelNodePassword` (optional, for sentinel URL auth)
- `tls`: `{ enabled, caPath, certPath, keyPath, keyPassphrase, insecureSkipVerify }`

## Deferred / limitations (honest)

1. **No connection wizard UI** — Task 8; users must set `options` via JSON/edit until wizard lands.
2. **Sentinel failover UX** — Rediscover re-queries Sentinel and replaces the master connection on IO-style errors; **no toast** when master changes (host notification deferred to Task 8 / shell).
3. **Sentinel TLS on sentinel nodes vs master** — Sentinel URLs and master use the same TLS mode/plan; separate sentinel-only TLS is not modeled beyond `sentinelNodePassword` in sentinel URLs.
4. **Cluster logical DB** — Redis Cluster effectively uses DB 0; `SELECT` on cluster may fail for non-zero DB (pre-existing Redis Cluster constraint).
5. **SSH tunnel + TLS composition** — Not integrated in this task (host tunnel path unchanged; full compose untested).
6. **`clusterRouting: pinnedNode`** — Plugin setting and node picker are Task 8; cluster ops always use redis-rs auto routing (`MOVED`/`ASK`).
7. **TLS without rustls feature** — Build always enables `tls-rustls`; no native-tls fallback.
8. **Encrypted key passphrases** — `keyPassphrase` is parsed in plan but **not** applied to rustls private key loading (redis crate PEM loader only; passphrase support would need extra work).
9. **Live integration tests** — Cluster/Sentinel/TLS end-to-end require real endpoints (Task 9 env hooks); not run in CI by default.

## Files touched

- `packages/drivers/redis/src/connect.rs` (new)
- `packages/drivers/redis/src/redis_driver.rs`
- `packages/drivers/redis/src/ops.rs`, `ops_exec.rs`, `ops_observe.rs` (generic connection bounds)
- `packages/drivers/redis/Cargo.toml`
- `packages/drivers/redis/src/lib.rs`
