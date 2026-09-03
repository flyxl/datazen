#!/usr/bin/env bash
# new-feature-worktree.sh — 创建并行开发轨道（子代理开发 Playbook 配套脚本）
# 用法: scripts/new-feature-worktree.sh <track> [base-branch]
#   <track>       轨道名，worktree 为主检出下的 .worktrees/datazen-<track>，分支为 feature/<track>
#   [base-branch] 基分支，默认 feature/ipc-refactor
#
# 自动完成:
#   1. git worktree + 新分支
#   2. node_modules 软链到主检出（禁 pnpm install 的前提）
#   3. resolve-drivers --codegen-only --drivers=basic（generated*.ts / driver_init.rs）
#   4. 拷贝主检出 docs/development 下未跟踪的规格文档（worktree 里不存在且不可提交）
#   5. 拷贝 e2e/.env（如存在）
set -euo pipefail

TRACK=${1:?usage: new-feature-worktree.sh <track> [base-branch]}
BASE=${2:-main}

MAIN="$(git rev-parse --show-toplevel)"
WT="${MAIN}/.worktrees/datazen-${TRACK}"
BRANCH="feature/${TRACK}"

# 代理沙箱通常只允许写入主检出及其子目录。将并行 worktree 放在
# .worktrees 下，避免“同级 worktree 可读但不可写”的权限差异。
mkdir -p "${MAIN}/.worktrees"

if git -C "$MAIN" worktree list --porcelain | grep -q "^worktree ${WT}$"; then
  echo "✋ worktree 已存在: ${WT}" >&2
  exit 1
fi

echo "▶ 创建 worktree ${WT}（分支 ${BRANCH}，基于 ${BASE}）"
if git -C "$MAIN" show-ref --verify --quiet "refs/heads/${BRANCH}"; then
  git -C "$MAIN" worktree add "$WT" "$BRANCH"
else
  git -C "$MAIN" worktree add -b "$BRANCH" "$WT" "$BASE"
fi

echo "▶ node_modules 软链"
ln -s "${MAIN}/node_modules" "${WT}/node_modules"

echo "▶ 驱动 codegen（basic）"
( cd "$WT" && node scripts/resolve-drivers.mjs --codegen-only --drivers=basic )

echo "▶ 内置 locale codegen"
( cd "$WT" && node scripts/generate-builtin-locales.mjs )

echo "▶ 拷贝主检出未跟踪的规格文档（保持未跟踪，禁止 git add）"
while IFS= read -r f; do
  rel="${f#${MAIN}/}"
  rel="${rel%/}"
  src="${MAIN}/${rel}"
  dest="${WT}/${rel}"
  if [ -d "$src" ]; then
    mkdir -p "$(dirname "$dest")"
    cp -R "$src" "$(dirname "$dest")/"
    echo "   + ${rel}/（untracked dir）"
  elif [ -f "$src" ]; then
    mkdir -p "$(dirname "$dest")"
    cp "$src" "$dest"
    echo "   + ${rel}（untracked）"
  fi
done < <(git -C "$MAIN" status --porcelain docs/development | awk '$1=="??"{print $2}')

if [ -f "${MAIN}/e2e/.env" ]; then
  cp "${MAIN}/e2e/.env" "${WT}/e2e/.env"
  echo "▶ e2e/.env 已拷贝"
fi

echo "▶ coordination tracks 目录准备（方案 B：各轨独立维护 tracks/<track>/，禁止修改或软链 hub.md）"
COORD_DIR="${WT}/docs/development/coordination"
mkdir -p "${COORD_DIR}/tracks/${TRACK}"

cat <<EOF

✅ 轨道就绪: ${WT} @ ${BRANCH}
后续提醒:
  - 代理简报必须写明: 该工作目录 + 禁止修改其他检出
  - 简报环境注意三件套: Grep 工具搜索(禁 bash 全仓 grep) / CARGO_TARGET_DIR 策略 / 禁 add 未跟踪文档
  - 进度管理: 各轨独立维护 tracks/${TRACK}/progress.md，禁止修改或提交 hub.md
  - 活性与死亡恢复协议见 docs/development/subagent/README.md
EOF
