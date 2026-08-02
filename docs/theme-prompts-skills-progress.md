# Theme 适配 + 可配置 Prompt + Skill 创建 — 开发进度

> 跟踪 `feat/theme-prompts-skills` 分支上所有功能模块的开发和测试状态。

## 总体进度

| # | 功能模块 | 状态 | 单元测试 | E2E 测试 | 提交 |
|---|---------|------|---------|---------|------|
| 1 | FileConnectionFields light theme 适配 | ✅ 已完成 | — | ✅ 10/10 | 9b6922d |
| 2 | 可配置 AI Prompt（后端 + driver-api） | ✅ 已完成 | ✅ 8/8 | ✅ 10/11 | 3704e42 |
| 3 | 可配置 AI Prompt（前端设置 UI） | ✅ 已完成 | — | ✅ (含于#2) | 3704e42 |
| 4 | 新增 Skill 创建入口 + 存储路径说明 | ✅ 已完成 | ✅ 9/9 | ✅ 21/21 | 5a56147 |

## 状态说明

- 🔲 未开始
- 🔨 开发中
- ✅ 已完成
- ❌ 测试不通过
- 🐛 有已知 Bug

## 详细记录

### 功能 1: Light Theme 适配
将 `FileConnectionFields.tsx` 中硬编码的 `neutral-*` 深色主题类替换为语义化 CSS token 类。

### 功能 2: 可配置 AI Prompt
- `driver-api`: 新增 `PromptScenario` 枚举（9 种场景）和 `PromptTemplate` 结构
- `DatabaseDriver` trait: 新增 `prompt_overrides()` 方法
- `PromptResolver`: 3 级优先级解析（用户覆盖 > 驱动定制 > 默认）
- AI 命令: 全部从 `PromptBuilder` 迁移至 `PromptResolver`
- IPC: `prompt_list`, `prompt_set_override`, `prompt_remove_override`
- Settings UI: "Prompt 管理" 区域

### 功能 3: Skill 创建入口
- 新建/编辑/删除 Skill 的完整表单（ID、名称、描述、变量、步骤）
- 显示 skills 存储路径 (`{app_data_dir}/skills/`)
- 刷新按钮
- IPC: `skill_get_dir`, `skill_get`

---

*所有功能开发、测试完毕。*
