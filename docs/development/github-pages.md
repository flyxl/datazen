# GitHub Pages 启用与验证指南

落地页源文件: [`site/index.html`](../../site/index.html)（英文根目录；中文见 [`site/zh/`](../../site/zh/)）  
部署工作流: [`.github/workflows/pages.yml`](../../.github/workflows/pages.yml)  
上线地址: **https://flyxl.github.io/datazen/**

## 第一步：合并并推送到 main

Pages 工作流在 `main` 分支上运行。确保以下文件已提交并推送:

- `site/index.html` 及 `site/*.html`（英文 7 页）
- `site/zh/index.html` 及 `site/zh/*.html`（中文 7 页）
- `site/assets/`（CSS、JS、`logo.png`、`screenshots/`）
- `site/robots.txt`、`site/sitemap.xml`
- `site/.nojekyll`
- `.github/workflows/pages.yml`

```bash
git add site/ .github/workflows/pages.yml
git commit -m "Add bilingual marketing site and GitHub Pages deploy"
git push origin main
```

## 第二步：在 GitHub 启用 Pages

### 仓库可见性

GitHub Pages 对 **私有免费库不可用**。若仓库为 private，需改为 **Public** 或升级 GitHub Pro:

```bash
gh repo edit flyxl/datazen --visibility public --accept-visibility-change-consequences
gh api -X POST repos/flyxl/datazen/pages -f build_type=workflow
```

也可在 Settings → General → Danger zone 中修改。

启用后:

1. 打开 https://github.com/flyxl/datazen/settings/pages  
2. **Build and deployment** → **Source**: `GitHub Actions`  
3. 重新运行 Actions 中的 **Deploy GitHub Pages**（或再 push 一次 `site/`）

### 用 CLI 启用（公开库或 Pro 账户）

```bash
gh api -X POST repos/flyxl/datazen/pages -f build_type=workflow
```

私有免费库会返回 422，需先按上表方案 A 或 B 处理。

## 第三步：确认部署成功

1. **Actions**: https://github.com/flyxl/datazen/actions → “Deploy GitHub Pages” 为绿色  
2. **Settings → Pages**: 显示  
   `Your site is live at https://flyxl.github.io/datazen/`  
3. 浏览器打开该 URL，检查:
   - [ ] 英文首页标题与 Download / GitHub / Contact 按钮；导航显示 `中` 语言切换
   - [ ] https://flyxl.github.io/datazen/zh/ 中文首页；导航显示 `EN`
   - [ ] 各子页（features、ai、charts 等）语言切换可往返
   - [ ] 截图与 macOS `xattr` 提示块
   - [ ] View Source 可见 canonical / hreflang（非 JS 注入）

本地结构校验（可选）:

```bash
node scripts/check-site-seo.mjs
```

## 本地预览（推送前）

在仓库根目录:

```bash
cd site && python3 -m http.server 8765
```

访问:

- http://127.0.0.1:8765/index.html — 英文；导航 `中`
- http://127.0.0.1:8765/zh/index.html — 中文；导航 `EN`

（本地无 `/datazen` 前缀；相对链接与 `/zh/` 路径检测仍可用。）

## 常见问题

| 问题 | 处理 |
|------|------|
| 404 | 确认 `site/index.html` 存在且工作流成功；等待 1–5 分钟 CDN |
| 图片 404 | 确认 `site/assets/logo.png` 与 `site/assets/screenshots/` 已提交 |
| Actions 无权限 | Settings → Actions → General → Workflow permissions 选 **Read and write** |
| OG 预览不对 | 社交缓存可测 https://www.opengraph.xyz/ |

## README 链接

README 中官网链接指向 `https://flyxl.github.io/datazen/`，Pages 上线后即可点击验证。
