# Track: e2e-perf 进度

## 功能摘要

| 编号 | 范围 | 状态 | 编码 commit | 测试 commit |
|------|------|------|-------------|-------------|
| A | wdio config (exclude + smoke + beforeSuite) | 未开始 | — | — |
| B | pause 消除（热点 spec 文件） | 未开始 | — | — |
| C | 合并小 spec (dashboard/chart) | 未开始 | — | — |
| D | CI 并行分层 | 未开始 | — | — |
| E | IPC spec → Rust 迁移 | 未开始 | — | — |
| F | 多 WebDriver 实例 | 未开始 | — | — |

## E2E 用例表

本轨优化 E2E 基础设施本身，验收标准为：
- 全量 E2E 通过率不低于优化前
- 运行时间下降 ≥20 min

## 测试结果与覆盖率

（待填充）

## 设计决策 / 遗留注意

1. `zz-screenshots.ts` 和 `demo-recording.ts` 移出默认 glob 但保留在独立 suite
2. 合并 spec 时保留所有 test case，不删除测试覆盖
3. pause 消除采用渐进式：先处理 ≥1000ms，保留 <500ms
