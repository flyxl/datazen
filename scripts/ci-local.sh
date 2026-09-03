#!/usr/bin/env bash
# ci-local.sh — 本地运行与 .github/workflows/ci.yml 等价的 CI 检查（可在 macOS / Linux 直接执行）。
#
# 用法:  bash scripts/ci-local.sh
# 或  :  pnpm ci:local
#
# 逐条镜像 ci.yml 的 jobs.test 步骤（跳过仅 CI runner 需要的 Linux 系统依赖安装，
# 以及 needs ubuntu 的步骤）。任一步失败立即以非零码退出（set -euo pipefail）。
#
# 可选环境变量:
#   CI_LOCAL_SKIP_CARGO=1   跳过全部 cargo 测试（仅跑前端 / 守护检查，更快）
#   CI_LOCAL_SITE=1         强制运行 site SEO 检查（默认仅当 site/ 有改动时运行）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"
cd "$ROOT"

step() {
  printf '\n\033[1;36m==== [ci-local] %s ====\033[0m\n' "$1"
}

fail() {
  printf '\n\033[1;31m[ci-local] FAILED: %s\033[0m\n' "$1" >&2
  exit 1
}

# --------------------------------------------------------------------------
step "1/10 TypeScript typecheck (pnpm typecheck)"
pnpm typecheck || fail "typecheck"

# --------------------------------------------------------------------------
step "2/10 Guard: tracked driver-managed files are not injected"
node scripts/check-managed-stubs.mjs || fail "check-managed-stubs"

# --------------------------------------------------------------------------
step "3/10 Guard: Host structure caps registry absent"
node scripts/check-structure-editor-guardrails.mjs || fail "check-structure-editor-guardrails"

# --------------------------------------------------------------------------
step "4/10 Frontend unit tests (pnpm test:unit; pretest/posttest 自动 codegen+restore)"
pnpm test:unit || fail "test:unit"

# --------------------------------------------------------------------------
# 5/10 Site SEO/i18n 检查（仅当 site/ 有改动，与 CI 一致；CI_LOCAL_SITE=1 时强制）
if [ "${CI_LOCAL_SITE:-0}" = "1" ]; then
  SITE_CHANGED=1
else
  BASE="$(git merge-base HEAD origin/main 2>/dev/null || git rev-parse HEAD~1 2>/dev/null || echo '')"
  if [ -n "$BASE" ] && git diff --name-only "$BASE" HEAD 2>/dev/null | grep -q '^site/'; then
    SITE_CHANGED=1
  else
    SITE_CHANGED=0
  fi
fi
if [ "$SITE_CHANGED" = "1" ]; then
  step "5/10 Site SEO/i18n checks (check-site-seo.mjs)"
  node scripts/check-site-seo.mjs || fail "check-site-seo"
else
  step "5/10 Site SEO/i18n checks (site/ 无改动，跳过)"
fi

# --------------------------------------------------------------------------
if [ "${CI_LOCAL_SKIP_CARGO:-0}" = "1" ]; then
  step "6-10/10 cargo 测试已通过 CI_LOCAL_SKIP_CARGO=1 跳过"
  printf '\n\033[1;32m[ci-local] 完成（前端 + 守护检查通过，cargo 已跳过）\033[0m\n'
  exit 0
fi

# --------------------------------------------------------------------------
step "6/10 Resolve drivers (basic, 完整注入)"
node scripts/resolve-drivers.mjs --drivers=basic || fail "resolve-drivers(basic)"

FEATURES="$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")"
printf '  cargo features: %s\n' "$FEATURES"

# --------------------------------------------------------------------------
step "7/10 Rust unit tests (datazen-driver-api --lib)"
cargo test -p datazen-driver-api --lib || fail "cargo test driver-api"

# --------------------------------------------------------------------------
step "8/10 Rust unit tests (datazen --lib --features $FEATURES)"
cargo test -p datazen --lib --features "$FEATURES" || fail "cargo test datazen"

# --------------------------------------------------------------------------
step "9/10 Rust unit tests (basic path drivers)"
cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --lib || fail "cargo test basic drivers"

# --------------------------------------------------------------------------
step "9.5/10 Restore managed files (driver-file-stash restore)"
node scripts/driver-file-stash.mjs restore || fail "driver-file-stash restore"

# --------------------------------------------------------------------------
step "10/10 Rust unit tests (datazen-ai-api --lib)"
cargo test -p datazen-ai-api --lib || fail "cargo test ai-api"

printf '\n\033[1;32m[ci-local] 全部 CI 检查通过 ✔\033[0m\n'
