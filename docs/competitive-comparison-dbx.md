# 竞品分析：DBX（t8y2/dbx） vs DataZen

> 分析日期：2026-08-09（**ABC + Schema Diff Deploy P1–P3 落地后第二轮**）。  
> 数据来源：本仓库 `feat/abc-competitive-parity`（`aaf54cf`）、GitHub API、DBX README。  
> DBX: https://github.com/t8y2/dbx ｜ DataZen: https://github.com/flyxl/datazen  
> 可视化画布：[datazen-dbx-reanalysis-post-abc.canvas.tsx](/Users/wuxiaolong/.cursor/projects/Users-wuxiaolong-code-rust-projects-datazen-worktrees-feat-abc-competitive/canvases/datazen-dbx-reanalysis-post-abc.canvas.tsx)

## 1. 本轮结论

ABC 与 Schema Diff Deploy 已把「信任面 / 每日 SQL 主路径 / 结构部署」从**明显落后**拉到**可对标**。

| 局面 | 说明 |
|------|------|
| **已收口** | Schema Diff Deploy；MCP allowlist + agent 片段；列级侧栏搜索；Workflow 模板 / 自动图表 / Ollama |
| **仍独有** | YAML Workflow、查询图表、MCP Client、运行时主题包、Basic/All SKU、Redis E1–E4 |
| **仍落后** | 70+ 长尾（JDBC/Agent）、Docker/Web/CLI/npm MCP、安装渠道实际上架、字段血缘 / 全局库搜 / 表结构编辑器 |
| **势能** | DBX ~13.7k★ / v0.5.77 vs DataZen ~12★ / v0.0.8；Apache-2.0 vs GPL-3.0 |

**策略不变**：不拼「70+」；用纵深差异化赢细分用户；下一刀优先 **分发落地** 与 **Schema Diff 产品化深度**，而不是再堆对标 checklist。

## 2. 基本数据（2026-08-09）

| 维度 | DBX | DataZen |
|------|-----|---------|
| Star / Fork | ~13,765 / ~1,414 | ~12 / 4 |
| License | Apache-2.0 | GPL-3.0 |
| 最近发布 | v0.5.77（2026-08-06） | v0.0.8（2026-08-07） |
| 框架 | Tauri 2 + Vue 3 | Tauri 2 + React 18 |
| 运行形态 | 桌面 + Docker/Web + CLI + MCP npm | 桌面 + MCP（`--mcp` / `--mcp-stdio`） |
| 安装渠道 | Homebrew / Scoop / WinGet / Flatpak | 模板与文档有，渠道上架仍待 ops |

## 3. 能力对照（落地后）

| 能力 | DBX | DataZen | 判断 |
|------|-----|---------|------|
| Schema Diff Deploy | ✅ | ✅ Compare→Plan→Review→Deploy（多表/索引/跨方言 IR） | **主路径追平** |
| MCP 权限 / allowlist | ✅ | ✅ + denylist + 设置内配置片段 | 信任面接近；**分发形态仍落后** |
| 库内 / 对象搜索 | 全局库搜 | 侧栏表·视图·列搜索 | 部分追平 |
| Workflow | ❌ | ✅ YAML + 内置模板 + MCP `run_workflow` | **DZ 独有** |
| 结果图表 | ❌（无一等公民） | ✅ + 可选自动切图 | **DZ 独有** |
| MCP Client | ❌ | ✅ | **DZ 独有** |
| Redis | 浏览 + 命令台 | E1–E4 深运维 | **DZ 更深** |
| 主题 / SKU | 暗色 + 编辑器主题；~20MB 一体 | 运行时主题包；Basic/All/自定义列表 | **DZ 独有** |
| DB 广度 | 70+（含 JDBC/Agent/MQ） | Path + git；不做 JDBC | **DBX 护城河** |
| 字段血缘 / 结构编辑器 | ✅ | ❌ | **仍落后** |
| 文件预览（Parquet 等） | ✅ DuckDB | DuckDB 在 All SKU，无拖放预览产品化 | 仍弱 |
| AI Provider | Claude / OpenAI / Ollama / 兼容 | OpenAI / Anthropic / DeepSeek / Ollama / Custom | 接近 |

## 4. ABC 执行效果

| 路径 | 已做 | 仍缺 |
|------|------|------|
| **A 分发与信任** | MCP allowlist、agent 片段、packaging 文档 | Homebrew/WinGet 实际上架、独立 MCP 包、Docker/Web |
| **B 每日 SQL** | 列搜索、独立 Schema Diff + Deploy | 血缘、对象浏览器、可视化表结构编辑 |
| **C 差异化包装** | 内置 Workflow、自动图表、Ollama | 官网/演示叙事统一 |

## 5. 建议的下一刀

1. **高 · 分发落地**：渠道上架 + MCP 安装摩擦（不必先做 Web）。  
2. **高 · Schema Diff 产品化**：改列预览 DDL / 影响面；先「敢在预发用」。  
3. **中 · 强化独有叙事**：Workflow + 图表 + MCP Client + Redis + SKU。  
4. **纪律 · 明确不做**：JDBC 堆量、完整 Docker/Web 复刻、MQ 控制台（无合同则不做）。

## 6. 相关文档

- [Schema Diff 使用手册](schema-diff-guide.md) · [Deploy 摘要](schema-diff-deploy.md)
- [Packaging](packaging.md) · [Updater](updater.md)
- [ABC 计划](superpowers/plans/2026-08-09-abc-competitive-parity.md)
- [Schema Diff Deploy 计划](superpowers/plans/2026-08-09-schema-diff-deploy.md)
