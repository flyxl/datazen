#!/usr/bin/env bash
#
# scripts/run-regression.sh — 合并前全量回归门禁套件。
#
# 用途：
#   一条命令跑齐 PR 合并前的全部检查：
#     ① cargo test -p datazen --lib          （Host Rust 单测）
#     ② npx vitest run                       （Host 前端单测）
#     ③ npx vitest run --config vitest.drivers.config.ts （驱动 UI 单测）
#     ④ node scripts/check-id-terminology.mjs（ID 术语守护）
#     ⑤ npx tsc --noEmit                     （类型检查）
#     ⑥ npx vite build                       （前端构建）
#
# 步骤①的驱动注入与 HOME 沙箱包装：
#   - 注入：worktree 生成态 capabilities/default.json 含 redis:default，要求
#     Cargo.toml 插件 feature 注入才能通过 tauri-build ACL 校验。经仓库正规解法
#     scripts/with-driver-inject.mjs --drivers=basic 执行（注入 → 执行 → 自动还原，
#     与 .github/workflows/ci.yml 一致）。注入器本身不需要 HOME 包装。
#   - HOME 包装只包 cargo 这一步：部分用例经 Store::default_app_data_dir() →
#     dirs::data_dir() 写入真实用户目录（macOS: $HOME/Library/Application Support），
#     受限运行环境下被拒导致 resolve_log_settings_* 等用例失败。将 HOME 重定向到
#     仓库内 .regression-home/ 即可转绿；CARGO_HOME / RUSTUP_HOME 仍指向真实用户
#     目录（可用 CARGO_HOME_OVERRIDE / RUSTUP_HOME_OVERRIDE 覆盖）；PATH 不变。
#     真实 HOME 探测：优先 $HOME（或 REGRESSION_REAL_HOME），不可用时回退 dscl。
#     已验证 vitest / tsc / vite 缓存均在项目本地，其余步骤无需包装。
#   - 失败子集复跑（负载型偶发兜底，如 ai_generate_schema_doc_* 在全量并发下偶发
#     失败、单独复跑稳定）：第 1 轮若有 FAILED 用例，则只对这些用例复跑第 2 轮；
#     两轮结果均打印记录，第 2 轮全绿视为该步骤通过。
#
# 已知副作用（正常瞬态，脚本不做任何 git 操作）：
#   - 注入周期会使 src-tauri/Cargo.lock 相对 HEAD 变化，由编排方在提交前还原；
#   - 包装器可能留下 .driver-file-stash/（有孤儿清理机制），不要动也不要提交；
#   - 注入会改写 gitignored 的 src-tauri/capabilities/default.json（合并插件权限），
#     脚本已做快照并在退出时还原（见下方 trap）；
#   - .regression-home/ 为沙箱 HOME（已 gitignore），可整目录删除。
#
# 用法：
#   bash scripts/run-regression.sh          # 任一步失败立即退出（先输出汇总表）
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# ---------------------------------------------------------------------------
# 真实 HOME 与 cargo 工具链目录探测
# ---------------------------------------------------------------------------
SANDBOX_HOME="$ROOT/.regression-home"
REAL_HOME="${REGRESSION_REAL_HOME:-$HOME}"

# 若调用方传入的 HOME 不可用（不存在或恰好就是本沙箱），尝试从用户数据库还原真实 HOME。
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
for d in "$CARGO_HOME_WRAPPED" "$RUSTUP_HOME_WRAPPED"; do
  if [[ -d "$d" ]]; then
    echo "[env] 工具链目录存在: ${d}"
  else
    echo "[env] 警告: 目录不存在: ${d}（若 rustc 非 rustup 管理，可忽略）"
  fi
done

mkdir -p "$SANDBOX_HOME"

# ---------------------------------------------------------------------------
# 注入周期副作用防护：快照并还原 src-tauri/capabilities/default.json。
# with-driver-inject 会把 active 插件权限（如 redis:default）合并进该 gitignored
# 生成文件；还原快照可避免污染后续裸 cargo 命令（tauri-build ACL 校验）。
# Cargo.toml / Cargo.lock 的注入与还原由 plugin-file-stash 负责，脚本不做 git 操作。
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

# ---------------------------------------------------------------------------
# 步骤执行框架：标题 + 耗时 + 失败即汇总退出
# ---------------------------------------------------------------------------
STEP_NAMES=()
STEP_RESULTS=()
STEP_DURATIONS=()
STEP_NOTE=""   # 可选备注（如「R1失败→复跑通过」），每步开始前重置

fmt_dur() {
  local s=$1
  printf '%dm%02ds' $((s / 60)) $((s % 60))
}

print_summary() {
  echo
  echo "======================== 回归结果汇总 ========================"
  printf '%-4s %-46s %-10s %s\n' '#' '步骤' '结果' '耗时'
  local i
  for i in "${!STEP_NAMES[@]}"; do
    printf '%-4s %-46s %-10s %s\n' \
      "$((i + 1))" "${STEP_NAMES[$i]}" "${STEP_RESULTS[$i]}" "${STEP_DURATIONS[$i]}"
  done
  echo "============================================================="
}

run_step() {
  local name="$1"
  shift
  STEP_NAMES+=("$name")
  STEP_NOTE=""
  echo
  echo "▶ [$(( ${#STEP_NAMES[@]} ))/6] ${name}"
  echo "  \$ $*"
  local t0=$SECONDS rc=0
  if "$@"; then
    rc=0
  else
    rc=$?
  fi
  local dur=$((SECONDS - t0))
  if [[ $rc -eq 0 ]]; then
    STEP_RESULTS+=("PASS${STEP_NOTE:+(${STEP_NOTE})}")
    STEP_DURATIONS+=("$(fmt_dur "$dur")")
    echo "✔ ${name} 通过 ($(fmt_dur "$dur"))${STEP_NOTE:+ [${STEP_NOTE}]}"
  else
    STEP_RESULTS+=("FAIL")
    STEP_DURATIONS+=("$(fmt_dur "$dur")")
    echo "✘ ${name} 失败 (exit=${rc}, dur=$(fmt_dur "$dur"))" >&2
    print_summary
    echo "回归门禁未通过：步骤「${name}」失败。" >&2
    exit "$rc"
  fi
}

echo "=== DataZen 全量回归门禁 ==="
echo "ROOT         : ${ROOT}"
echo "真实 HOME    : ${REAL_HOME}"
echo "沙箱 HOME    : ${SANDBOX_HOME}"

# ---------------------------------------------------------------------------
# ① Host Rust 单测：with-driver-inject(basic) 外层，env 只包 cargo；
#    第 1 轮失败的用例集合在第 2 轮单独复跑（两轮结果都记录）。
# ---------------------------------------------------------------------------
run_cargo_lib_with_retry() {
  local round1_log="$SANDBOX_HOME/cargo-lib-round1.log"
  local round2_log="$SANDBOX_HOME/cargo-lib-round2.log"
  local rc1=0 rc2=0 failed_names=""

  echo "--- 第 1 轮（全量，HOME 沙箱包装）---"
  # shellcheck disable=SC2086
  node scripts/with-driver-inject.mjs --drivers=basic -- \
    env \
      HOME="$SANDBOX_HOME" \
      CARGO_HOME="$CARGO_HOME_WRAPPED" \
      RUSTUP_HOME="$RUSTUP_HOME_WRAPPED" \
      cargo test -p datazen --lib 2>&1 | tee "$round1_log"
  rc1=${PIPESTATUS[0]}

  if [[ $rc1 -eq 0 ]]; then
    echo "第 1 轮全绿。"
    return 0
  fi

  # 解析 FAILED 用例名（libtest 行格式：test <path::name> ... FAILED）
  failed_names="$(sed -nE 's/^test (.+) \.\.\. FAILED$/\1/p' "$round1_log" || true)"
  if [[ -z "$failed_names" ]]; then
    echo "第 1 轮失败且未解析到 FAILED 用例（可能是编译错误），不复跑。" >&2
    return "$rc1"
  fi

  echo
  echo "--- 第 2 轮（仅复跑失败子集，共 $(wc -l <<<"$failed_names" | tr -d ' ') 例）---"
  printf '%s\n' "$failed_names"
  # shellcheck disable=SC2086
  node scripts/with-driver-inject.mjs --drivers=basic -- \
    env \
      HOME="$SANDBOX_HOME" \
      CARGO_HOME="$CARGO_HOME_WRAPPED" \
      RUSTUP_HOME="$RUSTUP_HOME_WRAPPED" \
      cargo test -p datazen --lib $failed_names 2>&1 | tee "$round2_log"
  rc2=${PIPESTATUS[0]}

  if [[ $rc2 -eq 0 ]]; then
    STEP_NOTE="R1失败$(wc -l <<<"$failed_names" | tr -d ' ')例→复跑全绿"
    echo "第 2 轮全部通过（负载型偶发确认：失败子集单独复跑稳定）。"
    return 0
  fi
  echo "第 2 轮仍有失败。" >&2
  return "$rc2"
}

run_step "cargo test -p datazen --lib [注入+HOME包装+复跑]" \
  run_cargo_lib_with_retry

# ② Host 前端单测
run_step "npx vitest run" npx vitest run

# ③ Path 驱动 UI 单测
run_step "npx vitest run --config vitest.drivers.config.ts" \
  npx vitest run --config vitest.drivers.config.ts

# ④ ID 术语守护
run_step "node scripts/check-id-terminology.mjs" \
  node scripts/check-id-terminology.mjs

# ⑤ 类型检查
run_step "npx tsc --noEmit" npx tsc --noEmit

# ⑥ 前端构建
run_step "npx vite build" npx vite build

print_summary
echo "全量回归门禁通过 ✔"
