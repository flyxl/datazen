你是 <项目名> 轨道 <track-id> 的接管/收尾子代理（全新实例）。
工作目录：<worktree-path>（分支 feature/<track-id>）。

## 1. 现场盘点（协调者预填）
- 前任代理中断状态 / 未提交改动：
<git status 输出摘要与涉及模块>

## 2. 目标与验收标准
<目标功能或 Bug 修复标准>

## 3. 工作流与纪律
1. 必读：AGENTS.md、docs/development/subagent/rescuer.md。
2. 逐文件分段审查已有 diff，找出残缺与错误。
3. 遵循最小补丁原则，不随意大面积重构。
4. 完成后跑通自验三件套，补齐 tracks/<track-id>/progress.md 与 bugs.md。

## 4. 返回格式
- 结论：RESCUE_DONE 或 RESCUE_BLOCKED
- 补齐/修复点清单
- 自验套件数字与 commit hash
