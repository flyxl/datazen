#!/usr/bin/env bash
# E2E 运行器：为每个 spec 启动独立 app 实例，真正并行跑。
#
# 用法：
#   bash e2e/run-group.sh --group V1 --port 4445 --data-dir e2e/.app-data-V1 \
#     -- e2e/specs/sql-query.ts e2e/specs/table-data.ts ...
#
# 保证：
# - 每个 spec 一个独立 app 进程 + 独立 WebDriver 端口 + 独立 DATAZEN_DATA_DIR
# - 所有 app 进程在脚本退出时被清理（trap）
# - 不做构建（免打包），不做全局 DB setup/teardown
set -euo pipefail

GROUP=""
BASE_PORT=""
DATA_DIR=""
SCHEMA=""
SPECS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --group) GROUP="$2"; shift 2 ;;
    --port) BASE_PORT="$2"; shift 2 ;;
    --data-dir) DATA_DIR="$2"; shift 2 ;;
    --schema) SCHEMA="$2"; shift 2 ;;
    --) shift; SPECS+=("$@"); break ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$GROUP" || -z "$BASE_PORT" || -z "$DATA_DIR" || ${#SPECS[@]} -eq 0 ]]; then
  echo "usage: $0 --group NAME --port PORT [--schema SCHEMA] --data-dir DIR -- spec..." >&2
  exit 2
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Load e2e/.env
if [[ -f "$ROOT/e2e/.env" ]]; then
  while IFS= read -r line; do
    line="$(echo "$line" | sed 's/#.*//' | xargs)"
    [[ -z "$line" ]] && continue
    key="${line%%=*}"
    val="${line#*=}"
    if [[ -z "${!key:-}" ]]; then export "$key"="$val"; fi
  done < "$ROOT/e2e/.env"
fi

BIN="$ROOT/target/debug/bundle/macos/DataZen.app/Contents/MacOS/datazen"
if [[ "$(uname)" != "Darwin" || ! -x "$BIN" ]]; then
  BIN="$ROOT/target/debug/datazen"
fi
if [[ ! -x "$BIN" ]]; then
  echo "[$GROUP] app binary not found: $BIN (先做一次 webdriver 构建)" >&2
  exit 3
fi

# ── Cleanup ───────────────────────────────────────────────────────────────────
APP_PIDS=()
cleanup() {
  for pid in "${APP_PIDS[@]+"${APP_PIDS[@]}"}"; do
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 1 5); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.5
      done
      kill -9 "$pid" 2>/dev/null || true
    fi
  done
  echo "[$GROUP] all app instances stopped."
}
trap cleanup EXIT INT TERM

# ── Result dir ────────────────────────────────────────────────────────────────
RESULT_DIR=$(mktemp -d)

# ── Per-spec runner: writes result to RESULT_DIR/<name>.result ────────────────
run_one() {
  local idx="$1" spec="$2"
  local port=$((BASE_PORT + idx))
  local sg="${GROUP}_${idx}"
  local sdir="${DATA_DIR}_${idx}"
  local sname; sname="$(basename "$spec" .ts)"
  local rfile="$RESULT_DIR/${sname}.result"
  local logfile="/tmp/e2e-${sg}.log"

  mkdir -p "$sdir"
  rm -rf "${sdir:?}/"*

  echo "[$sg] starting app on port $port for $sname..."

  # Start app with shell-prefix env (reliable, no subshell issues)
  DATAZEN_DATA_DIR="$sdir" \
  TAURI_WEBDRIVER_PORT="$port" \
  E2E_WD_PORT="$port" \
  E2E_WORKER_INDEX="$sg" \
  ${SCHEMA:+E2E_WORKER_SCHEMA="$SCHEMA"} \
  nohup "$BIN" > "$logfile" 2>&1 &
  local app_pid=$!

  # Wait port
  local ready=0
  for _ in $(seq 1 60); do
    if (echo > /dev/tcp/127.0.0.1/$port) 2>/dev/null; then ready=1; break; fi
    sleep 2
  done
  if [[ "$ready" != "1" ]]; then
    echo "SKIP" > "$rfile"
    echo "[$sg] ✗ port $port not ready, SKIP $sname"
    kill "$app_pid" 2>/dev/null || true
    return 1
  fi

  # Run wdio for this single spec
  echo "[$sg] running $sname on port $port..."
  local code=0
  DATAZEN_DATA_DIR="$sdir" \
  E2E_WD_PORT="$port" \
  E2E_WORKER_INDEX="$sg" \
  E2E_SKIP_TEARDOWN=1 \
  ${SCHEMA:+E2E_WORKER_SCHEMA="$SCHEMA"} \
  npx wdio run e2e/wdio.conf.ts --spec "$spec" > "$logfile.wdio" 2>&1 || code=$?

  # Stop app
  kill "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true

  # Safety net: ensure worker database is dropped even if WDIO after hook failed.
  local wdio_db
  wdio_db=$(sed -n 's/.*created worker database: //p' "$logfile.wdio" 2>/dev/null | head -1 | tr -d '[:space:]')
  if [[ -n "$wdio_db" ]]; then
    psql -h "${E2E_PG_HOST:-127.0.0.1}" -p "${E2E_PG_PORT:-5432}" -U "${E2E_PG_USER:-wuxiaolong}" -d postgres \
      -v ON_ERROR_STOP=1 -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${wdio_db}' AND pid <> pg_backend_pid()" 2>/dev/null || true
    psql -h "${E2E_PG_HOST:-127.0.0.1}" -p "${E2E_PG_PORT:-5432}" -U "${E2E_PG_USER:-wuxiaolong}" -d postgres \
      -v ON_ERROR_STOP=1 -c "DROP DATABASE IF EXISTS \"${wdio_db}\"" 2>/dev/null || true
  fi

  if [[ "$code" == "0" ]]; then
    echo "PASSED" > "$rfile"
    echo "[$sg] ✓ $sname PASSED"
  else
    echo "FAILED($code)" > "$rfile"
    echo "[$sg] ✗ $sname FAILED (exit=$code)"
  fi
}

# ── Launch all specs in parallel ──────────────────────────────────────────────
WDIO_PIDS=()
for i in "${!SPECS[@]}"; do
  run_one "$i" "${SPECS[$i]}" &
  WDIO_PIDS+=($!)
done

# Wait for ALL background runs
for pid in "${WDIO_PIDS[@]+"${WDIO_PIDS[@]}"}"; do
  wait "$pid" 2>/dev/null || true
done

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "[$GROUP] ═══ Summary (${#SPECS[@]} specs) ═══"
PASSED=0
FAILED=0
for spec in "${SPECS[@]}"; do
  sname="$(basename "$spec" .ts)"
  rfile="$RESULT_DIR/${sname}.result"
  if [[ -f "$rfile" ]]; then
    result=$(cat "$rfile")
  else
    result="SKIPPED"
  fi
  echo "  $result  $sname"
  case "$result" in
    PASSED) PASSED=$((PASSED + 1)) ;;
    *) FAILED=$((FAILED + 1)) ;;
  esac
done

echo ""
echo "[$GROUP] $PASSED passed, $FAILED failed out of ${#SPECS[@]}"
rm -rf "$RESULT_DIR"

[[ "$FAILED" -gt 0 ]] && exit 1
exit 0
