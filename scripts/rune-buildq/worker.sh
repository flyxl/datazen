#!/usr/bin/env bash
# Serial build/test worker. Exactly ONE instance per machine. FIFO, never parallel.
# Portable: queue dir from RUNE_BUILDQ (default ${TMPDIR:-/tmp}/rune-buildq).
set -uo pipefail
Q="${RUNE_BUILDQ:-${TMPDIR:-/tmp}/rune-buildq}"
mkdir -p "$Q/inbox" "$Q/running" "$Q/out" "$Q/logs"

# Recover anything left mid-flight by a previous worker instance.
for left in "$Q"/running/*.json; do
  [ -e "$left" ] || continue
  id=$(basename "$left" .json)
  mv "$left" "$Q/inbox/$id.json"
done
rm -f "$Q"/running/*.sh "$Q"/running/*.run.sh

echo $$ > "$Q/worker.pid"
trap 'rm -f "$Q/worker.pid"' EXIT

while [ ! -f "$Q/STOP" ]; do
  date -u +%FT%TZ > "$Q/heartbeat"
  job=$(ls -1 "$Q"/inbox/*.json 2>/dev/null | sort | head -1)
  if [ -z "$job" ]; then
    sleep 1
    continue
  fi
  id=$(basename "$job" .json)
  mv "$job" "$Q/running/$id.json" || continue   # atomic claim = mutex

  wd=$(jq -r '.workdir' "$Q/running/$id.json")
  jq -r '.command' "$Q/running/$id.json" > "$Q/running/$id.cmd"
  {
    printf '#!/usr/bin/env bash\nset -uo pipefail\ncd %q || exit 2\n' "$wd"
    printf 'pwd > %q\n' "$Q/running/$id.pwd"
    printf 'echo "[BUILDQ] executing in $(pwd)"\n'
    cat "$Q/running/$id.cmd"
    printf '\n'
  } > "$Q/running/$id.run.sh"

  started=$(date +%s)
  bash "$Q/running/$id.run.sh" > "$Q/logs/$id.log" 2>&1
  rc=$?
  ended=$(date +%s)
  tail -200 "$Q/logs/$id.log" > "$Q/out/$id.tail"
  jq -n --arg id "$id" --argjson rc "$rc" --argjson dur "$((ended - started))" \
    --arg workdir_actual "$(cat "$Q/running/$id.pwd" 2>/dev/null)" \
    --rawfile tail "$Q/out/$id.tail" \
    '{id:$id,exit_code:$rc,duration_s:$dur,workdir_actual:$workdir_actual,tail:$tail}' > "$Q/out/$id.tmp"
  mv "$Q/out/$id.tmp" "$Q/out/$id.result"
  rm -f "$Q/running/$id.json" "$Q/running/$id.cmd" "$Q/running/$id.run.sh" "$Q/running/$id.pwd" "$Q/out/$id.tail"
done
