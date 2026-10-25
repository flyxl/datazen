# DDD 模块化迁移 · 协调进度（hub）

> 依据 `.agents/skills/subagent-coordinator/SKILL.md`：本文件登记跨轨/跨会话进度，提交进 git，
> 随会话中断可恢复；整个 initiative 验收合并后删除，长期结论进代码/测试/正式文档。
> 集成分支：`feat/ddd-modules-migration`。权威方案：`docs/development/ddd-refactor-plan.md`（总纲）
> + `docs/development/ddd-refactor/`（细节，原 §2–§14）。

## 全局状态

- 当前阶段：**D1 进行中**（settings 切片已落地 `d9d6a868b`；剩余：D1 收尾清理 + D2 起各阶段）。
- 阶段与 PR 粒度：计划按 **D0–D8 九个阶段**推进（无 "wave" 概念，wave 仅是 coordinator 的
  并行批次术语）；每阶段可拆多个小 PR，每 PR 必须可构建、有真实消费者和测试。预估全程
  ~15–25 个 PR（D5 三件套各一个、D6 每功能族单独交付）。
- 环境限制（本沙箱）：**cargo 不可用**（`check-ddd-layers.mjs` 等依赖 `cargo metadata` 的
  门禁无法本地运行，ENOENT）；Node 侧检查（typecheck/vitest/scripts 检查）可用。
  Rust 侧验证需依赖 PR CI 或具备 cargo 的环境。

## 已完成

| 日期 | 提交 | 内容 |
|---|---|---|
| 2026-10-25 | `d9d6a868b` | D1 settings 切片：`datazen-modules::settings`（domain/application）、桌面仓储适配 `src-tauri/src/platform/settings.rs`、旧包过渡再导出、门禁脚本扩展（详见总纲 §3 / 07-stages.md D1 小节） |
| 2026-10-26 | `eafaf87e1` | 计划书拆分：总纲（原路径）+ `ddd-refactor/` 10 个细节文件（01–10 对应原 §2–§14，保留编号）；AGENTS.md、docs/README.md 同步；验证脚本逐行比对零丢失 |

## 下一步（按计划顺序）

1. **D1 收尾清理**（小 PR）：删除 `datazen_platform_api::settings::*`、
   `datazen_application::settings::*` 过渡导出与 `ApplicationServices` 设置门面字段
   （07-stages.md D1 兼容边界小节列明）。
2. **D2 连接领域与宿主仓储**（07-stages.md D2）：connection domain/application、
   DesktopConnectionStore 抽取、目录消费者收口；参考 04-repositories-and-security.md §7.2/§7.3。
3. D3 起按 07-stages.md 顺序推进；每阶段开工前先读 08-pr-playbook.md §11.1。

## 会话恢复要点

- 分支 `feat/ddd-modules-migration`，与远端同步；主检出即工作区（本轮文档工作未用 worktree）。
- 代码级开发（D2 起）按 coordinator 技能走 worktree + Coder/Tester 派发；Rust 编译验证依赖 CI。
- 简报落点参考：06-first-slice-table-browsing.md §9.4 的命令族台账记录了 P3 阶段命令族
  迁移事实（历史，勿混淆为 D 阶段）。
