#!/usr/bin/env bash
# Sabotage harness for the HistoryPageRequest -> QueryHistoryFilter translation.
#
# Each case mutates product code, requires a specific test to go RED, then
# restores the file and verifies it is byte-identical. Run from the repo root:
#   bash scripts/sabotage-history-filter.sh
#
# A case only counts as successful if the named test actually FAILED. A compile
# error does not count -- it proves nothing about the assertion, so those cases
# are reported separately rather than passed off as evidence.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/Users/wuxiaolong/code/rust-projects/datazen/target}"

PROD="src-tauri/src/commands/query.rs"
PASS=0
FAIL=0

# run_case <name> <test-filter> <file> <python-edit>
run_case() {
  local name="$1" filter="$2" file="$3" edit="$4"
  local backup; backup="$(mktemp)"
  cp "$file" "$backup"

  python3 -c "$edit" || { echo "  [$name] edit script errored"; rm -f "$backup"; FAIL=$((FAIL+1)); return; }
  if diff -q "$backup" "$file" >/dev/null; then
    echo "  [$name] pattern missed, file unchanged (cannot count as a pass)"
    cp "$backup" "$file"; rm -f "$backup"; FAIL=$((FAIL+1)); return
  fi

  local out
  out="$(cargo test --color=never -p datazen_desktop --lib "$filter" 2>&1)"
  # `failed` also matches `0 failed`, so match the FAILED summary explicitly.
  if echo "$out" | grep -qE "test result: FAILED|[1-9][0-9]* failed"; then
    echo "  [$name] RED as required"; PASS=$((PASS+1))
  elif echo "$out" | grep -qE "^error(\[|:)"; then
    echo "  [$name] COMPILE ERROR, not counted (proves no assertion)"; FAIL=$((FAIL+1))
  else
    echo "  [$name] STILL GREEN after mutation (test proves nothing)"; FAIL=$((FAIL+1))
  fi

  cp "$backup" "$file"
  if ! diff -q "$backup" "$file" >/dev/null; then
    echo "  [$name] RESTORE FAILED"; FAIL=$((FAIL+1))
  fi
  rm -f "$backup"
}

echo "-- HistoryPageRequest sabotage --"

run_case "unknown order no longer rejected" \
  an_unknown_order_is_rejected_rather_than_silently_defaulted \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('Some(other) => HistoryOrder::parse(other)\\n                .ok_or_else(|| CommandError::Validation(format!(\"unknown order: {other}\")))?,','Some(_other) => HistoryOrder::Recent,'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

run_case "order always Slowest" \
  every_named_order_survives_the_translation \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('HistoryOrder::parse(other)\\n                .ok_or_else(|| CommandError::Validation(format!(\"unknown order: {other}\")))?,','HistoryOrder::Slowest,'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

run_case "blank order no longer means Recent" \
  an_absent_or_blank_order_means_most_recent \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('None | Some(\"\") => HistoryOrder::Recent,','None => HistoryOrder::Recent,'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

# database and schema are both Option<String>. A swap is silent corruption the
# compiler cannot catch, which is exactly why the by-name assertion exists.
run_case "database and schema swapped (same-typed fields)" \
  each_field_lands_on_the_filter_field_of_the_same_name \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('database: self.database.as_deref(),','database: self.schema.as_deref(),').replace('schema: self.schema.as_deref(),','schema: self.database.as_deref(),'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

run_case "since lands on until" \
  each_field_lands_on_the_filter_field_of_the_same_name \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('since: self.since.as_deref(),','since: self.until.as_deref(),'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

run_case "search lands on database" \
  each_field_lands_on_the_filter_field_of_the_same_name \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('search: self.search.as_deref(),','search: self.database.as_deref(),'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

run_case "limit dropped" \
  each_field_lands_on_the_filter_field_of_the_same_name \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('limit: self.limit,','limit: 0,'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

# If a missing field became Some(""), the query would match an empty database
# string: a filter that looks applied but drops everything, with no clue on screen.
run_case "missing field filled with empty string" \
  an_absent_field_stays_absent_rather_than_becoming_an_empty_filter \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('database: self.database.as_deref(),','database: Some(self.database.as_deref().unwrap_or_default()),'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

run_case "Default limit degrades to 0" \
  a_defaulted_request_asks_for_a_real_page \
  "$PROD" \
  "p='$PROD'; s=open(p,encoding='utf-8').read(); o=s; s=s.replace('limit: DEFAULT_HISTORY_PAGE_SIZE,','limit: 0,'); assert s!=o; open(p,'w',encoding='utf-8').write(s)"

echo "  result: $PASS turned red, $FAIL anomalous"
[ "$FAIL" -eq 0 ] || exit 1
