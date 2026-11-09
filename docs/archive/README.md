# 过程文档归档

本目录存放**已交付过程文档**的只读归档：立项计划、PRD、设计稿、进度台账、worklog 与评审记录。它们在对应工作交付后移入此处，不再随代码演进维护——结论以活跃文档（`docs/architecture/`、`docs/features/`、`docs/development/`）与代码为准；需要追溯决策过程与历史语境时再回这里。

- 归档时间：2026-11-09（W8 迭代收官后统一归档）。
- 归档纪律见 [docs/README.md](../README.md) 维护规则第 3 条：过程文档不入活跃索引，交付后移入本目录。
- 文档内的路径与相对链接按归档前的位置书写，移入后可能不再可达——它们是历史语境的一部分，不做改写。

## development/（原 `docs/development/`）

### 平台与形态计划（均已交付或收敛）

| 文档 | 内容 | 终态 |
|---|---|---|
| `platform-development-plan.md` | 连接重构与桌面 / Web 多形态 P0–P10 分阶段开发计划 | P1–P7 已交付（P7 团队 Web 服务随 W8 交付）；P8 桌面团队认证、P9 多实例协调、P10 全驱动发布验收未启动 |
| `ddd-refactor-plan.md` + `ddd-refactor/`（10 文件） | DDD 模块化重构总纲与细节（原 §2–§14） | D0–D8 全部完成；旧 `packages/kernel` crate 已退役删除，内核收敛为 `crates/datazen/` |
| `frontend-form-split-plan.md` | 前端形态拆分（`packages/frontend` / `packages/desktop` 分家）F1–F6 实施计划 | 全部落地 |
| `frontend-form-split-worklog.md` | 形态拆分过程 worklog | 随计划归档 |

### W8 迭代（团队 Web 服务点亮，2026-11-05 至 11-09，全部交付）

| 文档 | 内容 |
|---|---|
| `web-ready-plan.md` | W8 权威进度台账：WR-A–WR-G 七轨任务表 + §11 共 85 条倒序回写（含全部设计裁决与实现事实）；契约演进的原始依据 |
| `w8-handoff.md` | W8 交接文档（点亮路径两刀的操作手册） |
| `w8-mutation-knife0-1-design.md` / `w8-knife2-live-handle-design.md` / `w8-knife3-mutation-pair-design.md` / `w8-knife4-result-cell-design.md` | 表格变更四刀设计档（变更集 / 活跃句柄 / 变更对 / 结果单元格写回） |
| `w8-knife4-wapp-static-design.md` | wapp 资产 origin 静态路由设计档（实施 PR #226，契约 §5） |
| `w8-idle-reclaimer-arming-design.md` | IdleReclaimer 武装设计档（实施 PR #203） |
| `w8-fk-mutation-assessment.md` | 外键变更影响评估 |

### 其他已交付过程文档

| 文档 | 内容 |
|---|---|
| `ai-assistant-parity-prd.md` | AI 助手能力对标 PRD（T01–T26，PR #205；结论已折叠进 [backend/ai.md](../../architecture/backend/ai.md)） |
| `language-pack-install-design.md` | 设置页安装外部语言包设计档（PR #214） |
| `e2e-ipc-migration-guide.md` | E2E 纯 IPC 用例向 Rust 集成测试迁移指南（Track E） |
| `code-organization-review.md` / `commands-layer-review.md` / `host-layer-inventory.md` / `host-boundary-followups.md` | 早期代码组织 / 命令层评审与宿主层盘点（结论已随 DDD 重构落地） |
