# Track: e2e-perf 进度

## 功能摘要

| 编号 | 范围 | 状态 | 编码 commit | 测试 commit |
|------|------|------|-------------|-------------|
| A | wdio config (exclude + smoke + beforeSuite) | ✅ 已完成 | `ba9d4b24` | tsc pass |
| B | pause 消除（80 个 spec 文件） | ✅ 已完成 | `0ea78ba2` `746ea178` `4c7a50e1` | tsc pass |
| C | 合并小 spec (dashboard 6→1, chart 2→1) | ✅ 已完成 | `0243ed52` | tsc pass |
| D | CI 并行分层 (--port + E2E_WD_PORT) | ✅ 已完成 | `cba8bacd` | tsc pass |
| E | IPC spec → Rust 迁移 (PoC) | ✅ 已完成 | `25f53823` | cargo test pass |
| F | 多 WebDriver 实例 (--instances N) | ✅ 已完成 | `c776411e` | tsc pass |

## 合并记录

| 步骤 | 操作 | 结果 |
|------|------|------|
| Wave 1 | merge feature/e2e-config → feat/e2e-perf-optimization | Fast-forward |
| Wave 1 | merge feature/e2e-pause → feat/e2e-perf-optimization | 自动合并 wdio.conf.ts 无冲突 |
| Wave 2 | merge feature/e2e-merge → feat/e2e-perf-optimization | 自动合并 wdio.conf.ts 无冲突 |
| Wave 2 | merge cba8bacd (Track D, 主检出) | 已在集成分支 |
| Wave 3 | merge feature/e2e-ipc → feat/e2e-perf-optimization | Fast-forward |
| Wave 3 | merge feature/e2e-multi → feat/e2e-perf-optimization | 自动合并 无冲突 |

## 验证结果

- `npx tsc --noEmit`: ✅ 通过
- `cargo check -p datazen`: ✅ 通过（3 个 warning 均为既有）
- 全量 E2E 回归: 待执行

## 设计决策 / 遗留注意

1. `zz-screenshots.ts`、`demo-recording.ts`、`zz-diag.ts` 移出默认 glob 但保留在独立 suite
2. 合并 spec 时保留所有 test case，不删除测试覆盖
3. pause 消除采用渐进式：所有 ≥1000ms 已处理，保留 <500ms
4. P3 IPC 迁移仅完成 PoC（driver-commands），其余 7 个候选见 `e2e-ipc-migration-guide.md`
5. P3 多实例并行需在有 webdriver build 的环境中验证
6. `specFileRetries: 1` 已添加，自动重试偶发失败
