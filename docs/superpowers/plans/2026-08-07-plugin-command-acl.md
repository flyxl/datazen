# Plugin Command ACL Implementation Plan

> **Status:** Implemented (pending kiwi publish + manual UI verify)

**Goal:** Fix `plugin:kiwi|login not allowed by ACL` using Tauri native plugin permissions (方案 A).

## Done

- [x] Kiwi: `permissions/`, `build.rs`, `commands.rs`, `init()`, feature `tauri-plugin`, `links = "kiwi"`
- [x] Host: `tauriPlugin` in registry; `resolve-plugins` injects dep features + `kiwi:default` ACL
- [x] `tauri-dev.mjs` restores capabilities on exit
- [x] `cargo check --features plugin-kiwi` passed; schema has `kiwi:default` / `kiwi:allow-login`

## Remaining

- [ ] Publish / push kiwi plugin repo changes (`datazen-driver-kiwi`)
- [ ] Manual: `pnpm tauri:dev --plugins=kiwi` → login no ACL error
- [ ] Commit host branch `fix/plugin-command-acl` when requested
