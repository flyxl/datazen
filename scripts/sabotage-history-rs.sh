#!/usr/bin/env bash
# Rust sabotage harness for the query-history page/delete work.
#
# Each mutation is applied to the PRODUCT file, then a specific test must go
# red. Two failure modes are called out explicitly because both masquerade as a
# green run:
#   * a mutation that breaks the build produces no "test result:" line at all;
#   * "0 failed" contains the substring "failed", so the result is parsed as an
#     integer instead of grepped.
set -uo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR=/Users/wuxiaolong/code/rust-projects/datazen/target

F=src-tauri/src/store/history_db.rs
PRISTINE=/tmp/hdb.pristine
cp "$F" "$PRISTINE"
restore() { [ -f "$PRISTINE" ] && cp "$PRISTINE" "$F"; }
trap restore EXIT

pass=0
fail=0

# $1 label, $2 "old@@new", $3 test-name filter
sabotage() {
  local label="$1" edit="$2" filter="$3"
  local old="${edit%%@@*}" new="${edit#*@@}"
  restore
  if ! grep -qF -- "$old" "$F"; then
    echo "  ⚠️  $label :: 锚点未命中"
    fail=$((fail + 1))
    return
  fi
  OLD="$old" NEW="$new" TARGET="$F" python3 - <<'PY'
import os
p, old, new = os.environ['TARGET'], os.environ['OLD'], os.environ['NEW']
s = open(p, encoding='utf-8').read()
assert old in s
open(p, 'w', encoding='utf-8').write(s.replace(old, new, 1))
PY
  local out n
  # --color=never so the patterns below match the text, not escape sequences.
  out=$(cargo test --color=never -p datazen_desktop --lib "$filter" 2>&1)
  if echo "$out" | grep -qE "error\[E[0-9]+\]|could not compile"; then
    echo "  ⚠️  $label :: 破坏后编译不过,此破坏无效"
    fail=$((fail + 1))
    return
  fi
  n=$(echo "$out" | grep -oE "^test result: [^\r]*" | head -1)
  if echo "$out" | grep -qE "test result: FAILED|[1-9][0-9]* failed"; then
    echo "  ✅ $(printf '%-34s' "$label")  ->  $n"
    pass=$((pass + 1))
  else
    echo "  ❌ $(printf '%-34s' "$label")  ->  仍然全绿: $n"
    fail=$((fail + 1))
  fi
}

echo "── C: total 必须如实反映命中总数 ──"
sabotage "total 谎报为本页长度" \
  'entries,
                total: total.max(0) as u64,@@total: entries.len() as u64,
                entries,' \
  'history_db'

echo "── C: search 谓词必须真的生效 ──"
sabotage "search 谓词被短路" \
  'if let Some(needle) = filter.search {@@if let Some(needle) = filter.search.filter(|_| false) { let needle = needle;' \
  'history_db'

echo "── 单条删除必须只删一行 ──"
sabotage "删除变成清空整表" \
  'WHERE id = ?1"@@"' \
  'history_db'

echo "── 默认 limit 不得为 0 ──"
sabotage "默认 limit 回到 0" \
  'limit: DEFAULT_HISTORY_PAGE_SIZE,@@limit: 0,' \
  'history_db'

echo "── LIKE 通配符必须转义 ──"
sabotage "% 不再转义" \
  ".replace('%', \"\\\\%\")@@.replace('%', \"\")" \
  'history_db'

echo
restore
echo "── 还原校验 ──"
if cmp -s "$F" "$PRISTINE"; then echo "  ✅ 字节一致"; else echo "  ❌ 不一致"; fail=$((fail + 1)); fi
rm -f "$PRISTINE"
echo
echo "结果: $pass 项破坏成功变红, $fail 项异常"
exit $((fail > 0 ? 1 : 0))
