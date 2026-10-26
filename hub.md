# DDD 模块化迁移 · 协调进度（hub）

> 依据 `.agents/skills/subagent-coordinator/SKILL.md`：本文件登记跨轨/跨会话进度，提交进 git，
> 随会话中断可恢复；整个 initiative 验收合并后删除，长期结论进代码/测试/正式文档。
> 集成分支：`feat/ddd-modules-migration`。权威方案：`docs/development/ddd-refactor-plan.md`（总纲）
> + `docs/development/ddd-refactor/`（细节，原 §2–§14）。

## 全局状态

- 当前阶段：**D3 进行中（第一切片词汇 + 第二切片端口均已归位 modules::execution）**；D2 完成 + D6 item 1（named_views）完成；下一步 D3 第三切片：执行机制迁移（runtime 的 connection/registry/gateway/resource/directory → execution::runtime）与 RuntimeConnectionUseCases 三分。
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
| 2026-10-26 | `b9ce44cd0` | D1 收尾清理：删除 `datazen_platform_api::settings::*`、`platform_api::ports::SettingsRepository`、`datazen_application::settings` 门面与 `ApplicationServices` 设置字段；`SettingsService` 改为持有 `Arc<dyn SettingsRepository>`；宿主 adapter 直接持有模块设置服务（`require_settings_service`），命令经 `platform::ipc` 取用例；feature-core/platform-api 直接依赖 `datazen-modules`（settings） |

## 下一步（按计划顺序）

1. **D3 Execution 拆分与资源机制归位**（07-stages.md D3）：`RuntimeConnectionUseCases`
   三分（application 编排 / domain 状态 / 私有机制）；`DbSessionId`/`SessionHandle`/
   `ExecutionId`/`ExecutionTarget` → `execution::domain`；会话 actor、lease、队列、
   等待、空闲回收、资源预算账本、driver registry → `execution::runtime`；旧 facade
   委托新服务。多切片交付，参考 03-execution-and-jobs.md。
2. D4 起按 07-stages.md 顺序推进；每阶段开工前先读 08-pr-playbook.md §11.1。

### D1.5 已完成（2026-10-26）

| 提交 | 内容 |
|---|---|
| `46dc15ba6` | ai-api 并入 ai-providers（第一步） |
| `e02d60d2c` | ai-providers 整体并入 modules/src/ai（第二步）；F-09 退役、F-14 豁免 reqwest |
| `a6d4b38fe` | platform-infra 改名 infra（第三步）+ 四类消费者共享审计结论 |
| `a15b228ba` | D2 第一切片：`datazen-modules::connection`（domain/application + 7 用例测试）；桌面 `DesktopConnectionStore`；删除 platform-api `ConnectionCatalog` 与 feature-core 重复 trait；`ApplicationServices` 去 connections；连接/分组/导入导出/schema diff/AI/MCP 消费者全部收口到模块服务 |
| `731c3b2d1` | D2 第二切片前半：platform-api 的 id/context/error（含全部单测）迁入 `modules::shared`（常驻，无 feature 门）；platform-api 原路径改再导出垫片 |
| `d740a3624` | D2 第二切片后半：Profile/namespace 词汇归位 `modules::connection`（domain::profile 双端口 + domain::target + application::profile_view 投影）；platform-api ports/dto/target 垫片化；PORT_TRAITS 口径扩展（声明或再导出）；4 个 to_view 调用点改 from_record；connection 模块转目录结构（domain/{catalog,profile,target}、application/{service,profile_view}） |
| `3d1da4df7` | D3 第一切片：`modules::execution` 建立（feature 依赖 connection）。domain 四文件（session/execution/target/artifact：句柄、状态机含 resolve_cancel 终态规则、能力快照、来源、ExecutionTarget+ObjectTarget、产物家族）；application 四文件（views/requests/events/idempotency：视图、回执、请求、事件信封、提交令牌）。platform-api dto 六文件 + target.rs 执行侧垫片化；测试随迁（modules 112 / platform-api 195） |
| 本次 | D3 第二切片：七个执行端口归位——domain：SessionDirectory、ArtifactRepository（含 ChunkPayload/ByteRange/导出/吊销/配额与指纹函数）+ CanonicalTarget 家族；application：ExecutionSettingsReader/ExecutionTargetResolver/IdentityResolver/SubmissionTokenIssuer/EventSink。platform-api ports 七垫片；include_str 自检测试改指新文件名；modules 167 / platform-api 145 / runtime 517 全绿 |

## 会话恢复要点

- 分支 `feat/ddd-modules-migration`，与远端同步；主检出即工作区（本轮文档工作未用 worktree）。
- 代码级开发（D2 起）按 coordinator 技能走 worktree + Coder/Tester 派发；Rust 编译验证依赖 CI。
- 沙箱磁盘 9.9G 极易打满：`CARGO_INCREMENTAL=0` 必开；测试二进制（deps 下无扩展名大文件）
  跑完即删（`find target/debug/deps -maxdepth 1 -type f -size +30M ! -name "*.rlib" ! -name "*.rmeta" ! -name "*.d" -delete`）；
  `~/.cargo/registry/src` 可随时清（自动从 cache 重解压）。
- 简报落点参考：06-first-slice-table-browsing.md §9.4 的命令族台账记录了 P3 阶段命令族
  迁移事实（历史，勿混淆为 D 阶段）。
