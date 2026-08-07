# Plugin Command ACL (方案 A) Design

> **Status:** Approved  
> **Date:** 2026-08-07

## Problem

Frontend calls `pluginInvoke('kiwi', 'login')` → IPC `plugin:kiwi|login`. Tauri v2 denies plugin commands unless ACL grants `kiwi:allow-*` / `kiwi:default`. Host `capabilities/default.json` had no kiwi permissions.

Superset has no `pluginInvoke` / extension commands; login is inside the driver. Out of scope for command ACL.

## Approach (Tauri native permissions)

1. **Plugin repo** declares Tauri plugin commands + `permissions/` (official layout). `build.rs` uses `tauri_plugin::Builder` to autogenerate allow/deny. `permissions/default.toml` exposes `kiwi:default`.
2. **Plugin** exports `init()` registering commands via `Builder::new("kiwi")`.
3. **Host** `resolve-plugins.mjs` reads `tauriPlugin` from `plugins-registry.json`, generates `plugin_init.rs`, injects `features = ["tauri-plugin"]` on the dep, and adds `{id}:default` into `capabilities/default.json`.
4. Frontend keeps `pluginInvoke` / `plugin:{id}|{command}` unchanged.

## Non-goals

- Superset/OLAP command migration
- Returning login to host `generate_handler!`
- Runtime dynamic plugins
