# 前端形态拆分实施工作日志（form-split worklog）

> 本文件随每个 form-split PR 同步提交，记录实际执行顺序、决策与验证结果，
> 避免工作进度丢失。条目按时间倒序（最新在上）。基线：`2a08e55`（F4a 合入后）。

## 2026-10-29 — F4b + F4c 重核 + F5 合入（74d2eef → 59dea69）

### F4b（74d2eefe8）
- 命名归零完成：`get_table_data_at_target`→`get_table_data`、`read_artifact_chunk`→`read_artifact`、
  `platform_list/create/update/disable_profile`→`list/create/update/disable_connection`；
  **`commandOverrides` 表删除**（toCommandName 纯约定，kernel 轨与命令名 1:1，裁决 #2 第 4 步达成）。
- Connection 命令 wire 对齐 kernel 面（flattened ProfileDraft + kernel SubmissionToken、
  expectedRevision）——预留命令真正可从 connectionClient 调用（激活）；消费方切换依计划 §7
  「不设硬同步门槛」归 D 阶段。
- 导出双流程示范（§3.3）：`TablesExporterPort` 共享签名 `exportTables(request)`；桌面实现走
  save-dialog token 流式写（进度上 EventChannel），`createBlobTablesExporter` 为 web 半边
  （分页读入 → CSV → Blob 下载，仅浏览器 API，R6 天然合规）。
- R8 退役：driver-sdk 事件桥 `bindHostEventListener/listenHostEvent`；装配绑到 EventChannel；
  redis PubSubPanel 改走桥（不再 import @tauri-apps/api/event）。
- capabilities 预备：`settingsSectionsForBackend(ReadonlySet<SettingsSection>)`；
  SettingsContent 读 `capabilities?.settingsSections`——**src/ 内 `backendId ===` 代码级归零**。

### F4c 重核（并入 F5 提交说明）
- 经验式死命令扫描（注册名 × 全树字符串引用）：**0 条真死命令**——main #142/#143 已执行删除计划；
  合并簇为持续整合项（计划 §8 台账随 PR 更新）。F4 退出标准全项达成。

### F5（59dea6941）
- `src/platform/capabilities.ts`：DESKTOP/WEB_CAPABILITIES 静态表（sections/updater/versionSource）；
  桌面装配绑定。UpdateSection 消费 `capabilities.updater`、版本读消费 `versionSource`。
- 每域契约表 `domainContractTables.test.ts`（32 例：方法名 × 请求封套 × 响应直通 + 缺口两分类）。
- 端口契约 `platformContracts.test.ts`（11 例：§7.4 未绑定显式错误 + 绑定后行为；HostSettings
  惰性默认例外）。
- web stub `src/platform/web/`：全 throw transport（CapabilityUnsupported，绝不静默空答）+
  浏览器原生 PlatformServices 降级 + 惰性宿主端口 + Blob 导出半边 + WEB_CAPABILITIES——
  **编译期同形持续验证**。
- playbook：`docs/development/platform-extension-playbook.md`（决策树 + F3 复刻配方 + 测试标准）。
- 验证：typecheck 绿；闸门 0 blocking / 41 advisory（R8 清零）；418 测试绿。

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
