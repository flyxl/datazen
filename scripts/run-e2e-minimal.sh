#!/usr/bin/env bash
#
# scripts/run-e2e-minimal.sh — Host WebdriverIO E2E「minimal 集」（DATAZEN_DRIVERS=basic）。
#
# 用途：
#   完全复刻 `pnpm e2e:minimal`（e2e/run.mjs --minimal-drivers）的步骤，但：
#   - 跳过任何 install 步骤（假定 node_modules 已就绪）；
#   - 显式管理 E2E 数据库连接信息的加载；
#   - 构建/运行分步呈现并计时。
#
# ENV 文件解析优先级（找到第一个存在者即用；加载方式 `set -a; source; set +a`）：
#   ① --env-file <path> 参数
#   ② $E2E_ENV_FILE 环境变量
#   ③ 本仓 e2e/.env
#   ④ 回退探测主检出的 e2e/.env（git worktree 相对布局推断，不硬编码绝对路径）：
#      $ROOT/../../e2e/.env（主检出内嵌 .worktrees/，本仓实际布局）与
#      $ROOT/../../datazen/e2e/.env（worktrees 目录与主检出同级）
#   全部缺失时报错退出并列出期望的变量名。文件内容含凭据，绝不打印、不提交。
#   注：e2e/run.mjs 自身也会加载 e2e/.env（不覆盖已有变量），与本脚本互补。
#
# 构建链（文档认可形式，见 docs/development/e2e-testing.md 与 e2e/run.mjs）：
#   node scripts/with-driver-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs
#   → tauri build --debug -f webdriver,driver-postgres,... （自带 webdriver feature 与
#   `.driver-features.json` 检查；beforeBuildCommand 负责生成 dist 并嵌入资产）。
#   禁止裸 `cargo build -p datazen --features webdriver`（会报 asset not found 或
#   4445 不监听）。底层 cargo 天然增量编译并尊重既有 CARGO_TARGET_DIR；
#   --skip-build 可跳过构建复用既有产物。
#
# 构建步骤的环境包装（受限运行环境实测需要）：
#   - HOME/CARGO_HOME/RUSTUP_HOME：与 run-regression.sh 相同的沙箱策略——应用数据、
#     node-gyp 缓存等写入落在仓库内 .regression-home/；工具链仍用真实用户目录
#     （CARGO_HOME_OVERRIDE / RUSTUP_HOME_OVERRIDE 可覆盖）；PATH 不变。
#   - npm_config_verify_deps_before_run=false：抑制 pnpm 在 beforeBuildCommand
#     （`pnpm build`）前的依赖自动安装——worktree 场景下该检查曾触发全量 install，
#     违背「跳过任何 install」并在受限环境因 ~/Library/Caches、~/.npm 写入被拒而失败；
#   - npm_config_cache 指向沙箱内目录，同理避免写真实 HOME。
#
# 已知副作用（正常瞬态，脚本不做任何 git 操作）：
#   - 注入周期会使 src-tauri/Cargo.lock 相对 HEAD 变化，由编排方在提交前还原；
#   - 包装器可能留下 .driver-file-stash/（有孤儿清理机制），不要动也不要提交；
#   - 注入会改写 gitignored 的 src-tauri/capabilities/default.json，脚本已做快照并
#     在退出时还原（见下方 trap）；
#   - .regression-home/ 为沙箱 HOME（已 gitignore），可整目录删除。
#
# 可选：E2E_ISOLATE_HOME=1 时第④步（DB 准备 + 应用启动 + WDIO）也以沙箱 HOME 运行，
#   并自动设 DATAZEN_KEYRING=file（密钥落 {appData}/.key，避免无头环境钥匙串弹窗）。
#   默认关闭，不影响开发机常规行为。第③步构建始终使用沙箱 HOME（见上）。
#
# 用法：
#   bash scripts/run-e2e-minimal.sh [--skip-build] [--env-file <path>] \
#        [-- <额外 wdio 参数，如 --spec e2e/specs/main-window.ts>]
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SKIP_BUILD=0
ENV_FILE_ARG=""
PASSTHROUGH=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    --env-file)
      ENV_FILE_ARG="${2:-}"
      shift 2
      ;;
    --env-file=*)
      ENV_FILE_ARG="${1#*=}"
      shift
      ;;
    --)
      shift
      PASSTHROUGH+=("$@")
      break
      ;;
    *)
      PASSTHROUGH+=("$1")
      shift
      ;;
  esac
done

EXPECTED_VARS=(
  E2E_PG_HOST E2E_PG_PORT E2E_PG_DB E2E_PG_USER E2E_PG_PASSWORD E2E_PG_SUPER
  E2E_MYSQL_HOST E2E_MYSQL_PORT E2E_MYSQL_DB E2E_MYSQL_USER E2E_MYSQL_PASSWORD
  E2E_REDIS_HOST E2E_REDIS_PORT
)

# ---------------------------------------------------------------------------
# 真实 HOME 与沙箱目录探测（同 run-regression.sh 策略）
# ---------------------------------------------------------------------------
SANDBOX_HOME="$ROOT/.regression-home"
REAL_HOME="${REGRESSION_REAL_HOME:-$HOME}"
if [[ ! -d "$REAL_HOME" || "$REAL_HOME" == "$SANDBOX_HOME" ]]; then
  if command -v dscl >/dev/null 2>&1; then
    REAL_HOME="$(dscl . -read "/Users/$(id -un)" NFSHomeDirectory 2>/dev/null | awk '{print $2}')"
  fi
fi
if [[ -z "${REAL_HOME:-}" || ! -d "$REAL_HOME" ]]; then
  echo "ERROR: 无法探测真实 HOME（当前 HOME='$HOME'）。可用 REGRESSION_REAL_HOME=<path> 显式指定。" >&2
  exit 1
fi
CARGO_HOME_WRAPPED="${CARGO_HOME_OVERRIDE:-$REAL_HOME/.cargo}"
RUSTUP_HOME_WRAPPED="${RUSTUP_HOME_OVERRIDE:-$REAL_HOME/.rustup}"
mkdir -p "$SANDBOX_HOME" "$SANDBOX_HOME/npm-cache"

# ---------------------------------------------------------------------------
# 注入周期副作用防护：快照并还原 src-tauri/capabilities/default.json
# （说明见头部注释；与 run-regression.sh 相同机制）
# ---------------------------------------------------------------------------
CAP_DEFAULT="src-tauri/capabilities/default.json"
CAP_BACKUP="$SANDBOX_HOME/capabilities.default.json.bak"
if [[ -f "$CAP_DEFAULT" ]]; then
  cp "$CAP_DEFAULT" "$CAP_BACKUP"
fi
restore_capabilities() {
  if [[ -f "$CAP_BACKUP" ]]; then
    cp "$CAP_BACKUP" "$CAP_DEFAULT"
    echo "[cleanup] 已还原 ${CAP_DEFAULT}（注入周期前状态）"
  elif [[ -f "$CAP_DEFAULT" ]]; then
    rm -f "$CAP_DEFAULT"
    echo "[cleanup] 已移除注入生成的 ${CAP_DEFAULT}（运行前不存在）"
  fi
}
trap restore_capabilities EXIT

# 构建步骤（③）的环境包装前缀
BUILD_ENV_PREFIX=(
  env
  "HOME=$SANDBOX_HOME"
  "CARGO_HOME=$CARGO_HOME_WRAPPED"
  "RUSTUP_HOME=$RUSTUP_HOME_WRAPPED"
  "npm_config_verify_deps_before_run=false"
  "npm_config_cache=$SANDBOX_HOME/npm-cache"
)

# ---------------------------------------------------------------------------
# 步骤执行框架：标题 + 耗时 + 失败即汇总退出
# ---------------------------------------------------------------------------
STEP_NAMES=()
STEP_RESULTS=()
STEP_DURATIONS=()

fmt_dur() {
  local s=$1
  printf '%dm%02ds' $((s / 60)) $((s % 60))
}

print_summary() {
  echo
  echo "====================== E2E minimal 结果汇总 ======================"
  printf '%-4s %-52s %-6s %s\n' '#' '步骤' '结果' '耗时'
  local i
  for i in "${!STEP_NAMES[@]}"; do
    printf '%-4s %-52s %-6s %s\n' \
      "$((i + 1))" "${STEP_NAMES[$i]}" "${STEP_RESULTS[$i]}" "${STEP_DURATIONS[$i]}"
  done
  echo "=================================================================="
}

run_step() {
  local name="$1"
  shift
  STEP_NAMES+=("$name")
  echo
  echo "▶ [$(( ${#STEP_NAMES[@]} ))/5] ${name}"
  echo "  \$ $*"
  local t0=$SECONDS rc=0
  if "$@"; then
    rc=0
  else
    rc=$?
  fi
  local dur=$((SECONDS - t0))
  if [[ $rc -eq 0 ]]; then
    STEP_RESULTS+=("PASS")
    STEP_DURATIONS+=("$(fmt_dur "$dur")")
    echo "✔ ${name} 通过 ($(fmt_dur "$dur"))"
  else
    STEP_RESULTS+=("FAIL")
    STEP_DURATIONS+=("$(fmt_dur "$dur")")
    echo "✘ ${name} 失败 (exit=${rc}, dur=$(fmt_dur "$dur"))" >&2
    print_summary
    echo "E2E minimal 中止：步骤「${name}」失败。" >&2
    exit "$rc"
  fi
}

echo "=== DataZen E2E minimal（basic 驱动集：postgres/mysql/sqlite/redis）==="
echo "ROOT         : ${ROOT}"
echo "真实 HOME    : ${REAL_HOME}"
echo "沙箱 HOME    : ${SANDBOX_HOME}"

# ---------------------------------------------------------------------------
# ENV 文件解析与加载（先于一切步骤）
# ---------------------------------------------------------------------------
ENV_FILE=""
CANDIDATES=()
if [[ -n "$ENV_FILE_ARG" ]]; then
  CANDIDATES+=("$ENV_FILE_ARG")
fi
if [[ -n "${E2E_ENV_FILE:-}" ]]; then
  CANDIDATES+=("$E2E_ENV_FILE")
fi
CANDIDATES+=("$ROOT/e2e/.env")
# worktree 布局回退（相对推断，不硬编码绝对路径）：
#   布局A：主检出内嵌 .worktrees/<branch>（本仓实际布局）→ $ROOT/../../ = 主检出根
#   布局B：worktrees 目录与主检出同级 → $ROOT/../../datazen = 主检出根
CANDIDATES+=("$ROOT/../../e2e/.env")
CANDIDATES+=("$ROOT/../../datazen/e2e/.env")

for c in "${CANDIDATES[@]}"; do
  if [[ -f "$c" ]]; then
    ENV_FILE="$c"
    break
  fi
done

if [[ -z "$ENV_FILE" ]]; then
  echo "ERROR: 未找到 E2E ENV 文件。已按序探测：" >&2
  printf '  - %s\n' "${CANDIDATES[@]}" >&2
  echo "期望以下变量存在于 ENV 文件中（可用 --env-file <path> 或 \$E2E_ENV_FILE 指定）：" >&2
  printf '  %s\n' "${EXPECTED_VARS[@]}" >&2
  exit 1
fi

echo "ENV 文件     : ${ENV_FILE}"
set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a

MISSING_VARS=()
for v in "${EXPECTED_VARS[@]}"; do
  if [[ -z "${!v:-}" ]]; then
    MISSING_VARS+=("$v")
  fi
done
if ((${#MISSING_VARS[@]})); then
  echo "警告: ENV 文件缺少以下变量（相关 spec 可能失败）：" >&2
  printf '  %s\n' "${MISSING_VARS[@]}" >&2
fi

# 第④步可选的 HOME 沙箱包装
RUN_E2E_PREFIX=()
if [[ "${E2E_ISOLATE_HOME:-0}" == "1" ]]; then
  RUN_E2E_PREFIX=(env "HOME=$SANDBOX_HOME" "DATAZEN_KEYRING=file")
  echo "第④步 HOME 沙箱: 启用（E2E_ISOLATE_HOME=1，DATAZEN_KEYRING=file）"
fi

# ---------------------------------------------------------------------------
# ① codegen 就绪检查（generated.ts / driver_init.rs 缺失则补齐 basic 选型）
# ---------------------------------------------------------------------------
check_codegen() {
  local missing=0
  [[ -f src/plugins/generated.ts ]] || {
    echo "  缺少 src/plugins/generated.ts"
    missing=1
  }
  [[ -f src-tauri/src/driver_init.rs ]] || {
    echo "  缺少 src-tauri/src/driver_init.rs"
    missing=1
  }
  if ((missing)); then
    echo "  → node scripts/resolve-drivers.mjs --codegen-only --drivers=basic"
    node scripts/resolve-drivers.mjs --codegen-only --drivers=basic
  else
    echo "  codegen 文件齐全，跳过生成"
  fi
}
run_step "codegen 就绪检查" check_codegen

# ---------------------------------------------------------------------------
# ② 前端构建（generate-menu-labels 是 pnpm build 链中 vite build 的前置）
# ---------------------------------------------------------------------------
run_step "前端构建（vite build）" bash -c 'node scripts/generate-menu-labels.mjs && npx vite build'

# ---------------------------------------------------------------------------
# ③ webdriver debug 构建（注入 basic → tauri build --debug，增量；--skip-build 跳过）
#
# 容错：受限运行环境中 tauri 打包器的 DMG 阶段（hdiutil/AppleScript）可能被拒，
# 但 .app 在 DMG 之前已产出——E2E（e2e/run.mjs）仅需 .app 二进制。因此打包器
# 失败时校验目标二进制是否存在且本次有更新：是则视为构建成功（附注忽略说明）。
# ---------------------------------------------------------------------------
build_webdriver_app() {
  local out_log="$SANDBOX_HOME/e2e-build.log"
  local start_epoch
  start_epoch="$(date +%s)"
  local rc=0
  "${BUILD_ENV_PREFIX[@]}" node scripts/with-driver-inject.mjs --drivers=basic \
    -- node scripts/e2e-tauri-build.mjs >"$out_log" 2>&1 || rc=$?

  if ((rc == 0)); then
    tail -5 "$out_log"
    return 0
  fi

  # 打包器失败：校验 E2E 所需二进制（macOS 优先 .app，与 e2e/run.mjs 选择一致）
  local bin="target/debug/datazen"
  if [[ "$(uname -s)" == "Darwin" && -f "target/debug/bundle/macos/DataZen.app/Contents/MacOS/datazen" ]]; then
    bin="target/debug/bundle/macos/DataZen.app/Contents/MacOS/datazen"
  fi
  if [[ -f "$bin" ]]; then
    local bin_mtime="0"
    bin_mtime="$(stat -f %m "$bin" 2>/dev/null || echo 0)"
    if ((bin_mtime >= start_epoch)); then
      echo "⚠ tauri 打包器退出码 ${rc}，但 E2E 所需二进制已在本次构建中产出：${bin}" >&2
      echo "  （被忽略的打包阶段报错如下）" >&2
      grep -E 'failed to bundle|Error' "$out_log" | head -4 >&2 || true
      return 0
    fi
  fi

  echo "webdriver 构建失败，日志尾部：" >&2
  tail -30 "$out_log" >&2
  return "$rc"
}

if ((SKIP_BUILD)); then
  STEP_NAMES+=("webdriver 构建（--skip-build 跳过）")
  STEP_RESULTS+=("SKIP")
  STEP_DURATIONS+=("-")
  echo
  echo "▶ [3/5] webdriver 构建：--skip-build，复用既有构建产物"
else
  run_step "webdriver 构建（注入 basic + tauri build --debug）" build_webdriver_app
fi

# ---------------------------------------------------------------------------
# ④ DB 准备 + 启动 minimal 集（复刻 e2e/run.mjs --skip-build 行为）
# ---------------------------------------------------------------------------
prepare_dbs() {
  if ! command -v psql >/dev/null 2>&1 || ! command -v mysql >/dev/null 2>&1; then
    echo "  警告: psql/mysql 客户端缺失，跳过 DB 准备（UI-only spec 仍可跑）"
    return 0
  fi
  bash e2e/setup-e2e-env.sh || echo "  警告: e2e/setup-e2e-env.sh 失败，DB 类 spec 可能失败" >&2
}

run_wdio() {
  "${RUN_E2E_PREFIX[@]+"${RUN_E2E_PREFIX[@]}"}" env DATAZEN_DRIVERS=basic node e2e/run.mjs --skip-build \
    ${PASSTHROUGH[@]+"${PASSTHROUGH[@]}"}
}

run_step "DB 准备（setup-e2e-env.sh，失败不致命）" prepare_dbs
run_step "WDIO minimal 集（e2e/run.mjs --skip-build）" run_wdio

print_summary
echo "E2E minimal 完成 ✔"
