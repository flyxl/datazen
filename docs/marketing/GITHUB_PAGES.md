# GitHub Pages 启用与验证指南

落地页源文件: [`docs/index.html`](../index.html)  
部署工作流: [`.github/workflows/pages.yml`](../../.github/workflows/pages.yml)  
上线地址: **https://flyxl.github.io/datazen/**

## 第一步：合并并推送到 main

Pages 工作流在 `main` 分支上运行。确保以下文件已提交并推送:

- `docs/index.html`
- `docs/.nojekyll`
- `docs/screenshots/`（含 `demo.gif`）
- `docs/marketing/assets/`（含 `og-image.png`、`logo.png`）
- `.github/workflows/pages.yml`

```bash
git add README.md docs/ .github/ISSUE_TEMPLATE/ .github/workflows/pages.yml
git commit -m "Add marketing landing page and GitHub Pages deploy"
git push origin main
```

## 第二步：在 GitHub 启用 Pages

1. 打开 https://github.com/flyxl/datazen/settings/pages  
2. **Build and deployment**
   - **Source**: `GitHub Actions`（不要选 “Deploy from a branch”）
3. 推送后 Actions 会自动运行 **Deploy GitHub Pages** 工作流  
4. 若首次使用 Pages，可能需在 Settings → Pages 点击保存以激活

### 用 CLI 启用（可选）

```bash
gh api -X POST repos/flyxl/datazen/pages \
  -f build_type=workflow
```

## 第三步：确认部署成功

1. **Actions**: https://github.com/flyxl/datazen/actions → “Deploy GitHub Pages” 为绿色  
2. **Settings → Pages**: 显示  
   `Your site is live at https://flyxl.github.io/datazen/`  
3. 浏览器打开该 URL，检查:
   - [ ] 标题与 Download / GitHub / Contact 按钮
   - [ ] 演示 GIF 加载
   - [ ] 三张截图
   - [ ] macOS `xattr` 提示块

## 本地预览（推送前）

在仓库根目录:

```bash
cd docs && python3 -m http.server 8765
```

访问 http://127.0.0.1:8765/index.html  
（路径与线上一致，根目录为 `docs/`。）

## 常见问题

| 问题 | 处理 |
|------|------|
| 404 | 确认 `docs/index.html` 存在且工作流成功；等待 1–5 分钟 CDN |
| 图片 404 | 确认 `docs/marketing/assets/logo.png` 与 `og-image.png` 已提交 |
| Actions 无权限 | Settings → Actions → General → Workflow permissions 选 **Read and write** |
| OG 预览不对 | 社交缓存可测 https://www.opengraph.xyz/ |

## README 链接

README 中官网链接指向 `https://flyxl.github.io/datazen/`，Pages 上线后即可点击验证。
