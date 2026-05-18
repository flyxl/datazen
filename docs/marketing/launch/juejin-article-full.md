# 用 Tauri + Rust 做桌面数据库客户端：DataZen 架构与实践

> 发布到掘金时：封面用 `docs/marketing/assets/og-image.png`，分类选「后端 / 开源」，标签：Rust、Tauri、数据库、开源。

## 前言

日常开发里总要连 PostgreSQL、MySQL、偶尔还有 Redis。商业客户端好用但要订阅，DBeaver 功能全但偏重。于是我做了 **DataZen** —— MIT 开源、安装包不到 10MB 的跨平台桌面客户端。

- 仓库：https://github.com/flyxl/datazen  
- 下载：https://github.com/flyxl/datazen/releases  
- 官网：https://flyxl.github.io/datazen/

## 技术选型

| 层级 | 技术 | 理由 |
|------|------|------|
| 桌面壳 | Tauri v2 | 小包体，Rust 后端 |
| 前端 | React + Zustand + CodeMirror 6 | 生态成熟，SQL 补全 |
| 大表 | @tanstack/react-virtual | 十万行级滚动 |
| 驱动 | sqlx + redis crate | 异步、连接池 |
| SSH | russh | 纯 Rust，无系统 ssh 依赖 |
| 安全 | AES-256-GCM | 连接密码本地加密 |

## 架构要点

后端采用 **注册表驱动** 的 `DatabaseDriver` trait：新增一种数据库只需实现 trait 并在 `init_drivers()` 注册，前端在 `DB_REGISTRY` 增加元数据即可。详细设计见仓库内 `docs/backend-architecture.md`。

数据流：`React → Tauri IPC → Rust services → 数据库驱动`，结果经事件回推前端。

## 功能亮点

1. **多窗口**：每个连接/查询独立窗口，互不干扰。  
2. **内置 SSH 隧道**：通过跳板机连内网库。  
3. **SQL 编辑器**：表名、列名自动补全，多语句执行。  
4. **PG ↔ MySQL 同步**：表结构对比 + 数据同步，支持断点续传。  
5. **Redis 专视图**：Database 列表 + Key 浏览器。  
6. **中英双语** + 暗色主题。

## macOS 安装提示

应用未经 Apple 公证。若提示「已损坏」：

```bash
xattr -cr /Applications/DataZen.app
```

## 现状与邀请

当前 **v0.0.3**，核心路径已可用于日常开发。欢迎 Star、Issue 和 PR。  
反馈：wuxiaolongklws@gmail.com
