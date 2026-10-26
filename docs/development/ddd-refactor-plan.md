# DDD 模块化重构：目标架构与实施指南

> **状态：已确认的目标设计，尚未按本方案完成实现。** 确认日期：2026-10-24；本次代码核对基线：`main`，`07bb93a261086b8aebf9c1472ca44c339bd3c55e`。
>
> **两项已确认决策**：application 是 `modules::<功能>::application`，最终不保留独立 application crate；数据库执行机制主要进入 `modules::execution::runtime`，Job 等机制分别归属自己的功能模块。
>
> **三项追加决策（2026-10-26）**：① `ai-api` 并入 `ai-providers` 后整体并入 `modules::ai`，不保留独立 AI crate；② `platform-infra` 改名 `infra`，四类消费者（drivers/modules/web/desktop）中任意两者共用的无业务工具收拢进去；③ Repository trait 定义在模块 domain 层，具体实现由 web/desktop 宿主提供。详见 [02 §5.4](ddd-refactor/02-legacy-crate-migration.md) 与 [01 §3.2](ddd-refactor/01-target-architecture.md)。
>
> 本方案整体替代此前以横向 application / platform-api / runtime crate 为长期目标的方案，不是在旧方案后面追加另一套规则。旧代码仍存在不代表目标变回旧结构；目标已经确认也不代表代码已经迁移。
>
> **文档结构（2026-10-26 起拆分）**：本文是总纲，只保留阅读路线、文档权威、完成边界、细节文件地图与阶段总览；实施细节按主题放在 [`ddd-refactor/`](ddd-refactor/) 目录下的细节文件。细节文件保留原 §2–§14 章节编号，代码注释、其他文档与历史讨论中的「§N.M」引用继续对号入座，不需要批量改写。

## 1. 如何使用本文

### 1.1 阅读路线与文档权威

第一次接触项目，按以下顺序阅读：

1. [01 目标架构](ddd-refactor/01-target-architecture.md)（原 §2–§4）：理解术语、最终目录和依赖方向。
2. [02 旧 crate 去向](ddd-refactor/02-legacy-crate-migration.md)（原 §5）至 [05 宿主装配](ddd-refactor/05-host-assembly.md)（原 §8）：理解旧包去向、执行机制、仓储、身份和宿主装配。
3. [06 第一条完整示范](ddd-refactor/06-first-slice-table-browsing.md)（原 §9）：用表浏览这一条真实链路理解如何实现。
4. [07 分阶段实施](ddd-refactor/07-stages.md)（原 §10）与 [08 PR 操作步骤](ddd-refactor/08-pr-playbook.md)（原 §11）：选一个可独立验收的阶段，按步骤开发。
5. [09 验收测试设计](ddd-refactor/09-acceptance-tests.md)（原 §12）与 [10 检查与发布](ddd-refactor/10-checks-and-release.md)（原 §13–§14）：编写测试、运行检查、判断能否合并和发布。

权威范围按问题划分，不用一句“代码永远优先”掩盖目标与现状的差别：

- **未来 Rust 包组织、模块归属和 DDD 迁移顺序**：以本方案（总纲与其细节文件）为准。
- **当前实现位置**：以当前分支代码与测试为准。[01 目标架构](ddd-refactor/01-target-architecture.md)的 §3.1 只记录上述基线的事实。
- **连接 DTO、错误码、会话状态和资源约束**：沿用[连接管理详细设计](../architecture/platform/connection-management.md)，本轮不重新定义协议。
- **产品形态、Web 安全与多实例交付**：沿用[平台开发计划](platform-development-plan.md)、[团队 Web 服务设计](../architecture/platform/team-server-and-auth.md)和[持久化模型](../architecture/platform/persistence-model.md)。这些文档中的旧包路径与总 application 装配方式由本方案替代，功能、安全与恢复要求不因此取消。
- **已实现架构文档**：随各阶段落地逐节更新；不能先把目标目录写成已存在。

本方案是已获授权的桌面 / Web 平台演进设计的一部分，不新增另一份 PRD、进度文档或评审记录。旧方案历史通过 Git 查看，不在本方案保留过期的行数指标、测试计数和互相冲突的候选方案。

### 1.2 完成与未完成的边界

本方案交付的是实施方案，不是一次实际发布。`modules/`、`web/` 及本方案新命名的服务和 feature 均为**目标**，不能直接宣称已经可用。

重构不同时做数据库协议升级、持久化格式重写、前端重写或微服务拆分。必要的协议/存储变更另行分期，给出兼容、数据迁移和回滚验证。

## 2. 细节文件地图

细节文件统一放在 [`ddd-refactor/`](ddd-refactor/) 目录；每篇开头有返回总纲的链接，相互之间沿用「§N.M」编号引用并附目标文件链接。

| 细节文件 | 主题 | 原章节 |
|---|---|---|
| [01-target-architecture.md](ddd-refactor/01-target-architecture.md) | 分层概念（Port、五类职责、三个 ID）、目标代码组织、依赖方向与模块边界 | §2–§4 |
| [02-legacy-crate-migration.md](ddd-refactor/02-legacy-crate-migration.md) | platform-api / application / runtime / kernel / feature-core 与迁移包的去向、防 Cargo 环 | §5 |
| [03-execution-and-jobs.md](ddd-refactor/03-execution-and-jobs.md) | Execution 与 Job 的详细边界、等待能力、runtime 实例与关停 | §6 |
| [04-repositories-and-security.md](ddd-refactor/04-repositories-and-security.md) | Repository 设计、连接目录统一、DesktopRepository 拆分、一致性与授权 | §7 |
| [05-host-assembly.md](ddd-refactor/05-host-assembly.md) | Desktop / Web 模块级装配、三种「可用」、Cargo feature 策略 | §8 |
| [06-first-slice-table-browsing.md](ddd-refactor/06-first-slice-table-browsing.md) | 第一条完整示范：连接配置到表浏览；含已落地命令族台账 | §9 |
| [07-stages.md](ddd-refactor/07-stages.md) | 分阶段实施与验收：D0–D8 各阶段输入、工作、退出与回滚 | §10 |
| [08-pr-playbook.md](ddd-refactor/08-pr-playbook.md) | 每个 PR 的操作步骤、自查方向与提交材料 | §11 |
| [09-acceptance-tests.md](ddd-refactor/09-acceptance-tests.md) | 验收测试设计：B/R/E/T 编号用例与验证质量要求 | §12 |
| [10-checks-and-release.md](ddd-refactor/10-checks-and-release.md) | 检查命令与证据记录、发布标准与最终交付清单 | §13–§14 |

## 3. 阶段总览（D0–D8）

使用 **D0–D8** 编号，避免与平台产品计划的 P0–P10 混淆。每阶段可以有多个小 PR，但每个 PR 必须构建可用、有真实消费者和测试；不先提交一堆空接口等待别人接线。各阶段的输入、工作项、退出标准与回滚方式见 [07-stages.md](ddd-refactor/07-stages.md)。

| 阶段 | 目标 | 状态（2026-10-26） |
|---|---|---|
| D0 | 建立可信基线 | 已完成：基线核对见文首状态行 |
| D1 | 新 modules 与第一条可运行小切片 | 已完成：settings 切片 + 收尾清理（旧过渡导出已删，详见 07-stages.md D1 小节） |
| D1.5 | AI crate 收拢与 infra 改名（2026-10-26 追加） | 已完成：三步合并/改名 + 共享审计结论见 07-stages.md D1.5 小节 |
| D2 | 连接领域与宿主仓储 | 已完成：目录切片 + Profile/revision 词汇统一 + modules::shared 建立（见 07-stages.md D2 小节）；桌面 UI 旅程验证依赖 PR CI |
| D3 | Execution 拆分与资源机制归位 | 进行中：第一切片（词汇归位 domain/application）已完成；端口与机制、编排拆分待做 |
| D4 | Schema、表读写与 SQL 消费者 | 未开始 |
| D5 | Jobs 与迁移三件套 | 未开始 |
| D6 | 其余功能与旧宿主业务移除 | 进行中：item 1（named_views）已迁移 modules |
| D7 | 真实 Web 装配与功能选择验收 | 未开始 |
| D8 | 旧包退役与发布收尾 | 未开始 |

状态列是随阶段推进更新的快照；逐 PR 的验证证据记录在 PR 描述中，不另建长期进度文件。
