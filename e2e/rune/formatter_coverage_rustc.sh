#!/usr/bin/env bash
set -euo pipefail
compiler=$1; shift
name=""
args=("$@")
for ((i=0; i+1<${#args[@]}; i++)); do
  if [[ ${args[i]} == --crate-name ]]; then name=${args[i+1]}; break; fi
done
case "$name" in
  datazen_sql_formatter)
    printf 'instrument\t%s\n' "$name" >> "$DZ_FORMATTER_PROFILE_RUSTC_LOG"
    exec "$compiler" "$@" -C instrument-coverage ;;
  datazen_native)
    printf 'link-profiler-only\t%s\n' "$name" >> "$DZ_FORMATTER_PROFILE_RUSTC_LOG"
    exec "$compiler" "$@" -C 'link-arg=-Wl,-u,__llvm_profile_runtime' -C "link-arg=$DZ_FORMATTER_PROFILE_RLIB" ;;
  *) exec "$compiler" "$@" ;;
esac
