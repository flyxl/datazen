# Post-Review Hardening — 实施计划

> **PRD**：`docs/todo/post-review-hardening-prd.zh-CN.md`  
> **Playbook**：`docs/development/subagent-dev-playbook.md`  
> **集成分支**：`feat/post-review-hardening`（各轨 `feature/prh-*` 合入此分支）  
> **协调总览**：`docs/development/coordination/hub.md`  
> **轨目录**：`docs/development/coordination/tracks/<track-id>/`

本计划按子代理并行开发 Playbook 编排：分轨依据是**文件冲突面**；每轨独立 worktree；编码与测试分离；全量 E2E 仅在 R 阶段执行。

---

## 0. 角色与集成分支约定

| 角色 | 约束 |
|------|------|
| 协调者 | 不写业务代码；维护 hub；merge / worktree 清理；写锁台账 |
| 编码代理 | 仅在 `.worktrees/datazen-<track>`；`scripts/new-feature-worktree.sh <track> feat/post-review-hardening` |
| 测试代理 | 全新实例、独立 worktree；只测不修 |
| 基线 | 各轨 base = `feat/post-review-hardening` |

**自检（每个代理 BOOTSTRAP 必报）**：`pwd`、`git rev-parse --show-toplevel`、`git branch --show-current`、`git status --short`。路径必须为分配的 worktree；分支不得为 `main`。

---

## 1. 波次编排

### Wave 1（无共享写冲突，四轨并行）

| Track | PRD | 任务摘要 | 主要写路径 | 冲突面 |
|-------|-----|----------|------------|--------|
| **prh-split-mcp** | T1 | 拆分 `mcp/server.rs` | `src-tauri/src/mcp/**` | 与其它 Wave 1 轨无重叠 |
| **prh-split-dcmd** | T1 | 拆分 `commands/driver_command.rs` | `src-tauri/src/commands/driver_command.rs` + 新建子模块 | 仅可能碰 `commands/mod.rs` 导出行 |
| **prh-sql-guard** | T3+T8 | Safe Mode 文档/单测/高危确认入口 | `sql_guard.rs`、相关 commands、少量前端确认 UI、手册 | 避免改 `mcp/server.rs` |
| **prh-ai-egress** | T4+T8 | AI 默认出域策略 + safety 单测 + 设置提示 | `ai/safety.rs`、AI settings UI、prompt/context 装配 | 避免改 MCP server 大文件 |

**Wave 1 合并策略**：任一轨测试闭环即可合入集成分支；合并后协调者跑 `tsc --noEmit` + 定向 `cargo test -p datazen --lib`（basic features）健全性检查。`commands/mod.rs` 若两边都改导出，协调者人工并集。

### Wave 2（依赖 Wave 1 合并后的稳定树）

| Track | PRD | 任务摘要 | 主要写路径 | 依赖 / 冲突 |
|-------|-----|----------|------------|-------------|
| **prh-split-lib** | T1 | 拆分 `lib.rs` 编排层 | `src-tauri/src/lib.rs` + 新建子模块（如 `app_menu.rs` / `bootstrap.rs`） | 等 Wave 1 合并 |
| **prh-panic-policy** | T2 | 生产路径 unwrap 约定 + 关键路径治理 | `services/connection_manager.rs`、`store/**`、IPC 入口；文档 | 与 split-lib 避让同文件 |
| **prh-contract** | T5+T8 | 外部契约策略文档 + MCP golden 测试 | `docs/development/**`、mcp 测试、PR 模板 | **必须在 split-mcp 合并后** |
| **prh-ci-docs** | T6+T7+T9 | CI 矩阵文档、窗口/store 边界文档、onboarding | 文档 + 可选 workflow 小改 | 几乎无代码冲突 |

### R 阶段（全部轨道 MERGED 后）

见 §6。E2E 仅在各轨测试轮**登记**用例，标注【留待 R 回归】。

---

## 2. 落点侦察（协调者预填；代理须自行核实）

> 行号以计划编写时快照为准，实施前以当前分支重新定位。

### Track prh-split-mcp

| 文件 | 约行数 / 位置 | 说明 |
|------|----------------|------|
| `src-tauri/src/mcp/server.rs` | ~1702 | 拆分主目标 |
| `src-tauri/src/mcp/mod.rs` | 模块根 | 导出调整 |
| 建议拆出 | 新建 | 如 `mcp/tools.rs`、`mcp/resources.rs`、`mcp/stdio.rs`、`mcp/server_core.rs`（以实际职责为准，行为不变） |

**验收**：

- [ ] `server.rs` 降为编排层或显著减负（建议单文件 ≤800 行，或入口 + 子模块清晰）
- [ ] 公开行为不变：MCP tools/resources/stdio 契约不变
- [ ] `cargo test -p datazen --lib`（含 mcp 相关）通过
- [ ] 无新增对外 API 改名

**范围外**：不改工具语义、不改权限模型、不做功能增强。

---

### Track prh-split-dcmd

| 文件 | 约行数 / 位置 | 说明 |
|------|----------------|------|
| `src-tauri/src/commands/driver_command.rs` | ~1573 | 拆分主目标 |
| `src-tauri/src/commands/mod.rs` | 注册/导出 | 仅追加 `mod` 与 re-export |
| 建议拆出 | 新建同目录子模块 | discovery / validation / execute / streaming |

**验收**：

- [ ] 职责拆分清晰；入口文件不再承载全部实现细节
- [ ] IPC 命令签名与前端调用不变
- [ ] `cargo test -p datazen --lib` 通过

**范围外**：不改 Driver Command 协议；不新增 Host 按 driver_type 硬编码分支。

---

### Track prh-sql-guard

| 文件 | 位置 | 说明 |
|------|------|------|
| `src-tauri/src/sql_guard.rs` | ~424 行 | 核心逻辑 + 单测扩展 |
| 调用方 | 代理 grep `check_sql` | 确认钩子完整 |
| 前端确认 UI | 查询执行 / 危险语句路径 | DROP/TRUNCATE 等二次确认 |
| 文档 | `docs/architecture/security.md`、手册 Safe Mode 说明 | 「尽力防护」表述 |

**验收**：

- [ ] 用户可见文档写明 Safe Mode / 只读为尽力防护、非形式化保证
- [ ] 单测至少覆盖：只读拦截写、Safe Mode 无 WHERE 的 UPDATE/DELETE、拦截 DROP/TRUNCATE、多语句混合
- [ ] 高危操作确认：GUI 主路径可用（无头路径本轨默认仅文档记录行为）
- [ ] 不在本轨实现完整 AST 重写

---

### Track prh-ai-egress

| 文件 | 位置 | 说明 |
|------|------|------|
| `src-tauri/src/ai/safety.rs` | ~367 行 | 脱敏与限制；扩展单测 |
| AI context 装配 | `ai/context.rs`、`schema_pipeline.rs` 等 | 默认是否附带结果行 |
| 前端设置 | settings / AI 面板 | 开关与首次提示 |
| 文档 | 手册 AI 章节 | 出域说明 |

**验收**：

- [ ] 默认不自动附带查询结果行（或默认严格模式等价行为）
- [ ] 开启高敏感上下文时有明确「数据离开本机」提示
- [ ] `ai/safety` 单测覆盖常见密钥键、URI 用户信息、Bearer
- [ ] 既有 AI 单测不回归

---

### Track prh-split-lib

| 文件 | 位置 | 说明 |
|------|------|------|
| `src-tauri/src/lib.rs` | ~1491 行 | 菜单映射、启动与生命周期编排外移 |
| 建议新建 | `app_menu.rs` 等 | 保持 `pub`/`pub(crate)` 可见性 |

**验收**：

- [ ] `lib.rs` 以模块声明与启动编排为主
- [ ] 菜单行为与启动路径不变
- [ ] 全量 lib 测试通过

---

### Track prh-panic-policy

| 文件 | 说明 |
|------|------|
| `AGENTS.md` 或 `docs/development/` | 生产路径避免裸 unwrap；测试除外；故意 panic 须注释 |
| `services/connection_manager.rs`、`store/**`、高频 commands | 关键路径审视 |
| `CONTRIBUTING.md` | PR checklist 一项 |

**验收**：

- [ ] 约定文档合入
- [ ] 至少完成一轮关键路径替换或意图标注
- [ ] 不引入行为变化；测试绿

---

### Track prh-contract

| 文件 | 说明 |
|------|------|
| `docs/development/external-contract-policy.md`（名称可微调） | Deprecation 策略：v0.x vs 近 1.0 |
| MCP 测试 | golden JSON 或现有测试扩展 |
| PR 模板或 `CONTRIBUTING.md` | 破坏性契约检查项 |

**验收**：

- [ ] 策略文档合入
- [ ] 至少一条 MCP 契约测试进 CI
- [ ] CONTRIBUTING 或模板含检查项

---

### Track prh-ci-docs

| 文件 | 说明 |
|------|------|
| CI/测试矩阵短文 | Basic 必测 + path 轮转 + 契约；All 不进 PR CI |
| `docs/architecture/windows.md` | 主工作区 vs 子窗口 |
| `README.md` / `CONTRIBUTING.md` | toolchain 推荐版本 |

**验收**：

- [ ] 文档更新合入
- [ ] 与当前 `.github/workflows/ci.yml` 行为一致（若仅文档，不强制改 workflow）

---

## 3. 进度与 Bug 文件布局

```text
docs/development/coordination/
├── hub.md
├── post-review-hardening-plan.md
└── tracks/
    ├── prh-split-mcp/{progress.md,bugs.md}
    ├── prh-split-dcmd/{progress.md,bugs.md}
    ├── prh-sql-guard/{progress.md,bugs.md}
    ├── prh-ai-egress/{progress.md,bugs.md}
    ├── prh-split-lib/{progress.md,bugs.md}
    ├── prh-panic-policy/{progress.md,bugs.md}
    ├── prh-contract/{progress.md,bugs.md}
    └── prh-ci-docs/{progress.md,bugs.md}
```

- Bug ID：`<track-id>-BUG-nnn`
- 代理**只写本轨** `tracks/<id>/`；不得改 hub

---

## 4. 单轨状态机（Playbook §3.4）

```text
DISPATCHED → BOOTSTRAP → CODING → READY_FOR_TEST
                              ↓
                         BLOCKED/PAUSED
READY_FOR_TEST → TESTING → PASSED → READY_TO_MERGE → MERGED → CLEANUP → CLOSED
                         └→ FAILED → BUG_RECORDED → REPAIRING → TESTING
```

---

## 5. 编码 / 测试完成标准（各轨通用）

### 编码代理自验三件套（basic 驱动）

```bash
node scripts/generate-builtin-locales.mjs
cargo test -p datazen --lib --features <basic-features>
npx tsc --noEmit          # 若改 TS
npx vitest run <paths>    # 若改 TS
```

- 禁 `pnpm install`；禁 bash 全仓 grep；禁 add codegen / 未跟踪规格文档
- 返回前：`git status --short --branch`、`git diff --check`、`READY_FOR_TEST`

### 测试代理

- 范围完整性 + diff 逻辑审查；独立重跑三件套
- E2E 只登记，标注【留待 R 回归】
- 问题只写本轨 `bugs.md`

---

## 6. R 阶段清单

- [ ] `cargo test -p datazen-driver-api --lib`
- [ ] `cargo test -p datazen --lib`（basic）
- [ ] basic path drivers lib 测试
- [ ] `cargo test -p datazen-ai-api --lib`
- [ ] `pnpm typecheck` / unit
- [ ] 登记的 E2E 用例
- [ ] 集成分支 → main PR 说明

---

## 7. 写锁台账模板

| Track | 写锁代理 | Worktree | Branch | Phase | 最后心跳 |
|-------|----------|----------|--------|-------|----------|
| prh-split-mcp | — | — | feature/prh-split-mcp | — | — |
| prh-split-dcmd | — | — | feature/prh-split-dcmd | — | — |
| prh-sql-guard | — | — | feature/prh-sql-guard | — | — |
| prh-ai-egress | — | — | feature/prh-ai-egress | — | — |
| prh-split-lib | — | — | feature/prh-split-lib | — | — |
| prh-panic-policy | — | — | feature/prh-panic-policy | — | — |
| prh-contract | — | — | feature/prh-contract | — | — |
| prh-ci-docs | — | — | feature/prh-ci-docs | — | — |

---

## 8. 风险与协调注意

| 风险 | 处理 |
|------|------|
| `commands/mod.rs` 双轨改导出 | 合并时并集 |
| split-mcp 后测试路径变化 | contract 在 split-mcp 合并后启动 |
| AI 默认策略体验回退 | 严格模式可关；验收以默认安全为准 |
| 大文件拆分回归 | 行为不变；小步 PR |

---

## 9. 协调者启动检查清单

1. [ ] 本分支 `feat/post-review-hardening` 已含 PRD + 计划 + hub + tracks
2. [ ] 为 Wave 1 四轨执行 `scripts/new-feature-worktree.sh <track> feat/post-review-hardening`
3. [ ] 按 Playbook 附录 A 派发编码简报
4. [ ] 登记写锁与心跳观察

---

## 10. 修订记录

| 日期 | 变更 |
|------|------|
| 2026-09-03 | 初稿：按 subagent-dev-playbook 编排 |
| 2026-09-03 | 与 PRD 合并至单一分支 `feat/post-review-hardening` |
