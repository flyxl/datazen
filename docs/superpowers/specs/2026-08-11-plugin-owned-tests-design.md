# 插件自有测试（Host 默认不拉）设计

> 日期：2026-08-11  
> 状态：已批准  
> 分支：`refactor/plugin-owned-tests`  
> 实现计划：[../plans/2026-08-11-plugin-owned-tests.md](../plans/2026-08-11-plugin-owned-tests.md)  
> 决策：方案 **C** — 插件相关测试在插件包 / 插件仓执行；**Host 默认 CI 与默认命令永不拉**这些测试。  
> 补充锁定：Redis **单元测试也不进入** Host `pnpm test:unit`。

## 1. 问题

Host 套件（Vitest / `cargo test -p datazen` / WebdriverIO E2E）混入了：

- **Git 插件 E2E**（`e2e/specs/kiwi.ts`）— `DATAZEN_DRIVERS=basic` 时必然失败（无 Kiwi UI）。
- **Redis 深度 E2E**（`redis.ts` / `redis-topology.ts`）— 属于 path 驱动深度能力，与 Host 壳无关。
- **仅测 Redis UI 模块的 Vitest** — 文件在 `src/**/__tests__`，但断言对象在 `packages/drivers/redis/ui/*`。
- **Host Sync roundtrip** — `roundtrip_tests.rs` 直接依赖各 `datazen-driver-*`（及可选 olap），把驱动实现细节绑在 Host 测试图上。

结果：`basic` 全量 E2E 噪声大；驱动改动却要动 Host 测试路径；与「驱动/插件自洽」架构不一致。

## 2. 目标

1. **Host 默认零插件深度测试**：下列命令**不得**执行 Redis / Kiwi（及日后 Superset、OLAP）的插件自有用例：
   - `pnpm test:unit`
   - `cargo test -p datazen --lib`（允许留下与驱动无关的 Host 行为测；见非目标）
   - `pnpm e2e` / `pnpm e2e:minimal`（及 CI 中等价步骤）
2. **测试与实现同仓**：
   - Path 驱动（Redis 等）：`packages/drivers/<id>/` 内 Vitest / cargo /（可选）e2e。
   - Git 驱动（Kiwi 等）：测试只在插件 Git 仓库；Host **删除**对应 spec，不留 thin wrapper。
3. **显式入口可选**：Host `package.json` 可保留 `e2e:redis` 等**非默认**脚本，文档标明「手动 / 驱动维护者」；**不进** `.github/workflows/ci.yml` 默认 job。
4. **文档**：`docs/e2e-testing.md`、`AGENTS.md`、`docs/architecture/testing.md` 写清归属与如何跑。

## 3. 非目标

- 不为 Kiwi 在 Host 保留 `describe.skip` 占位 spec。
- 不把 Redis E2E 挂回默认 `pnpm e2e`。
- 不删除 Host 对 **DB_REGISTRY 路由** 的测（如 `useConnectionForm` 的 kiwi 分支 `if (!DB_REGISTRY.kiwi) return`）— 测的是 Host，不是插件实现。
- 不强制在本 PR 内给 `datazen-driver-kiwi` 私有仓落地 CI（可开 follow-up；Host 侧只负责删除与文档指向）。
- 不改变发布 SKU / `resolve-drivers` 选型行为。

## 4. 产品 / 工程规则（已锁定）

| 决策 | 选择 |
|------|------|
| 总体方案 | **C**：插件自有 CI/命令；Host 默认不拉 |
| Redis 单元 | **不进入** Host `pnpm test:unit`（从 Vitest root `include` 排除 `packages/drivers/**`，或仅排除 redis；推荐 **排除全部 `packages/drivers/**`**，改由包内 / 专用脚本跑） |
| Redis E2E | 迁至 `packages/drivers/redis/e2e/`；默认 WDIO specs 排除；`e2e:redis` 显式可选 |
| Kiwi E2E | Host 删除 `e2e/specs/kiwi.ts`；测试归属 kiwi 插件仓 |
| Host Sync roundtrip | 删除或大幅缩减对 `datazen-driver-*` 的跨 crate roundtrip；各驱动 crate 内已有 `sync_adapter` 单测为准；Host 可留 `adapter_registry` smoke（dev-dependencies 链接 path 驱动 — 属 Host 集成 smoke，**保留**；不测 Redis UI） |
| 分支 | `refactor/plugin-owned-tests` |

## 5. 迁出 / 排除清单

### 5.1 Vitest（Redis → 包内；Host test:unit 排除 packages/drivers）

| 现位置 | 动作 |
|--------|------|
| `src/windows/connection/__tests__/redisWorkbench.test.tsx` | 迁到 `packages/drivers/redis/ui/__tests__/`（合并或并列现有测）后删除宿主文件 |
| `src/windows/connection/__tests__/redisConsole.test.ts` | 同上 |
| `src/windows/connection/__tests__/infoParse.test.ts` | 同上（测 `infoParse`） |
| `src/lib/__tests__/redisSettingsSchema.test.ts` | 同上或并入已有 `settings*.test.ts` |
| `packages/drivers/redis/ui/__tests__/*`（已有） | 保留；**仅**通过 `pnpm test:unit:drivers`（新脚本）或 `vitest run packages/drivers/redis` 运行 |
| `vitest.config.ts` `include` | **移除** `packages/drivers/**/*.test.{ts,tsx}`，使 `pnpm test:unit` 不再扫驱动包 |

### 5.2 E2E

| 现位置 | 动作 |
|--------|------|
| `e2e/specs/kiwi.ts` | **删除**；文档指向 kiwi 仓 |
| `e2e/specs/redis.ts`、`redis-topology.ts` | 移到 `packages/drivers/redis/e2e/`；`e2e/wdio.conf.ts` / `run.mjs` 默认 specs **不包含**该目录 |
| `package.json` `e2e:kiwi` | 改为打印说明并 `exit 1`，或删除并在文档列出替代命令 |
| `package.json` `e2e:redis` | 改为显式 `--spec packages/drivers/redis/e2e/**` + 需已构建含 redis 的 webdriver 二进制；**不**被 `pnpm e2e` 调用 |

### 5.3 Cargo（Host）

| 现位置 | 动作 |
|--------|------|
| `src-tauri/src/sync/adapters/roundtrip_tests.rs` | 删除模块或仅保留不依赖具体 driver 类型的测；单驱动映射以各 `packages/drivers/*/src/sync_adapter.rs` 为准 |
| `adapter_registry.rs` `force_link_driver_sync_adapters` | **保留**（Host 集成链接 smoke）；olap 继续 `cfg(feature = "plugin-olap")` |
| `[dev-dependencies]` path 驱动列表 | 可保留供 registry smoke；**不**为 Redis UI 增加前端测 |

## 6. Host 命令契约（实现后）

| 命令 | 是否跑 Redis/Kiwi 插件测 |
|------|-------------------------|
| `pnpm test:unit` | **否** |
| `pnpm test:unit:drivers`（新增） | 仅 path 驱动 Vitest（含 Redis UI） |
| `cargo test -p datazen --lib` | 否 Redis UI；无 kiwi；可有 path sync registry smoke |
| `cargo test -p datazen-driver-redis` | Redis Rust 单测（包内） |
| `pnpm e2e` / `e2e:minimal` | **否** redis/kiwi specs |
| `pnpm e2e:redis` | 显式可选 |
| Kiwi E2E | 仅 kiwi 插件仓 |

## 7. 验证计划

1. `pnpm test:unit` — 通过，且输出中无 `packages/drivers/redis` 用例。  
2. `pnpm test:unit:drivers`（或等价）— Redis UI 测通过。  
3. `cargo test -p datazen --lib`（`--drivers=basic` inject）— 通过；无 roundtrip 对已删用例的依赖失败。  
4. `DATAZEN_DRIVERS=basic pnpm e2e:skip-build`（若已有 binary）或全量 e2e — specs 列表不含 kiwi/redis。  
5. **重跑**先前失败且仍属 Host 的 specs（排除 kiwi）：见实现计划 checklist。

## 8. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 忘记跑 Redis 单元导致回归 | CI 可增加 **optional** job `test-drivers-unit`（仅 path 驱动 Vitest）；若本 PR 严格「Host CI 不拉」，则该 job 可放 follow-up，本 PR 只加本地脚本 |
| Kiwi 仓暂无对等 E2E | Host 删除后短期覆盖下降；在 kiwi 仓开 issue / follow-up；文档写明 |
| Redis e2e 迁路径后 helpers 引用断裂 | 使用相对路径引用宿主 `e2e/helpers.ts`，或抽 shared 到 `e2e/helpers/` 再 import |

## 9. 实现顺序（摘要）

1. 新分支已建：`refactor/plugin-owned-tests`。  
2. Vitest：迁 Redis 宿主测 → 改 `vitest.config` + 新脚本。  
3. E2E：迁 Redis specs、删 Kiwi、改 scripts/docs。  
4. Cargo：收敛 `roundtrip_tests`。  
5. 重跑 Host 侧失败用例（无 kiwi）。

---

已批准；实现见计划文档。
