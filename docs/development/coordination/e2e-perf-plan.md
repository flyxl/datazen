# E2E 性能优化实施计划

> 目标：将全量 E2E 运行时间从 ~75 分钟降至 <40 分钟（墙钟），开发迭代 smoke 降至 <10 分钟

## 现状基线

| 指标 | 数值 |
|------|------|
| spec 文件总数 | 93 + 8 journeys |
| 全量运行时间 | ~75 min |
| spec 实际执行 | ~55 min |
| session 开销 | ~20 min（beforeSuite 重置 × 100 次） |
| browser.pause 总等待 | ~11.4 min（810 次，≥1s 有 298 次 = 7.5 min） |
| maxInstances | 1（纯串行） |

## 轨道划分（按文件冲突面）

### Track A: `e2e-config`（P0-exclude + P1-beforeSuite + P2-smoke）
- **触碰文件**：`e2e/wdio.conf.ts`、`package.json`
- **内容**：
  1. 从默认 glob 排除 `zz-screenshots.ts`、`demo-recording.ts`
  2. 精简 `beforeSuite` 钩子（条件执行子窗口关闭逻辑）
  3. 新建 `smoke` suite（30 个核心 spec，目标 <10 min）
  4. 添加 `pnpm e2e:smoke` 脚本到 `package.json`
  5. 添加 `specFileRetries: 1`（自动重试偶发失败）
  6. 添加 `bail` 策略（单 describe 内首失败跳过后续）

### Track B: `e2e-pause`（P0-pause 消除）
- **触碰文件**：`e2e/specs/*.ts`（内容级别修改，不改文件结构）、`e2e/helpers.ts`
- **内容**：
  1. 替换所有 `browser.pause(≥1000)` 为条件等待 `waitUntil` / `waitForDisplayed` / `waitForExist`
  2. 保留必要的短 pause（<500ms 的 UI settle 延迟保留为 200-300ms）
  3. 不改变文件结构，不删除/合并文件
- **优先处理热点**：`zz-screenshots.ts`(77次) → `helpers.ts`(54次) → `navigator-context-menu.ts`(38次)

### Track C: `e2e-merge`（P1-merge 小 spec）
- **触碰文件**：`e2e/specs/data-dashboard-*.ts`（6个→1个）、`e2e/specs/chart-*.ts`（2个→1个）
- **内容**：
  1. 合并 6 个 dashboard spec 为 `data-dashboard.ts`
  2. 合并 2 个 chart spec 为 `chart.ts`
  3. 更新 `wdio.conf.ts` suites 引用
- **与 Track A 冲突**：都改 `wdio.conf.ts`，**必须在 Track A 之后串行**

### Track D: `e2e-ci-parallel`（P2-CI 分层）
- **触碰文件**：CI 配置（如有）、文档
- **内容**：
  1. 设计 3-job 并行 CI 分层方案
  2. 更新 `e2e/run.mjs` 支持 `--port` 参数
  3. 更新 e2e-testing.md 文档

### Track E: `e2e-ipc-migrate`（P3-IPC 迁移 · 长期）
- **触碰文件**：新建 Rust 测试文件、可能删除/标记部分 spec
- **内容**：将纯 IPC 断言类 spec 迁移为 Rust integration test
- **风险高，独立评估**

### Track F: `e2e-multi-instance`（P3-多实例并行 · 长期）
- **触碰文件**：`e2e/run.mjs`、`e2e/wdio.conf.ts`
- **内容**：支持启动多个 DataZen 实例 + 多 WDIO capabilities
- **风险高，独立评估**

## 波次编排

```
Wave 1（并行）: Track A (config) ∥ Track B (pause)
Wave 2（串行）: Track C (merge)        ← 依赖 Track A 完成
Wave 3（并行）: Track D (ci-parallel) ∥ Track E (ipc-migrate)
Wave 4（串行）: Track F (multi-instance) ← 依赖 Track D
R 阶段：全量 E2E 回归验证
```

## 预期收益

| 优化项 | 预计收益 |
|--------|----------|
| P0 排除 screenshots/demo | -5~6 min |
| P0 pause 消除 | -3~5 min |
| P1 合并小 spec | -3~4 min |
| P1 beforeSuite 精简 | -5~8 min |
| P2 smoke suite | 开发迭代 <10 min |
| P2 CI 并行 | 墙钟 -30~40 min |
| **P0+P1 总计** | **~55 min（从 75 min）** |
| **+P2 CI 并行** | **墙钟 ~25 min** |
