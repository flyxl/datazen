# 掘金技术文章

**标题建议**: 用 Tauri + Rust 做桌面数据库客户端：DataZen 架构与实践

**标签**: Rust, Tauri, 数据库, 开源

---

## 正文大纲

### 1. 为什么做 DataZen

- TablePlus 订阅成本、DBeaver 体积与启动速度
- 目标：&lt;10MB、MIT 开源、日常开发够用

### 2. 技术选型

| 层 | 选型 | 原因 |
|----|------|------|
| 桌面 | Tauri v2 | 小包体、Rust 后端 |
| 前端 | React + Zustand + CodeMirror 6 | 生态成熟 |
| 驱动 | sqlx + redis + russh | 异步、无系统 ssh 依赖 |

详细架构见仓库内文档: [backend-architecture.md](https://github.com/flyxl/datazen/blob/main/docs/backend-architecture.md)

### 3. 多窗口与驱动注册表

简述 `DatabaseDriver` trait 与 `DB_REGISTRY`，新增数据库类型只需少量文件（README 有步骤）。

### 4. 难点摘录

- 虚拟滚动大表（@tanstack/react-virtual）
- SSH 隧道与连接池生命周期
- PG ↔ MySQL 同步与断点续传

### 5. 当前状态与路线图

- 版本 v0.0.3，核心场景可用
- 欢迎 Issue 和 PR

### 6. 试用

- 下载: https://github.com/flyxl/datazen/releases
- 官网: https://flyxl.github.io/datazen/
- 联系: wuxiaolongklws@gmail.com

---

## 文末 CTA（可直接粘贴）

> **DataZen** 是 MIT 开源的跨平台数据库客户端，支持 PostgreSQL、MySQL、SQLite、Redis，内置 SSH 与 SQL 自动补全。  
> GitHub: https://github.com/flyxl/datazen  
> 欢迎 Star。

---

## 发布检查

- [ ] 封面图: `docs/marketing/assets/og-image.png`
- [ ] 文内 2-3 张截图
- [ ] 链接可点击
