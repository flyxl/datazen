#!/usr/bin/env bash
# ==============================================================================
# DataZen 全量 100% 自动化测试执行脚本 (Run Full Automation Test Suite)
# 
# 覆盖范围:
#   1. 环境与凭据免密设置 (DATAZEN_KEYRING=file, e2e/.env AI 接入, 数据库 Seeding)
#   2. 静态代码守护与架构规范 (Typecheck, IDs, i18n sync, Managed stubs)
#   3. 前端与驱动 UI 单元测试 (Vitest & Typing Journey Tests)
#   4. Rust 后端安全治理与驱动契约测试 (Cargo test workspace)
#   5. Webdriver 自动化打包 (tauri:build:webdriver)
#   6. WebdriverIO 桌面全链路 E2E 自动化测试 (SQL Editor, 真实 AI, Schema Diff, Transfer)
#   7. Community vs Pro 双版本隔离自动化验证
# ==============================================================================

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"
cd "$ROOT"

# 统一输出格式
step() {
  printf '\n\033[1;36m====================================================================\033[0m\n'
  printf '\033[1;36m[AUTOMATION] %s\033[0m\n' "$1"
  printf '\033[1;36m====================================================================\033[0m\n'
}

fail() {
  printf '\n\033[1;31m[AUTOMATION FAILED] %s\033[0m\n' "$1" >&2
  exit 1
}

# ------------------------------------------------------------------------------
# 阶段 0: 环境与凭据免密设置
# ------------------------------------------------------------------------------
step "Stage 0: 检查环境配置与凭据设置 (免系统钥匙串 + 真实 AI 注入)"

# 强制使用本地文件密钥，避免任何系统钥匙串弹窗 (免开发者账号)
export DATAZEN_KEYRING="file"
export DATAZEN_DRIVERS="all"
export CI="true"

printf "  ✔ DATAZEN_KEYRING=file (本地加密存储已激活，跳过系统 Keychain)\n"

# 检查 e2e/.env 配置
if [[ -f "$ROOT/e2e/.env" ]]; then
  printf "  ✔ 检测到 e2e/.env 文件，正在加载环境变量...\n"
  set -a
  # shellcheck disable=SC1090
  source "$ROOT/e2e/.env"
  set +a
  if [[ -n "${E2E_AI_API_KEY:-}" ]]; then
    printf "  ✔ 真实 AI 模型已配置: Provider=%s, Endpoint=%s, Model=%s\n" "${E2E_AI_PROVIDER:-open_ai}" "${E2E_AI_ENDPOINT:-}" "${E2E_AI_MODEL:-}"
  else
    printf "  ℹ E2E_AI_API_KEY 为空，AI E2E 将以降级/Mock 模式执行\n"
  fi
else
  printf "  ⚠️  未检测到 e2e/.env，建议从 e2e/.env.example 复制并配置数据库与 AI 参数\n"
fi

# 初始化/清理测试数据库 Schema
step "Stage 0.1: 自动化预置测试数据库 (setup-e2e-env.sh)"
if bash "$ROOT/e2e/setup-e2e-env.sh"; then
  printf "  ✔ 测试数据库 Schema 预置成功\n"
else
  printf "  ⚠️  setup-e2e-env.sh 执行异常，请确认本地 PostgreSQL/MySQL 服务是否启动\n"
fi

# ------------------------------------------------------------------------------
# 阶段 1: 静态守护与质量门禁
# ------------------------------------------------------------------------------
step "Stage 1: 静态代码守护与质量门禁 (Typecheck, Terminology, i18n)"

node scripts/generate-builtin-locales.mjs || fail "generate-builtin-locales 失败"
pnpm typecheck || fail "TypeScript typecheck 失败"
pnpm test:ids || fail "ID 术语一致性检查失败 (connectionId vs dbSessionId)"
node scripts/check-managed-stubs.mjs || fail "驱动文件防污染检查失败"
node scripts/check-structure-editor-guardrails.mjs || fail "结构编辑器守卫检查失败"

if node scripts/i18n-sync-check.mjs; then
  printf "  ✔ 多语言翻译完整度校验通过\n"
else
  printf "  ⚠️  存在未补齐的多语言 Key (不阻断执行)\n"
fi

# ------------------------------------------------------------------------------
# 阶段 2: 前端与驱动 UI 单元测试
# ------------------------------------------------------------------------------
step "Stage 2: 前端与驱动 UI 单元测试 (Vitest & Typing Journey)"

pnpm test:unit || fail "前端单元测试失败"
pnpm test:unit:drivers || fail "驱动 UI 单元测试失败"

# ------------------------------------------------------------------------------
# 阶段 3: Rust 后端安全网与驱动契约测试
# ------------------------------------------------------------------------------
step "Stage 3: Rust 后端安全网与驱动契约测试 (Cargo test workspace)"

node scripts/resolve-drivers.mjs --drivers=all || fail "resolve-drivers 失败"
FEATURES="$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")"

cargo test -p datazen-driver-api --lib || fail "cargo test driver-api 失败"
cargo test -p datazen --lib --features "$FEATURES" || fail "cargo test datazen 核心失败"
cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --lib || fail "驱动契约单测失败"
cargo test -p datazen-ai-api --lib || fail "cargo test ai-api 失败"

node scripts/driver-file-stash.mjs restore || fail "driver-file-stash restore 失败"

# ------------------------------------------------------------------------------
# 阶段 4: Webdriver 自动化测试包编译
# ------------------------------------------------------------------------------
step "Stage 4: 编译 Webdriver 桌面自动化测试包 (tauri:build:webdriver)"

if [ "${SKIP_E2E_BUILD:-0}" = "1" ]; then
  printf "  ℹ SKIP_E2E_BUILD=1，跳过编译步骤\n"
else
  pnpm tauri:build:webdriver || fail "tauri:build:webdriver 编译失败"
fi

# ------------------------------------------------------------------------------
# 阶段 5: WebdriverIO 桌面全链路 E2E 自动化测试
# ------------------------------------------------------------------------------
step "Stage 5: WebdriverIO 桌面全链路 E2E 自动化测试"

# 5.1 核心冒烟测试
step "Stage 5.1: 核心 Smoke E2E"
node e2e/run.mjs --skip-build -- --suite smoke || fail "E2E Smoke 测试失败"

# 5.2 5 大 SQL 编辑器专项 E2E
step "Stage 5.2: SQL 编辑器专项 E2E (多语句边界、安全参数、生产力、智能补全)"
npx wdio run e2e/wdio.conf.ts --spec e2e/specs/sql-editor-statement.ts || fail "E2E sql-editor-statement 失败"
npx wdio run e2e/wdio.conf.ts --spec e2e/specs/sql-editor-safety-params.ts || fail "E2E sql-editor-safety-params 失败"
npx wdio run e2e/wdio.conf.ts --spec e2e/specs/sql-editor-productivity.ts || fail "E2E sql-editor-productivity 失败"
npx wdio run e2e/wdio.conf.ts --spec e2e/specs/sql-editor-intelligence.ts || fail "E2E sql-editor-intelligence 失败"

# 5.3 真实 AI 诊断端到端 E2E
step "Stage 5.3: 真实 AI 诊断与特性 E2E (基于 e2e/.env 配置)"
npx wdio run e2e/wdio.conf.ts --spec e2e/specs/sql-editor-ai-error.ts || fail "E2E sql-editor-ai-error 失败"
node e2e/run.mjs --skip-build -- --suite ai || fail "E2E AI Suite 失败"

# 5.4 结构比对 Schema Diff 自动化 E2E
step "Stage 5.4: 结构比对 Schema Diff E2E"
bash e2e/setup-schema-diff-e2e.sh
node e2e/run.mjs --skip-build -- --suite schema-diff || fail "E2E schema-diff 失败"

# 5.5 数据传输与压测 E2E
step "Stage 5.5: 数据传输与 25k 迁移压测 E2E"
bash e2e/setup-data-transfer-e2e.sh
node e2e/run.mjs --skip-build -- --suite data-transfer || fail "E2E data-transfer 失败"

# ------------------------------------------------------------------------------
# 阶段 6: Community vs Pro 双版本隔离自动化验证
# ------------------------------------------------------------------------------
step "Stage 6: 双版本编译与干净室隔离自动化验证"

printf "  -> 验证纯净社区版 (Community) 装配...\n"
node scripts/resolve-pro.mjs --edition=community || fail "resolve-pro community 失败"
pnpm typecheck || fail "社区版类型校验失败"
node scripts/resolve-pro.mjs --restore || fail "resolve-pro restore 失败"

printf '\n\033[1;32m====================================================================\033[0m\n'
printf '\033[1;32m🎉 恭喜！DataZen 全量 100%% 自动化测试套件全部通过 (ALL PASSED) ✔\033[0m\n'
printf '\033[1;32m====================================================================\033[0m\n'
