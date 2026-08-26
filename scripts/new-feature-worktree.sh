#!/usr/bin/env bash
# new-feature-worktree.sh — 创建并行开发轨道（子代理开发 Playbook 配套脚本）
# 用法: scripts/new-feature-worktree.sh <track> [base-branch]
#   <track>       轨道名，worktree 为 ../datazen-<track>，分支为 feature/<track>
#   [base-branch] 基分支，默认 feature/ipc-refactor
#
# 自动完成:
#   1. git worktree + 新分支
#   2. node_modules 软链到主检出（禁 pnpm install 的前提）
#   3. resolve-drivers --codegen-only --drivers=basic（generated*.ts / plugin_init.rs）
#   4. 拷贝主检出 docs/development 下未跟踪的规格文档（worktree 里不存在且不可提交）
#   5. 拷贝 e2e/.env（如存在）
set -euo pipefail

TRACK=${1:?usage: new-feature-worktree.sh <track> [base-branch]}
BASE=${2:-feature/ipc-refactor}

MAIN="$(git rev-parse --show-toplevel)"
WT="$(dirname "$MAIN")/datazen-${TRACK}"
BRANCH="feature/${TRACK}"

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

echo "▶ 拷贝主检出未跟踪的规格文档（保持未跟踪，禁止 git add）"
while IFS= read -r f; do
  rel="${f#${MAIN}/}"
  mkdir -p "$(dirname "${WT}/${rel}")"
  cp "$f" "${WT}/${rel}"
  echo "   + ${rel}（untracked）"
done < <(git -C "$MAIN" status --porcelain docs/development | awk '$1=="??"{print $2}')

if [ -f "${MAIN}/e2e/.env" ]; then
  cp "${MAIN}/e2e/.env" "${WT}/e2e/.env"
  echo "▶ e2e/.env 已拷贝"
fi

cat <<EOF

✅ 轨道就绪: ${WT} @ ${BRANCH}
后续提醒:
  - 代理简报必须写明: 该工作目录 + 禁止修改其他检出
  - 简报环境注意三件套: Grep 工具搜索(禁 bash 全仓 grep) / CARGO_TARGET_DIR 策略 / 禁 add 未跟踪文档
  - 活性与死亡恢复协议见 docs/development/subagent-dev-playbook.md §3
EOF
