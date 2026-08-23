# CI：私有插件仓库（Deploy Key + Environment）

**Akulaku** 等含 git 驱动的发布变体会通过 `resolve-drivers.mjs` clone `datazen-driver-kiwi` / `datazen-driver-superset`。仓库为 private 时，Release workflow 使用 **只读 Deploy Key（SSH）** 拉取；凭据放在 GitHub Environment **`release`**，而不是仓库级 Secrets。

日常 PR CI（`.github/workflows/ci.yml`）与 **Basic / All** 发布变体**不**拉取 git 驱动，也**不**挂这些密钥。`all` 预设仅为 path 原生驱动。

## 1. 生成两把 Deploy Key

同一公钥不能挂到多个仓库，需各生成一对：

```bash
ssh-keygen -t ed25519 -C "datazen-ci-kiwi" -f ./datazen-ci-kiwi -N ""
ssh-keygen -t ed25519 -C "datazen-ci-superset" -f ./datazen-ci-superset -N ""
```

- 公钥：`datazen-ci-kiwi.pub` → kiwi 仓库  
  Settings → Deploy keys → Add deploy key → **只勾选 Read**（不要 Allow write）
- 公钥：`datazen-ci-superset.pub` → superset 仓库（同上）
- 私钥：`datazen-ci-kiwi` / `datazen-ci-superset` → 下一步写入 Environment Secrets（**不要**提交进 git）

本地用完后删除磁盘上的私钥副本。

## 2. 创建 Environment `release`

在 **datazen** 仓库：

1. Settings → Environments → **New environment** → 名称：`release`
2. **Deployment protection rules**（可选）
   - 当前未启用 Required reviewers（发版自动注入 Secrets，无需每次 Approve）
   - 可选：Deployment branches → 限制为 tags（如 `v*`）或受保护分支
3. **Environment secrets**（在该 Environment 下添加，不要放在 Repository secrets）：

| Name | Value |
|------|--------|
| `KIWI_DEPLOY_KEY` | `datazen-ci-kiwi` 私钥全文（含 `BEGIN/END` 行） |
| `SUPERSET_DEPLOY_KEY` | `datazen-ci-superset` 私钥全文 |
| `WINDOWS_CERTIFICATE` | （若已有）从原 Repository secret 迁过来 |
| `WINDOWS_CERTIFICATE_PASSWORD` | （若已有）一并迁入 |

迁完后可删除仓库级同名 Secrets，避免双份、范围过大。

## 3. Workflow 行为摘要

`.github/workflows/release.yml`：

- `environment: release` — 从此 Environment 读取上述 Secrets（无审批门禁时自动注入）
- `permissions: {}` 顶层 + job 内 `contents: write`（仅用于 draft Release）
- `matrix.needs_git == true` 时（当前为 **Akulaku**）：把两把私钥写成 `~/.ssh/datazen_{kiwi,superset}`，用 **Host 别名 + IdentitiesOnly** 做 per-repo 选钥，再用  
  `url."git@github.com-<id>:flyxl/datazen-driver-<id>.git".insteadOf`  
  把 `drivers-registry.json` 里的 HTTPS（及同名 SSH）URL 改写到对应别名。  
  （不要用「单条 `git@github.com:` insteadOf」+ 多把 Deploy Key：GitHub 只认第一把已知钥，另一仓会报 `Repository not found`。）
- Akulaku 驱动列表在 CI 中写死为显式逗号串（`postgres,mysql,sqlite,redis,mongodb,kiwi,superset`），**不是** `resolve-drivers` 的命名预设

公钥出现在文档或插件仓 Deploy keys 页面**不会**让外人 clone 私有仓；只有私钥可以。

可选：若希望继续用 `webfactory/ssh-agent` 的自动映射，生成密钥时把 comment 设为仓库 URL（`-C "git@github.com:flyxl/datazen-driver-kiwi.git"`）；当前 workflow **不依赖**该 comment。

## 4. 本地含 git 驱动的构建

Deploy Key 仅供 CI。本地需要：

- 被加为 kiwi / superset 私有仓 collaborator，或
- 使用自己的 SSH key / HTTPS 凭据

```bash
# 与 Akulaku SKU 对齐的显式列表（勿新增与 basic/all 同级的预设名）
pnpm tauri:build --drivers=postgres,mysql,sqlite,redis,mongodb,kiwi,superset
# 或
DATAZEN_DRIVERS=postgres,mysql,sqlite,redis,mongodb,kiwi,superset pnpm tauri:build
```

## 5. 仓库侧建议（可选但推荐）

- `main` 开启 branch protection + PR review
- `CODEOWNERS` 要求 `.github/workflows/` 由维护者审
- Settings → Actions → Fork PR workflows：**Require approval for first-time contributors**
- Settings → Actions → Workflow permissions：默认 **Read repository contents**
