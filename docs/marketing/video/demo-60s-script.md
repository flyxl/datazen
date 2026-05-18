# DataZen 60 秒演示视频脚本

用于 Product Hunt、Twitter/X、B 站/YouTube。建议分辨率 1920x1080 或 1280x720，暗色主题录制。

## 时间轴

| 时间 | 画面 | 旁白（英） | 旁白（中） |
|------|------|------------|------------|
| 0:00-0:05 | Logo + 标题卡 | Meet DataZen — a free, lightweight database client. | DataZen：轻量、免费的数据库客户端。 |
| 0:05-0:12 | 主窗口连接列表 | Save all your connections in one place. Group by project or environment. | 连接集中管理，按项目或环境分组。 |
| 0:12-0:20 | 新建连接 + 测试连接 | Connect to PostgreSQL, MySQL, SQLite, or Redis in seconds. | 几秒连上 PG、MySQL、SQLite 或 Redis。 |
| 0:20-0:30 | 表数据浏览 + 筛选 | Browse millions of rows smoothly with virtual scrolling. Filter and sort inline. | 虚拟滚动流畅浏览大表，筛选排序。 |
| 0:30-0:40 | SQL 编辑器 + 补全 | Write SQL with autocomplete for tables and columns. Run multiple statements. | SQL 自动补全，多语句执行。 |
| 0:40-0:48 | SSH 设置（可选） | Reach private databases through built-in SSH — no local ssh client needed. | 内置 SSH 隧道，无需本机 ssh。 |
| 0:48-0:55 | Redis 视图 | Explore Redis keys with a dedicated browser. | Redis 专用 Key 浏览器。 |
| 0:55-1:00 | 结束卡：GitHub + 下载 | Download free on GitHub. MIT licensed. | GitHub 免费下载，MIT 开源。 |

## 结束卡文字

```
DataZen
github.com/flyxl/datazen
wuxiaolongklws@gmail.com
```

## 录制提示

1. 使用干净测试数据，避免真实生产库名/IP。
2. macOS 若录屏包含「无法打开」提示，先执行 `xattr -cr /Applications/DataZen.app`。
3. 导出 MP4（H.264），目标时长 55-65 秒。

## 从现有 GIF 生成占位视频（无旁白）

```bash
ffmpeg -y -i docs/screenshots/demo.gif -movflags faststart -pix_fmt yuv420p docs/marketing/video/demo-60s-placeholder.mp4
```

正式发布前请用屏幕录制替换占位文件。
