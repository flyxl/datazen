#!/usr/bin/env bash
# Full E2E parallel runner — splits all specs into groups and runs them in batches.
#
# Usage:
#   bash e2e/full-run.sh [BATCH_SIZE]
#
# Each batch runs via run-group.sh with BATCH_SIZE specs in parallel (default 10).
set -euo pipefail

BATCH_SIZE="${1:-10}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Collect all spec files, excluding helpers and known excluded files
EXCLUDE_PATTERN="connectionJourneyHelpers|dataSyncJourneyHelpers|zz-screenshots|demo-recording|zz-diag"

SPECS_FILE=$(mktemp)
find e2e/specs -name '*.ts' | grep -vE "$EXCLUDE_PATTERN" | sort > "$SPECS_FILE"
TOTAL=$(wc -l < "$SPECS_FILE" | tr -d ' ')
echo "=== Full E2E Run: $TOTAL specs, batch size $BATCH_SIZE ==="
echo ""

BEFORE_COUNT=$(psql -U wuxiaolong -d postgres -t -c "SELECT count(*) FROM pg_database WHERE datname LIKE 'e2e_w%'" 2>/dev/null | tr -d ' ')
echo "Existing e2e_w databases before run: ${BEFORE_COUNT:-0}"
echo ""

BASE_PORT=4445
BATCH_NUM=0
BATCH_SPECS=()
RESULTS_DIR=$(mktemp -d)

while IFS= read -r spec; do
  BATCH_SPECS+=("$spec")

  if [[ ${#BATCH_SPECS[@]} -ge $BATCH_SIZE ]]; then
    BATCH_NUM=$((BATCH_NUM + 1))
    BATCH_PORT=$((BASE_PORT + (BATCH_NUM - 1) * BATCH_SIZE))
    BATCH_DATA_DIR="e2e/.app-data-batch-${BATCH_NUM}"
    GROUP_NAME="batch-${BATCH_NUM}"

    echo ""
    echo "══════ Batch $GROUP_NAME (${#BATCH_SPECS[@]} specs, port $BATCH_PORT) ══════"

    bash e2e/run-group.sh \
      --group "$GROUP_NAME" \
      --port "$BATCH_PORT" \
      --data-dir "$BATCH_DATA_DIR" \
      -- "${BATCH_SPECS[@]}" 2>&1 | tee "$RESULTS_DIR/batch-${BATCH_NUM}.log" || true

    BATCH_SPECS=()
  fi
done < "$SPECS_FILE"

# Handle remaining specs
if [[ ${#BATCH_SPECS[@]} -gt 0 ]]; then
  BATCH_NUM=$((BATCH_NUM + 1))
  BATCH_PORT=$((BASE_PORT + (BATCH_NUM - 1) * BATCH_SIZE))
  BATCH_DATA_DIR="e2e/.app-data-batch-${BATCH_NUM}"
  GROUP_NAME="batch-${BATCH_NUM}"

  echo ""
  echo "══════ Batch $GROUP_NAME (${#BATCH_SPECS[@]} specs, port $BATCH_PORT) ══════"

  bash e2e/run-group.sh \
    --group "$GROUP_NAME" \
    --port "$BATCH_PORT" \
    --data-dir "$BATCH_DATA_DIR" \
    -- "${BATCH_SPECS[@]}" 2>&1 | tee "$RESULTS_DIR/batch-${BATCH_NUM}.log" || true
fi

# ── Aggregate results ─────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════════════════════════════"
echo "                    FULL E2E RESULTS SUMMARY"
echo "══════════════════════════════════════════════════════════════════"

for log in "$RESULTS_DIR"/batch-*.log; do
  if [[ -f "$log" ]]; then
    echo ""
    # Extract the batch summary
    grep -E '(PASSED|FAILED|Summary|passed|failed)' "$log" 2>/dev/null || true
  fi
done

echo ""
echo "── Post-run checks ──"

RESIDUAL_PROCS=$(pgrep -c datazen 2>/dev/null || echo "0")
echo "Residual datazen processes: $RESIDUAL_PROCS"

RESIDUAL_DBS=$(psql -U wuxiaolong -d postgres -t -c "SELECT count(*) FROM pg_database WHERE datname LIKE 'e2e_w%'" 2>/dev/null | tr -d ' ')
echo "Residual e2e_w databases: ${RESIDUAL_DBS:-0}"

if [[ "${RESIDUAL_DBS:-0}" -gt 0 ]]; then
  echo ""
  echo "Cleaning up residual databases..."
  psql -U wuxiaolong -d postgres -t -c "SELECT datname FROM pg_database WHERE datname LIKE 'e2e_w%'" 2>/dev/null | tr -d ' ' | while IFS= read -r db; do
    [[ -z "$db" ]] && continue
    psql -U wuxiaolong -d postgres -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '$db' AND pid <> pg_backend_pid()" 2>/dev/null || true
    psql -U wuxiaolong -d postgres -c "DROP DATABASE IF EXISTS \"$db\"" 2>/dev/null || true
    echo "  dropped: $db"
  done
fi

rm -f "$SPECS_FILE"
rm -rf "$RESULTS_DIR"
echo ""
echo "Done."
