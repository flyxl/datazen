# DataZen 查询体验（Query Experience）产品定义（PRD）

> **状态**：Draft  

> **优先级**：P0 / P1（见各章节）  

> **目标版本**：v0.1.x 持续落地 → v0.2.0 前形成可感知差距收敛  

> **产品主题**：同时汲取 DataGrip 与 Navicat 的高杠杆长处，强化 SQL-first 查询闭环  

> **影响范围**`SqlEditor`、查询面板`QueryPanel`）、补全/诊断、执行与参数、格式化与 Snippet、历史/收藏/结果、设置与文档  

> **相关文档**：  

> - 桌面产品定位`docs/features/desktop-v0.1x-prd.zh-CN.md`  

> - 子代理开发`docs/development/subagent-dev-playbook.md`  

> - 竞品对照结论：内部 Review（DataGrip / Navicat SQL Editor 差距分析）

---

## 1. 文档目的

本文定义 DataZen **查询体验（Query Experience）** 的产品范围、目标用户、问题陈述、功能需求、非目标、分期交付与验收标准。

背景：相对 **DataGrip**，DataZen SQL Editor 在语言智能（写时诊断、深补全、导航、执行策略）上偏弱；相对 **Navicat**，在 Snippet、美化可配、参数查询 UX、结果运维小功能上偏弱。DataZen 自身在 AI 闭环、大结果/图表、开源与本地优先、MCP/Workflow 上具备差异化。

本 PRD 的任务不是复制任一竞品，而是：

> **用 Navicat 的方式减少重复劳动，用 DataGrip 的方式减少错误与寻找成本，用 DataZen 的方式完成「写 → 跑 → 看 → 改/AI」闭环。**

---

## 2. 产品定位

### 2.1 一句话

DataZen 查询体验是 **开发者优先的 SQL-first 工作台**：在保持轻量与开放的前提下，补齐专业编辑器的高频能力，使从 DataGrip / Navicat 迁移的用户在主路径上不感到明显残缺。

### 2.2 设计原则

1. **SQL-first，辅助可关** — Snippet、美化、AI、可视化生成均不得取代手写 SQL。  

2. **一条主路径，多层增强** — 连接与上下文 → 编写/补全 → 按策略执行 → 结果（表/图）→ 诊断/EXPLAIN/AI → 再编辑。  

3. **深度分阶段** — 先高体感、低架构风险；语言服务「尽力解析 + 可降级」，永不阻塞输入。  

4. **模块边界清晰** — Language assist / Editor ergonomics / Execution / Assets 分域演进。  

5. **开放与本地优先不回退** — 无强制云端、无核心能力付费墙；AI 出域可配置。

---

## 3. 目标用户与场景

| 用户 | 诉求 |

|------|------|

| 后端/全栈开发者 | 补全准、少误跑、少重复敲、能看计划 |

| 从 Navicat 迁移 | Snippet、美化、参数弹窗、存查询 |

| 从 DataGrip 迁移 | 执行策略、写时提示、Go to 对象 |

**非优先：** 纯可视化业务用户、企业云协作审计、重度过程体重构专家。

---

## 4. 问题陈述

| ID | 问题 | 对标 |

|----|------|------|

| Q1 | 重复 SQL 靠复制粘贴 | Navicat Snippets |

| Q2 | 格式化不可配 | Navicat Beautify |

| Q3 | 参数 UX 偏弱 | Navicat 参数查询 |

| Q4 | 执行策略单一，易误跑 | DataGrip |

| Q5 | 补全偏目录式 | DataGrip |

| Q6 | 缺少写时反馈 | DataGrip |

| Q7 | 无法跳到对象定义 | DataGrip |

| Q8 | 补全缓存刷新不显式 | Navicat |

| Q9 | 多语句耗时反馈弱 | DataGrip |

| Q10 | 历史/收藏未成轻资产中心 | Navicat |

---

## 5. 目标

**用户：** Snippet + 占位符；Beautify 可配；参数填参流；执行策略可配；补全覆盖表/列/INSERT/函数；写时提示未知表/列；Go to 对象；显式刷新补全 schema；可见耗时；历史/收藏可检索打开。

**产品：** Navicat 迁移者不别扭；DataGrip 迁移者觉得补全/执行「能用」；DataZen 的 AI/图/MCP 不被稀释。

**工程：** 不阻塞输入；设置可测；模块可独立演进。

---

## 6. 非目标

- 完整 Inspection 体系、Rename / Find Usages  

- 以 Query Builder 为主路径  

- 过程体调试器、云协作同步  

- 强制账号 / 显著增肥安装包  

- 替换 CodeMirror 内核（除非单独 RFC）  

- 全方言完美解析  

---

## 7. 竞品汲取策略

| 来源 | 汲取 | 阶段 |

|------|------|------|

| Navicat | Snippet、Beautify 选项、参数 UX、刷新补全、可选 Pin | 1 / 3 |

| DataGrip | 执行策略、深补全、写时诊断、Go to、耗时 | 1 / 2 |

| DataZen | AI、EXPLAIN、图表、MCP | 保持，与静态诊断互补 |

---

## 8. 功能需求（摘要）

**Snippet（P0）：** 内置 + 用户自定义；补全/命令插入`$1` 占位；P1 导入导出 JSON。  

**格式化（P0）：** 关键字大小写、缩进、主关键字换行等可配；全文/选区；持久化。Minify 为 P2。  

**执行与参数（P0）：** 策略 `current_statement` | `largest_statement` | `ask` | `entire_script`；选区/全部显式动作保留；未绑定参数先填参；总耗时；取消回归。多语句分段耗时 P1。  

**补全（P0–P1）：** FROM/JOIN/表.列/INSERT 列；函数库扩容；刷新补全；FK JOIN 启发为 P1。  

**写时诊断（P0–P1）：** 未知表（P0）、未知列（P1）；失败降级；与 AI 分工。  

**导航（P1）：** Go to 树/DDL。  

**资产（P0–P2）：** 收藏打开；历史检索；可选 Pin；Snippet ≠ 收藏。  

**设置（P0）：** 编辑器/格式化/执行/补全分组；诊断默认可关。  

---

## 9. 分期

| Phase | 内容 | 成功标准 |

|-------|------|----------|

| **1 体感对齐** | Snippet、Beautify、执行策略与参数、刷新补全、收藏、设置 | Navicat 向「插片段→美化→填参执行」跑通 |

| **2 语言辅助** | 补全加深、写时诊断、耗时、JOIN/Go to | 错表名执行前可见；INSERT/列补全明显改善 |

| **3 抛光** | 导入导出、历史检索、Pin、文档与性能 | 资产与稳定性 |

---

## 10. 验收标准

- Snippet 可建可插、占位可跳  

- Beautify 配置重启保持  

- 四种执行策略符合文档；默认当前语句  

- 未绑定参数不可静默当空执行  

- 刷新补全后新表可补全  

- 写时表名提示 + 大脚本不卡输入  

- Go to 有结果或明确无结果（Phase 2）  

- 耗时可见；AI/EXPLAIN/结果主路径不回归  

- 手册覆盖上述能力与非目标期望管理  

---

## 11. 风险与依赖

误报、缓存不一致、Snippet/收藏概念混淆、范围膨胀、继续堆巨型 `SqlEditor`。  

依赖：schema/FK 元数据、settings`sqlStatementRange`、bind params、QueryPanel。

---

## 12. 实施轨道建议

| Track | 内容 | Phase |

|-------|------|-------|

| `qx-snippets-format` | Snippet + Beautify | 1 |

| `qx-exec-params` | 执行策略 + 参数 + 耗时 | 1–2 |

| `qx-assist-completion` | 补全 + 刷新 | 1–2 |

| `qx-assist-diagnostics` | 写时诊断 | 2 |

| `qx-navigate` | Go to | 2 |

| `qx-assets` | 历史 / Pin / 导入导出 | 3 |

集成分支建议`feat/query-experience`。

---

## 13. 开放问题

1. 未加载 namespace 显示「加载中」还是「不存在」？  

2. Snippet 是否按连接绑定？  

3. `ask` 策略用弹窗还是状态栏选择器？  

4. Go to 优先树还是 DDL？  

5. 函数库进主仓还是按驱动分发？  

6. Phase 1 是否绑定 AI 改动？**建议否。**

---

## 14. 修订记录

| 日期 | 变更 |

|------|------|

| 2026-09-04 | 初稿：整合 DataGrip/Navicat 对比与汲取策略为完整 PRD |

---

## 附录 A — 与桌面 v0.1.x PRD

强化 SQL 编写与执行体验；不突破桌面 PRD 非目标。冲突时以桌面非目标为准。

## 附录 B — 核心用户故事

1. 用片段插入带 WHERE 的 SELECT 并填占位符。  

2. 默认只跑光标处语句，避免大文件误跑。  

3. 写错表名时编辑器能提示。  

4. 带参数查询执行前先填表（Navicat 习惯）。  

5. 从标识符跳到对象定义（DataGrip 习惯）。

---

文件路径：`/home/workdir/artifacts/query-experience-prd.zh-CN.md`](/home/workdir/artifacts/[query-experience-prd.zh-CN.md](http://query-experience-prd.zh-CN.md))  

若要下一步把该 PRD 推进到 `feat/query-experience` 分支并按 playbook 写实施计划，可以直接说。