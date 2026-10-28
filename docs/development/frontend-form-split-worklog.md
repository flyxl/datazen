# 前端形态拆分实施工作日志（form-split worklog）

> 本文件随每个 form-split PR 同步提交，记录实际执行顺序、决策与验证结果，
> 避免工作进度丢失。条目按时间倒序（最新在上）。基线：`2a08e55`（F4a 合入后）。

---

## 2026-10-29 — F4b/F4c/F5/F6 续作会话

### 会话环境基线（续作前核对）

- 检出 `docs/frontend-form-split-plan` @ `2a08e5507`，工作区干净。
- 工具链：node v24.22.1；pnpm 12.16.1（全局安装）；**cargo 本机不可用**
  （src-tauri 侧改动只能静态核对 + 远程 CI 验证，与 F4a 同策略）。
- `pnpm install --frozen-lockfile` 完成；`scripts/typecheck.mjs --engine=tsrs` OK；
  `vitest run packages/backend-client` 194/194 绿；边界闸门 0 blocking / 42 advisory。
- IPC 注册命令实测：**227**（计划轨迹数字 267→255→~232 为旧基线估算，
  main 侧 IPC 合并已提前消化部分——F4c 需按当前面重新核定死命令清单）。

### 待办（按计划 §5.4–§5.6）

- [ ] F4b：Connection 域示范迁移 + `platform_*_profile` 改名 + override 表消减
- [ ] F4c：死命令删除 + 合并簇（按 227 现状重核）
- [ ] F4 项5：SettingsContent capabilities 预备、PubSubPanel 违规、导出双流程
- [ ] F5：DESKTOP_CAPABILITIES / 每域契约测试 / web stub / playbook
- [ ] F6：workspace 协议 + 包完整化 + 目录迁移 + R5 闸门

（进行中的条目将在各 PR 提交时补记到此处上方。）
