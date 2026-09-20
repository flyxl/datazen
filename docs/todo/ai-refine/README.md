# AI 功能优化（ai-refine）

本目录收录 DataZen AI 模块系统性优化的需求、设计与实施方案，覆盖 AI Review 中列出的全部 P0/P1/P2 问题。

- [PRD（需求）](./prd.zh-CN.md) — 背景、目标、功能需求 FR-01~FR-20、验收标准
- [技术设计](./design.zh-CN.md) — 架构、模块设计、IPC 契约、数据模型
- [实施方案](./implement.zh-CN.md) — 分阶段任务、文件清单、测试策略

## 问题速览

| 级别 | 主题 | PRD | 设计 |
|------|------|-----|------|
| P0 | 流式可取消 | FR-01 | §2 |
| P0 | NL2SQL 流式预览 | FR-02 | §3 |
| P0 | 会话按连接隔离 | FR-03 | §4 |
| P0 | JSON 鲁棒解析 + 长任务反馈 | FR-04 | §5 |
| P0 | Tool Loop 护栏 | FR-05 | §6 |
| P0 | 安全门统一 | FR-06 | §7 |
| P1 | Markdown 渲染 + 代码块动作 | FR-07/FR-08 | §8 |
| P1 | Chat 交互细节 | FR-09 | §8 |
| P1 | ContextPicker 性能与隔离 | FR-10 | §9 |
| P1 | Schema 上下文质量 | FR-11 | §10 |
| P1 | Prompt 方言小抄 + 异步加载 | FR-12 | §11 |
| P1 | Egress 常驻提示 + 连接清单收敛 | FR-13 | §7 |
| P2 | Provider 能力矩阵 / 超时重试 / 用量观测 / 反馈闭环 / Workflow 复用 / 组件复用 | FR-14~FR-20 | §12 |

> 状态：Draft
