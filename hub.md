# DDD 模块化迁移 · 协调进度（hub）

> 依据 `.agents/skills/subagent-coordinator/SKILL.md`：本文件登记跨轨/跨会话进度，提交进 git，
> 随会话中断可恢复；整个 initiative 验收合并后删除，长期结论进代码/测试/正式文档。
> 集成分支：`feat/ddd-modules-migration`。权威方案：`docs/development/ddd-refactor-plan.md`（总纲）
> + `docs/development/ddd-refactor/`（细节，原 §2–§14）。

## 全局状态

- 当前阶段：**D1 已完成**（settings 切片 `d9d6a868b` + 收尾清理见下表）；下一步 **D2 连接领域**，
  以及用户新增的三条架构原则落地（见「架构原则更新」）。
- 阶段与 PR 粒度：计划按 **D0–D8 九个阶段**推进（无 "wave" 概念，wave 仅是 coordinator 的
  并行批次术语）；每阶段可拆多个小 PR，每 PR 必须可构建、有真实消费者和测试。预估全程
  ~15–25 个 PR（D5 三件套各一个、D6 每功能族单独交付）。
- 环境限制（本沙箱）：**cargo 可用**（`~/.cargo/bin`，注意默认 PATH 不含，需
  `export PATH="$HOME/.cargo/bin:$PATH"`；磁盘紧张，构建用 `CARGO_INCREMENTAL=0`，
  定期清理 `target/debug/incremental`）。**src-tauri（桌面宿主）无法本地编译**：
  缺 GTK/WebKit 系统库且无 sudo；桌面侧改动靠 grep 审计 + 定向 cargo check（依赖
  crates）+ PR CI 验证。Node 侧检查（typecheck/vitest/scripts 检查）可用。

## 架构原则更新（用户 2026-10-26 指令，覆盖计划书旧表述）

1. **AI crate 收拢**：`ai-api` 并入 `ai-providers`；两者合并后**整体并入 modules**
   （`modules/src/ai/`），不再保留独立 AI crate。
2. **platform-infra 改名 infra**：`packages/platform-infra` → `packages/infra`
   （crate 名 `datazen-infra`），定位为 **drivers / modules / web / desktop 四类消费者
   中任意两者共用**的基础设施工具的家，提高代码复用率。
3. **Repository trait 放 domain 层**：trait 定义在各功能模块的 `domain`，
   具体实现由 web/desktop 宿主的 infrastructure 提供（与 01-target-architecture.md
   §4.1 第 5 条一致，后续所有模块迁移照此执行）。

## 已完成

| 日期 | 提交 | 内容 |
|---|---|---|
| 2026-10-25 | `d9d6a868b` | D1 settings 切片：`datazen-modules::settings`（domain/application）、桌面仓储适配 `src-tauri/src/platform/settings.rs`、旧包过渡再导出、门禁脚本扩展（详见总纲 §3 / 07-stages.md D1 小节） |
| 2026-10-26 | `eafaf87e1` | 计划书拆分：总纲（原路径）+ `ddd-refactor/` 10 个细节文件（01–10 对应原 §2–§14，保留编号）；AGENTS.md、docs/README.md 同步；验证脚本逐行比对零丢失 |
| 2026-10-26 | 本次 | D1 收尾清理：删除 `datazen_platform_api::settings::*`、`platform_api::ports::SettingsRepository`、`datazen_application::settings` 门面与 `ApplicationServices` 设置字段；`SettingsService` 改为持有 `Arc<dyn SettingsRepository>`；宿主 adapter 直接持有模块设置服务（`require_settings_service`），命令经 `platform::ipc` 取用例；feature-core/platform-api 直接依赖 `datazen-modules`（settings） |

## 下一步（按计划顺序）

1. **计划文档更新**（小 PR）：把三条架构原则写进 01-target-architecture.md（§3.2 四类
   组织单元、§3.3 目录示意）与 07-stages.md（D6 AI 小节、新增 infra 说明）。
2. **AI crate 合并**（两个 PR）：① `ai-api` 并入 `ai-providers`；② 合并后的
   `ai-providers` 整体并入 `modules/src/ai/`，删除两个独立 crate。
3. **platform-infra → infra**（一个 PR）：改名 + 审计 drivers/modules/web/desktop
   共享代码收拢进 infra。
4. **D2 连接领域与宿主仓储**（07-stages.md D2）：connection domain/application、
   DesktopConnectionStore 抽取、目录消费者收口；参考 04-repositories-and-security.md §7.2/§7.3。
5. D3 起按 07-stages.md 顺序推进；每阶段开工前先读 08-pr-playbook.md §11.1。

## 会话恢复要点

- 分支 `feat/ddd-modules-migration`，与远端同步；主检出即工作区（本轮文档工作未用 worktree）。
- 代码级开发（D2 起）按 coordinator 技能走 worktree + Coder/Tester 派发；Rust 编译验证依赖 CI。
- 简报落点参考：06-first-slice-table-browsing.md §9.4 的命令族台账记录了 P3 阶段命令族
  迁移事实（历史，勿混淆为 D 阶段）。
