# Track: onboarding-rust — Rust 基建

## Phase: CODING

### 目标
Phase 0 Rust 侧基建 + Phase 3 sample seed 命令。

### 范围
1. `src-tauri/src/store/settings.rs` — AppSettings 加 `onboarding?: { completed: bool, version: i32 }`（serde default）
2. `src-tauri/src/commands/sample.rs`（新）— `seed_sample_db` 命令：生成 `{appData}/sample/playground.db`，含 `demo_sales(region TEXT, amount REAL, quarter TEXT)` 8 行全英文数据
3. `e2e/specs/welcome.ts` — before/after 加 `onboardingCompleted=true` bypass
4. `src-tauri/src/commands/mod.rs` — 注册 sample 模块

### 验收标准
- `cargo test -p datazen --lib` 全通过
- settings roundtrip 单测通过
- `seed_sample_db` 命令可生成 playground.db
- welcome E2E 不受 onboarding gate 影响

### 自验套件
- [ ] cargo test -p datazen --lib
