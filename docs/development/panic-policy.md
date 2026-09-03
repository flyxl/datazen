# Production panic policy (Host Rust)

> Applies to `src-tauri/src/**` production code paths (IPC handlers, services, store, MCP).

## Rules

1. **No bare `unwrap()` / `expect()` on production paths.** Propagate errors via `Result` and the existing error types (`CommandError`, `ConnectionError`, `StoreError`, etc.).
2. **Tests are exempt.** `#[cfg(test)]` modules, `#[test]` functions, and test-only helpers may use `unwrap()` / `expect()` for brevity.
3. **Intentional panic must be documented.** If a path truly cannot fail (e.g. a compile-time invariant) or recovery is impossible, use `expect("…")` with a comment explaining why panic is acceptable.
4. **Poisoned locks are not fatal by default.** Prefer `lock().map_err(...)` and return an internal/store error. In `Fn` callbacks that cannot return `Result`, recover with `into_inner()` after logging — see `commands/export.rs`.

## Preferred patterns

```rust
// IPC / service — propagate
let guard = self.connect_locks
    .lock()
    .map_err(|e| ConnectionError::Internal(format!("connect lock poisoned: {e}")))?;

// Fallible operation — use ? or map_err
let value = serde_json::from_str(&raw).map_err(CommandError::from)?;

// Precondition already checked above — avoid expect; use ok_or_else
let app = dialog.ok_or_else(|| {
    CommandError::Internal("save dialog AppHandle missing after preflight".into())
})?;
```

## Scope for review

Priority areas when touching Host Rust:

- `services/connection_manager.rs` — session lifecycle
- `store/**` — encryption and persistence (most modules already use `with_conn` + `map_err`)
- `commands/**` — Tauri IPC entry points, especially streaming/export paths

## PR checklist

See [CONTRIBUTING.md](../../CONTRIBUTING.md) — confirm production paths do not add bare `unwrap()` / `expect()` without justification.
