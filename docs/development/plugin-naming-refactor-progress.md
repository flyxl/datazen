# Plugin Naming Refactor Progress

## Track A: Compile-time driver `plugin-` → `driver-` rename

**Status:** ✅ COMPLETED  
**Commit:** `9b0395e6`  
**Branch:** `feature/rename-driver`  

### Changes Summary (29 files)

| Category | Files | Key Changes |
|----------|-------|-------------|
| Config | `drivers-registry.json`, `Cargo.toml` (root), `src-tauri/Cargo.toml` | All `plugin-*` feature values and marker blocks → `driver-*` |
| Rust | `driver_init.rs` (renamed from `plugin_init.rs`), `lib.rs`, `redis_flush_gate.rs`, `transfer/adapter_registry.rs` | File rename, `#[cfg(feature)]`, `mod` declaration, function call |
| Scripts | `resolve-drivers.mjs`, `plugin-deinject.mjs`, `plugin-stash-precommit.mjs`, `ensure-generated-drivers.mjs`, `check-managed-stubs.mjs`, `run-e2e-minimal.sh`, `new-feature-worktree.sh`, `ci-tauri-build.mjs`, `e2e-tauri-build.mjs`, `tauri-dev.mjs`, `plugin-file-stash.mjs` | Marker names, file paths, variable names, output paths |
| Gitignore | `.gitignore` | `plugin_init.rs` → `driver_init.rs`, `.plugin-features.json` → `.driver-features.json`, `.plugin-file-stash/` → `.driver-file-stash/` |
| Artifacts | `.plugin-features.json` → `.driver-features.json` | File renamed |
| Tests | `fixture.ts`, `plugin-stash-precommit.test.ts`, `plugin-deinject.test.ts`, `ci-tauri-build.test.ts` | Updated fixture data, import names, assertions |
| Docs | `AGENTS.md`, `CONTRIBUTING.md`, `e2e-testing.md`, `independent-plugin-development.en.md`, `independent-plugin-development.zh-CN.md` | File references, feature examples |
| CI/CD | `.github/workflows/ci.yml` | `.plugin-features.json` → `.driver-features.json` |
| Driver | `packages/drivers/redis/src/ops.rs` | Comment update |

### Verification Results

```
✅ No plugin- driver features in src-tauri/Cargo.toml (tauri-plugin-* are Tauri packages)
✅ No plugin_init references in src-tauri/src/ or scripts/
✅ No cfg(feature = "plugin-*") in src-tauri/src/
✅ All gitignored files renamed correctly
```

### Not Changed (by design)
- `plugin-file-stash.mjs` script name and `.plugin-file-stash/` directory (stash mechanism, not driver naming)
- `src-tauri/src/plugins/` directory (Track B scope)
- Frontend files like `src/commands/plugins.ts` (Track B scope)
- External Git driver crate names (`datazen-plugin-kiwi`, etc.)
