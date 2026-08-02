# 测试工具配置：zavora-ai/computer-use-mcp

## 概述

[zavora-ai/computer-use-mcp](https://github.com/zavora-ai/computer-use-mcp) 是一个基于 MCP（Model Context Protocol）的高性能桌面自动化服务器，使用 Rust NAPI 构建，提供截图、鼠标、键盘、剪贴板和应用管理功能。支持 macOS、Windows 和 Linux。

---

## 安装（无需本地安装）

computer-use-mcp 通过 `npx` 直接运行，无需预先安装：

```bash
npx --yes --prefer-offline @zavora-ai/computer-use-mcp
```

---

## 在 Cursor 中配置

### 项目级配置（推荐）

已在项目根目录创建 `.cursor/mcp.json`：

```json
{
  "mcpServers": {
    "computer-use": {
      "command": "npx",
      "args": ["--yes", "--prefer-offline", "@zavora-ai/computer-use-mcp"]
    }
  }
}
```

### 生效方式

1. 确保 `.cursor/mcp.json` 文件存在（已创建）
2. 在 Cursor 中执行 `Cmd+Shift+P` → "Reload Window" 重载窗口
3. 等待 MCP 服务器自动启动
4. 通过 `GetMcpTools` 验证服务器状态

---

## macOS 权限设置（必需）

macOS 需要 **辅助功能** 权限才能控制桌面：

1. 打开 **系统设置** → **隐私与安全性** → **辅助功能**
2. 点击 **+** 按钮
3. 添加 **Cursor.app**（路径：`/Applications/Cursor.app`）
4. 确保 Cursor 的开关为 **开启**（蓝色）
5. 可能需要重启 Cursor

### 验证权限

```bash
npx @zavora-ai/computer-use-mcp demo
```

如果权限正确，将自动打开计算器、计算 42+58、然后关闭。

---

## 可用工具（共 64 个）

### 核心工具

| 工具名 | 功能 | 用途 |
|--------|------|------|
| `screenshot` | 截取屏幕/窗口截图 | 视觉验证、页面状态检查 |
| `click` | 鼠标单击 | 按钮点击、菜单选择 |
| `double_click` | 鼠标双击 | 打开连接、编辑单元格 |
| `right_click` | 鼠标右键 | 打开上下文菜单 |
| `type` | 键盘输入 | 填写表单、输入 SQL |
| `key` | 按键/快捷键 | Cmd+N、Cmd+Enter、Escape 等 |
| `drag` | 鼠标拖拽 | 拖拽连接到分组、调整侧边栏 |
| `scroll` | 滚轮滚动 | 浏览列表、滚动表数据 |
| `move` | 移动鼠标 | 悬停操作 |
| `get_screen_size` | 获取屏幕尺寸 | 坐标计算基础 |

### 高级工具

| 工具名 | 功能 | 用途 |
|--------|------|------|
| `snapshot` | 截图 + UI 树 + 窗口信息 | 综合页面分析 |
| `zoom` | 区域放大截图 | 检查小元素 |
| `annotate` | 截图标注 | 标记 Bug 位置 |
| `grid` | 网格参考线 | 精确坐标定位 |
| `list_apps` | 列出运行中应用 | 查找 DataZen 窗口 |
| `list_windows` | 列出窗口信息 | 获取窗口 ID 和位置 |
| `focus_app` | 聚焦应用 | 切换到 DataZen 窗口 |
| `clipboard_read/write` | 剪贴板操作 | 验证复制功能 |
| `run_script` | 执行脚本 | AppleScript 操作 |
| `doctor` | 环境检查 | 验证权限和兼容性 |

### 窗口定位

支持通过 `target_app`（Bundle ID）或 `target_window_id` 定位特定窗口：

```
target_app: "com.datazen.app"  # DataZen 的 Bundle ID（需确认）
focus_strategy: "strict" | "best_effort" | "none"
```

---

## DataZen 测试流程

### 1. 启动被测应用

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen
pnpm tauri:dev --plugins=kiwi
```

### 2. 确认 MCP Server 可用

```
# 在 Cursor Agent 中
GetMcpTools({ pattern: "computer" })
```

### 3. 初始化测试

```
# 运行环境检查
CallMcpTool("computer-use", "doctor", {})

# 获取屏幕信息
CallMcpTool("computer-use", "get_screen_size", {})

# 找到 DataZen 窗口
CallMcpTool("computer-use", "list_windows", {})

# 截取初始状态
CallMcpTool("computer-use", "screenshot", { width: 1024 })
```

### 4. 执行测试用例

```
# 示例：测试 TC-CONN-001（新建 PostgreSQL 连接）

# 1. 点击「新建连接」按钮
CallMcpTool("computer-use", "click", { x: 675, y: 119 })

# 2. 等待窗口打开
sleep(1000)

# 3. 截图验证
CallMcpTool("computer-use", "screenshot", { width: 1024 })

# 4. 填写连接名称
CallMcpTool("computer-use", "click", { x: 400, y: 75 })
CallMcpTool("computer-use", "type", { text: "Test PG Connection" })

# 5. 点击测试连接
CallMcpTool("computer-use", "click", { x: 200, y: 500 })

# 6. 截图记录结果
CallMcpTool("computer-use", "screenshot", { width: 1024 })
```

### 5. Bug 报告

发现异常时：
1. 立即截图保存到 `test/screenshots/`
2. 记录当前环境配置
3. 记录重现步骤（包含坐标和操作）
4. 写入 `test/bug-list.md`

---

## 注意事项

1. **Retina 显示器**：macOS Retina 显示器有 2x DPI 缩放，`screenshot` 默认返回逻辑分辨率坐标
2. **窗口位置**：确保 DataZen 窗口在屏幕可见区域内
3. **多显示器**：如有多个显示器，需确认截图覆盖正确的屏幕
4. **等待时间**：操作间需适当等待 UI 渲染完成（建议 300-1000ms）
5. **焦点管理**：Tauri 多窗口应用需注意窗口焦点切换
6. **截图目录**：所有测试截图保存到 `test/screenshots/`

---

## 当前状态

| 项目 | 状态 |
|------|------|
| npm 包 | ✅ 已下载可用 (`@zavora-ai/computer-use-mcp`) |
| Cursor 配置 | ✅ `.cursor/mcp.json` 已创建 |
| 权限 | ⚠️ 需要手动授予 Cursor 辅助功能权限 |
| MCP 连接 | ⚠️ 需要重载 Cursor 窗口后生效 |
