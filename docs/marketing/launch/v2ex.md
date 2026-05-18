# V2EX 发布稿（分享创造）

**节点**: 分享创造  
**标题建议**: [开源] DataZen — 10MB 以下的跨平台数据库客户端（PG/MySQL/Redis）

---

厌倦了 TablePlus 订阅？可以试试 **DataZen**。

DataZen 是 MIT 协议的开源桌面数据库工具，基于 **Tauri + Rust**，安装包不到 10MB，启动快、占内存少。

**支持数据库**: PostgreSQL、MySQL / MariaDB、SQLite、Redis

**主要功能**:
- 多窗口，连接与查询互不干扰
- 内置 **SSH 隧道**（纯 Rust，无需本机 ssh）
- SQL 编辑器：语法高亮、表名/列名自动补全
- 数据浏览：虚拟滚动、筛选排序、行内编辑
- 备份、导入导出（CSV/JSON/SQL）
- **PG ↔ MySQL** 表结构对比与数据同步
- 中英文界面、暗色主题
- 连接密码本地 AES 加密，无云端账号

**下载**: https://github.com/flyxl/datazen/releases  
**仓库**: https://github.com/flyxl/datazen  
**官网**: https://flyxl.github.io/datazen/

**macOS 提示**: 首次打开若提示「已损坏」，安装后执行:
```bash
xattr -cr /Applications/DataZen.app
```

欢迎 Star 和 Issue，反馈: wuxiaolongklws@gmail.com

---

## 发布检查

- [ ] 附图: `docs/screenshots/connection-window.png` 或 `demo.gif`
- [ ] 发布后 24h 回复评论
