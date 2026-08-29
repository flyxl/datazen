# cr-p2-sync-pairing — 进度

**轨 ID：** cr-p2-sync-pairing | **状态：** 已完成

## 交付

1. **`classify_data_sync_pair` IPC** — `commands/sync/mod.rs` + `lib.rs` 注册
2. **前端单一来源** — `syncPairing.ts` 改为 IPC 薄封装（含 cache + `useSyncPairingState` hook）；`DataSyncWindow` 异步加载 pairing
3. **测试** — 单测 mock IPC；E2E `SYNC-REAL-020` / `SYNC-BATCH-002` 断言新 IPC；Rust 守护测试确认 `classify_sync_pair` 仍移除、`classify_data_sync_pair` 已注册
4. **文档** — `data-sync.md`、`commands.md` 更新

## 验证

- `npx vitest run src/lib/__tests__/syncPairing.test.ts src/windows/data-sync/__tests__/DataSyncWindow.test.tsx`
- `CARGO_TARGET_DIR=.../target cargo test -p datazen --lib sync::tests`
