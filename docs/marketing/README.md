# DataZen 市场推广素材

本目录包含市场推广计划的可交付物：发布文案、视觉素材、演示视频脚本与指标周报模板。

## 目录结构

| 路径 | 说明 |
|------|------|
| [`launch/`](launch/) | 各渠道可直接复制发布的帖子（中/英） |
| [`assets/`](assets/) | OG 图、Product Hunt 图（SVG + PNG） |
| [`video/`](video/) | 60 秒演示视频脚本与录制说明 |
| [`metrics/`](metrics/) | KPI 基线与周报模板 |
| [`scripts/`](scripts/) | 素材导出、GIF 更新脚本 |

## 快速链接

- **下载**: https://github.com/flyxl/datazen/releases
- **官网**: https://flyxl.github.io/datazen/
- **联系**: wuxiaolongklws@gmail.com

## 发布前检查清单

- [ ] README 与落地页链接可访问
- [ ] macOS `xattr` 说明已附在帖子中
- [ ] Product Hunt 五张图 + 60s 视频已上传
- [ ] 发布后 48h 内回复 Issue / 邮件

## 重新生成 PNG 素材

```bash
./docs/marketing/scripts/export-assets.sh
```

## 更新演示 GIF

```bash
./docs/marketing/scripts/update-demo-gif.sh
```
