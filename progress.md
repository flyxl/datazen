# p5-domain-extract 进度台账（P5 Wave-0，随分支流转，合入后删除）

基点：`codex/p5-integration` @ 6db45f0a8。轨道分支：`feature/p5-domain-extract`。

## 验收标准对照

1. 三件套 crate 抽取：已完成。`packages/schema-diff/`、`packages/data-sync/`、`packages/data-transfer/`（`transfer/` 并入 data-transfer 的 `src/transfer/`）。
2. 宿主只剩 Tauri commands/窗口/文件选择/原生工具环境：commands 仍在 `src-tauri/src/commands/{schema_diff.rs,sync/,data_transfer/}`；`src-tauri/src/lib.rs` 的 `mod` 声明改为 re-export shim（`pub use datazen_data_sync as data_sync;` 等），路径外观不变。
3. 领域包不引用 Tauri/HTTP/窗口 Store/driver 实现库：`cargo tree --edges normal` 闭包仅含 driver-api 与三件套自身；driver 具体类型（PostgresTypeNormalizer、PgSyncAdapter 等）仅出现在 `#[cfg(test)]` + dev-dependency。
4. 测试落点：crate 内单测随迁移；`cargo test -p datazen-schema-diff` 218 passed EXIT=0；`-p datazen-data-sync` 176 passed EXIT=0；`-p datazen-data-transfer` 183 passed EXIT=0；`cargo test -p datazen --lib` 1710 passed EXIT=0；`pnpm --config.verify-deps-before-run=false typecheck` EXIT=0。
5. 行为不变：commands 层仅 import/路径改写（`crate::data_sync::` → 经 shim 不变），IPC 外观与参数未动；见 commands diff 摘要（0 行为变化，仅 `crate::transfer::`/`crate::data_sync::` 解析目标更换）。
6. workspace / CI 登记：根 `Cargo.toml` members + `[workspace.dependencies]` 三条；`scripts/lib/cargoWorkspace.mjs` LAYERS 增 3 条；`scripts/run-platform-crate-tests.mjs` TESTED_LAYERS 增 3 条；`--dry-run` EXIT=0 发现 6 个 crate；`scripts/check-platform-crate-boundaries.mjs` EXIT=0（25 members classified, 0 violations）。

## 已做的关键决策（机械迁移，行为不变）

- `crate::db::X` → `datazen_driver_api::X`（host `crate::db` 只是 driver-api 的 re-export）。
- `crate::schema_objects::X` → `datazen_driver_api::schema_objects::X`。
- `crate::services::query_executor::{FilterCondition,FilterOperator}` 迁入 `datazen-driver-api::filters`（serde 形状不变）；host `query_executor` 与 `services` 保留 re-export。
- `services/transaction.rs`（TransactionScope）整体迁入 `packages/schema-diff/src/transaction.rs`；host 删除该模块。
- `data_transfer::recordset_bounds` 迁入 data-sync（`crate::recordset_bounds`），错误从 `TransferError` 改为 `String`，调用点用 `.map_err(TransferError::validation)` 包回（Display 文本不变）。
- `transfer/pairing.rs`（SyncPairing 策略）迁入 data-sync 为 `sync_pairing` 模块；`datazen_data_transfer::transfer::pairing` 通过 `pub use` 再导出保持旧路径可寻址。
- `testing/mock_driver.rs` → `packages/driver-api/src/mock_driver.rs`；host `testing` 模块 re-export。
- 三个新 crate `#[cfg(test)] extern crate datazen_driver_{mysql,postgres,sqlite}` 强链，使 `create_driver`/sync 分类学（inventory）在 crate 自有测试二进制里生效；driver crates + tempfile 等列为 dev-dependencies。
- host `webdriver` feature 透传 `datazen-data-transfer/webdriver`，保证 WDIO 构建下的 fault-seam cfg 路径可编译（`cargo check -p datazen --features webdriver --lib` EXIT=0）。

## 提交

- `50229935e` refactor(p5): extract schema-diff/data-sync/data-transfer into packages/* crates
- `290479d7c` fix(p5): make extracted crates' tests self-contained
- （本台账与 CI 登记提交：TODO）

## 遗留

- `FilterCondition`/`FilterOperator` 从 host services 迁入 driver-api 是类型身份迁移，但 IPC JSON 形状完全一致。
- `recordset_bounds` 错误文本路径：data-transfer 侧显示文本不变；data-sync 侧 `DataSyncError::validation(string)` 文本不变。
- WDIO E2E 未在本轨重跑（超出 Wave-0 纯机械范围）；host `webdriver` feature 编译已验证。
