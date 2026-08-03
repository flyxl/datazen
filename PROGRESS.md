# 开发进度跟踪

> 分支: `feat/logging-and-token-security`

## 功能列表

### 功能1: 修改 Kiwi 插件不记录鉴权 Token

**状态**: 🔄 开发中

**需求**: 
- 不在日志中打印鉴权相关的 token（SSO ticket, Admin-Token 等）
- 错误日志中不暴露完整的响应 body（可能包含 token）
- 保留必要的调试信息（状态码、长度等）

**涉及文件**:
- `.plugins/kiwi/src/lib.rs`

**需要修改的日志语句**:
| 行号 | 问题 | 修复方式 |
|------|------|----------|
| 194 | validate_ticket URL 包含 JWT ticket | 只记录路径，不记录查询参数 |
| 397 | GET 错误时记录完整 response body | 截断并脱敏 |
| 466 | POST 错误时记录完整 response body | 截断并脱敏 |
| 472-474 | 解析错误时记录 body 片段 | 移除 body 内容 |

**单元测试**: ✅ 3/3 通过  
**E2E 测试**: 🔄 进行中  

---

### 功能2: 添加日志记录功能

**状态**: ⬜ 待开发

**需求**:
- 在设置中配置日志路径和日志级别
- 在 Tools 菜单栏添加查看日志入口
- 支持日志持久化到文件

**涉及文件**:
- `src-tauri/src/store/mod.rs` — AppSettings 新增日志配置字段
- `src-tauri/src/lib.rs` — 日志初始化逻辑
- `src-tauri/src/commands/config.rs` — 日志配置 IPC
- `src/stores/settingsStore.ts` — 前端设置
- `src/windows/settings/SettingsWindow.tsx` — 设置界面
- 新增 Tools 菜单查看日志入口

**单元测试**: ⬜ 待编写  
**E2E 测试**: ⬜ 待执行  

---

## 测试记录

### 功能1 测试
- 单元测试: (待记录)
- E2E 测试: (待记录)

### 功能2 测试
- 单元测试: (待记录)
- E2E 测试: (待记录)
