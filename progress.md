# Issue 69 — READY_FOR_TEST

## 已完成

- 核对 `driver-api/src/factory.rs::create_driver`、`DatabaseDriver::{migration_renderer,migration_capabilities,type_normalizer}` 和 MySQL/PostgreSQL/SQLite 实现。生产计划路径原本已走 inventory registry，无契约缺口；issue 所称生产编译期绑死不成立，实际问题为三个具体驱动 dev-dependencies。
- 删除 schema-diff 的具体驱动 dev-dependencies 和仅测试 force-link；driver 只通过 dev-dependency 引用 schema-diff，无生产反向边、无 driver 互相依赖。
- 原 plan_tests 80 测试：保留纯逻辑与缺失注册回归 2 个；78 个迁到对应目标 driver，4 个双方言循环和 2 个双目标用例拆为 84 个独立测试，保留各方言断言；3 个真实 normalizer 测试迁到对应 driver。新增文件最大 602 行，fixture 在各驱动 tests/schema_diff_support 中共享。
- 增加 Cargo metadata 回归，覆盖 normal/build/dev、重命名、optional 和 target-specific 声明；更新正式边界文档为已实现事实。

## 实测

- `cargo test -p datazen-schema-diff --lib --test job_handler`：exit 0；`test result: ok. 148 passed; 0 failed` + `test result: ok. 10 passed; 0 failed`。首次运行自动更新预期 Cargo.lock 依赖边，之后全部使用 --locked。
- 独立运行 `cargo test --locked -p datazen-driver-{mysql,postgres,sqlite} --test 'schema_diff_*'`：三个退出码均 0；mysql 26、postgres 59、sqlite 2，共 87 通过，无失败。首尾 HEAD d122d2a37e246067a163e2cc5fbd5403c02bfee7，工作区 SHA eb9c6d26ce9cbbfc53485cebb06518b0fcb0fe324ce8a6fa12078a3bf1b1cc89 均一致。
- `vitest run scripts/__tests__/schema-diff-driver-boundary.test.ts`：exit 0，1 file / 1 test passed。
- `pnpm --config.verify-deps-before-run=false typecheck`：exit 0，含 src/packages、scripts、pack-ep 类型检查。
- vitest/typecheck 首尾 HEAD d122d2a37e246067a163e2cc5fbd5403c02bfee7，工作区 SHA a326444371fa19da7cecd9fcf0a339ae1660e988a599bba514b90786a92c5468 均一致（后续仅整理 Rust test imports 与台账）。
- 完整日志留系统临时目录 /tmp/datazen-issue69-*.log；独立 Rust target /tmp/datazen-issue69-target。

## 遗留

无功能遗留。集成验收后删除本 progress.md；未改外部 issue 状态，未发送外部消息。
