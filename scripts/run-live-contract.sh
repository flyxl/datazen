#!/usr/bin/env bash
#
# Run the driver contract **live tier** against real servers.
#
# The tests read the process environment only — never an env file themselves
# (fake-runtime-fixtures.md §10.2 rule 5). This runner is the one place that
# populates that environment, so nobody has to paste credentials into a
# terminal by hand and so `cargo test` can be invoked the ordinary way.
#
#   scripts/run-live-contract.sh                 # postgres + mysql, strict
#   scripts/run-live-contract.sh --dry-run       # resolve + report only
#   scripts/run-live-contract.sh --no-provision  # assume fixture DBs exist
#   scripts/run-live-contract.sh --crate datazen-driver-postgres
#
# AGENTS.md rules this script exists to satisfy without breaking:
#
#   1. Reading an env file is legitimate for a program; putting its *content*
#      into the agent's context is not. So `set -x` is forbidden below, and
#      every line this script prints is either a key name from a fixed table
#      written here or a verdict. No value read from any file is ever printed.
#      A raw cargo failure line can quote a DSN, so panic bodies and assertion
#      text are deliberately not forwarded — the full log goes to a temp file
#      whose path is printed for a human to open.
#   2. Values are *parsed*, not executed. `. "$file"` runs whatever is in the
#      file; the reader below can only ever assign KEY=VALUE, so a hostile or
#      merely malformed env file cannot run code in the developer's shell.
#
# Env files rarely spell their keys the way the tests ask for. Rather than
# hard-code one vendor's layout, this script tries a small table of common
# spellings and reports which *strategy* won — a word from the table below,
# never a key discovered inside the file. When none matches, the escape hatch
# is a gitignored alias file (see ALIAS_FILE) mapping your names to the
# `TEST_*` names, without anyone having to say what they are out loud.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ALIAS_FILE="$ROOT/scripts/.live-contract.aliases"
CRATES=("-p" "datazen-driver-postgres" "-p" "datazen-driver-mysql")
DRY_RUN=0
PROVISION=1

while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    --no-provision) PROVISION=0; shift ;;
    --crate) shift; CRATES=("-p" "$1"); shift ;;
    -h|--help) sed -n '4,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

# --------------------------------------------------------------------------
# dotenv reader — assigns KEY=VALUE, executes nothing, prints nothing.
# --------------------------------------------------------------------------
# Later files win over earlier ones, which is the order a developer expects
# from `.env` then `.env.test`. Blank lines and `#` comments are ignored;
# `export KEY=` is accepted; matching surrounding quotes are stripped.
# Anything that is not `KEY=VALUE` is skipped rather than guessed at.
#
# An already-set, non-empty process variable is **not** overwritten. That rule
# exists because of a failure this script actually had: `.env.test` carried an
# empty `MIGRATION_TEST_DATABASE=`, so a shell invocation of
# `MIGRATION_TEST_DATABASE=production_db scripts/run-live-contract.sh` had its
# value silently replaced by the file's empty one and fell back to the default
# name. The refusal guard never saw the name it was supposed to refuse, so a
# test meant to prove the guard goes red instead proved nothing at all. A value
# typed into the shell is the more specific statement of intent and wins; an
# empty one counts as "not stated" and is fillable from the file.
load_dotenv() {
  local file="$1" line key value
  [ -f "$file" ] || return 0
  while IFS= read -r line || [ -n "$line" ]; do
    line="${line#"${line%%[![:space:]]*}"}"          # trim leading space
    [ -z "$line" ] && continue
    case "$line" in '#'*) continue ;; esac
    case "$line" in export\ *) line="${line#export }" ;; esac
    case "$line" in *=*) ;; *) continue ;; esac
    key="${line%%=*}"
    value="${line#*=}"
    key="${key%"${key##*[![:space:]]}"}"             # trim trailing space
    [ -z "$key" ] && continue
    value="${value%"${value##*[![:space:]]}"}"       # trim trailing space
    case "$value" in                                  # strip one quote pair
      \"*\") value="${value#\"}"; value="${value%\"}" ;;
      \'*\') value="${value#\'}"; value="${value%\'}" ;;
    esac
    case "$key" in *[!A-Za-z0-9_]*) continue ;; esac   # not a shell identifier
    [ -n "${!key-}" ] && continue                     # shell already stated it
    export "$key=$value"
  done < "$file"
  return 0
}

FOUND_ANY_FILE=0
for f in "$ROOT/.env" "$ROOT/.env.test" "$ROOT/packages/drivers/.env"; do
  if [ -f "$f" ]; then
    FOUND_ANY_FILE=1
    load_dotenv "$f"
    printf 'loaded %s\n' "${f#"$ROOT"/}"
  fi
done
[ "$FOUND_ANY_FILE" = 1 ] || {
  echo "no env file found; looked for .env, .env.test, packages/drivers/.env" >&2
  exit 1
}

# --------------------------------------------------------------------------
# Alias file — the escape hatch for layouts this script does not guess.
# --------------------------------------------------------------------------
#   # scripts/.live-contract.aliases  (gitignored)
#   # YOUR_NAME = TEST_PG_NAME
# Only the count of applied rules is reported, never a name, so the hatch stays
# usable without becoming a way to read somebody's env file aloud.
if [ -f "$ALIAS_FILE" ]; then
  ALIAS_APPLIED=0
  while IFS= read -r line || [ -n "$line" ]; do
    line="${line#"${line%%[![:space:]]*}"}"
    case "$line" in ''|'#'*) continue ;; esac
    case "$line" in *=*) ;; *) continue ;; esac
    src="${line%%=*}"
    dst="${line#*=}"
    src="${src%"${src##*[![:space:]]}"}"
    dst="${dst%"${dst##*[![:space:]]}"}"
    case "$dst" in TEST_*) ;; *) continue ;; esac
    val="${!src-}"
    [ -z "$val" ] && continue
    export "$dst=$val"
    ALIAS_APPLIED=$((ALIAS_APPLIED + 1))
  done < "$ALIAS_FILE"
  printf 'alias file applied (%d rule(s) bound)\n' "$ALIAS_APPLIED"
fi

# --------------------------------------------------------------------------
# Connection URL, expanded into component variables.
# --------------------------------------------------------------------------
# Output is captured, never echoed, so the expanded credentials stay inside
# this process. `DATABASE_URL` is shared by both families, so a URL is only
# accepted when its scheme says which family it belongs to — otherwise the
# postgres family could pick up a MySQL target.
expand_url() {
  local url="$1" prefix="$2" out kv
  command -v python3 >/dev/null 2>&1 || return 1
  out="$(DZ_URL="$url" DZ_PREFIX="$prefix" python3 -c '
import os, urllib.parse as u
p = u.urlparse(os.environ["DZ_URL"])
pre = os.environ["DZ_PREFIX"]
db = (p.path or "").lstrip("/")
if not db:
    raise SystemExit(1)
rows = [("DATABASE", db),
        ("HOST", p.hostname or ""),
        ("PORT", str(p.port) if p.port else ""),
        ("USER", u.unquote(p.username) if p.username else ""),
        ("PASSWORD", u.unquote(p.password) if p.password else "")]
for name, value in rows:
    if value:
        print("%s%s=%s" % (pre, name, value))
' 2>/dev/null)" || return 1
  [ -n "$out" ] || return 1
  while IFS= read -r kv; do
    export "${kv?}"
  done <<EOF
$out
EOF
  return 0
}

# --------------------------------------------------------------------------
# Resolution — reports a strategy word, never a key name or a value.
# --------------------------------------------------------------------------
# The alias table is the script's own vocabulary, fixed at authoring time, so
# naming the strategy that matched says the script worked without saying
# anything about the file it worked on.
resolve_family() {
  local prefix="$1" scheme="$2" host="$3" user="$4" pass="$5" db="$6"
  local k

  for k in DATABASE_URL POSTGRES_URL PG_URL MYSQL_URL; do
    case "${!k-}" in
      "$scheme"://*) expand_url "${!k}" "$prefix" && { echo "url"; return 0; } ;;
    esac
  done

  for k in $(compgen -v 2>/dev/null); do
    case "$k" in "$prefix"*) [ -n "${!k-}" ] && { echo "direct"; return 0; } ;; esac
  done

  # Only counts as resolved if it can name the database. DATABASE is required
  # anyway, and a partial set could only ever produce a REFUSED — worse, the
  # postgres and mysql drivers' own admin_commands tests read bare PGHOST /
  # MYSQL_HOST, so a half-filled variable block here would silently shadow an
  # absent TEST_* block.
  if [ -n "$db" ]; then
    [ -n "$host" ] && export "$prefix""HOST=$host"
    [ -n "$user" ] && export "$prefix""USER=$user"
    [ -n "$pass" ] && export "$prefix""PASSWORD=$pass"
    [ -n "$db" ]   && export "$prefix""DATABASE=$db"
    echo "component-vars"
    return 0
  fi

  echo "none"
}

# --------------------------------------------------------------------------
# Presence report — verdicts only.
# --------------------------------------------------------------------------
# Mirrors Contract::availability(): a prefix needs *some* key set, DATABASE is
# required, DATABASE_B must differ from it, and both must be `dz_fixture_*`
# dedicated databases so a live run cannot touch a real one.
MISSING=0
check_prefix() {
  local prefix="$1" strategy="$2"
  local db_var="${prefix}DATABASE" db b_var="${prefix}DATABASE_B" b
  local found=0 k kv
  db="${!db_var:-}"
  b="${!b_var:-}"

  printf '%-12s strategy=%-15s' "$prefix" "$strategy"
  for k in $(compgen -v 2>/dev/null); do
    case "$k" in "$prefix"*) [ -n "${!k-}" ] && found=1 ;; esac
  done
  if [ "$found" = 0 ]; then
    printf ' ABSENT\n'
    printf '  no %s* key resolved from any env file\n' "$prefix"
    MISSING=1
    return
  fi
  printf ' present'
  for k in HOST PORT USER PASSWORD; do
    kv="${prefix}${k}"
    [ -n "${!kv:-}" ] && printf ' %s' "$k"
  done
  printf '\n'

  if [ -z "$db" ]; then
    printf '  %-14s MISSING (required)\n' "DATABASE"
    MISSING=1
  else
    case "$db" in
      dz_fixture_*) printf '  %-14s ok (dedicated fixture name)\n' "DATABASE" ;;
      *) printf '  %-14s REFUSED (not a dz_fixture_* database)\n' "DATABASE"; MISSING=1 ;;
    esac
  fi

  if [ -z "$b" ]; then
    printf '  %-14s MISSING (required: it defaults to DATABASE, and two equal targets are refused)\n' "DATABASE_B"
    MISSING=1
  elif [ "$b" = "$db" ]; then
    printf '  %-14s REFUSED (same as DATABASE)\n' "DATABASE_B"
    MISSING=1
  else
    case "$b" in
      dz_fixture_*) printf '  %-14s ok (distinct fixture name)\n' "DATABASE_B" ;;
      *) printf '  %-14s REFUSED (not a dz_fixture_* database)\n' "DATABASE_B"; MISSING=1 ;;
    esac
  fi
}

PG_STRATEGY="$(resolve_family TEST_PG_ 'postgres|postgresql' \
  "${PGHOST:-${POSTGRES_HOST:-}}" "${PGUSER:-${POSTGRES_USER:-}}" \
  "${PGPASSWORD:-${POSTGRES_PASSWORD:-}}" \
  "${PGDATABASE:-${POSTGRES_DB:-${POSTGRES_DATABASE:-}}}")"
MY_STRATEGY="$(resolve_family TEST_MYSQL_ 'mysql|mariadb' \
  "${MYSQL_HOST:-}" "${MYSQL_USER:-}" "${MYSQL_PASSWORD:-}" "${MYSQL_DATABASE:-}")"

echo
check_prefix TEST_PG_ "$PG_STRATEGY"
check_prefix TEST_MYSQL_ "$MY_STRATEGY"

# --------------------------------------------------------------------------
# Fixture provisioning.
# --------------------------------------------------------------------------
# The live cases create and drop real objects, so both fixture databases have to
# exist before cargo runs. Provisioning refuses any name that is not a fixture
# name, and that gate is a second, independent copy of the rule in check_prefix
# rather than a call into it: this path has the ability to CREATE, so it must
# not be widenable by an edit to the reporting path. Because the accepted
# charset is [A-Za-z0-9_] only, a name that passes the gate cannot carry a
# quote, backtick or semicolon into the statements below.
fixture_name_ok() {
  printf '%s' "$1" | grep -Eq '^dz_fixture_[A-Za-z0-9_]+$'
}

# The migration tier gets a *separate* gate rather than a second case in the one
# above. Those suites CREATE and DROP schemas inside the target database, so the
# rule is deliberately narrower than the Rust side, which lets an individual
# suite also accept a name it was historically written against (`datazen_test`
# and friends). Here there is no escape hatch at all: only a name this script
# itself minted can be created.
migration_name_ok() {
  printf '%s' "$1" | grep -Eq '^dz_mig_[A-Za-z0-9_]+$'
}

# A pre-existing MIGRATION_TEST_DATABASE / MYSQL_MIGRATION_TEST_DATABASE in an
# env file wins, so a developer who already provisioned a migration database is
# not forced to create a second one. Otherwise the runner mints the name itself:
# the migration tier needs a database distinct from the cross-database tier,
# because those suites drop schemas and the two would otherwise share one.
MIG_PG_DB="${MIGRATION_TEST_DATABASE:-dz_mig_contract_pg}"
MIG_MY_DB="${MYSQL_MIGRATION_TEST_DATABASE:-dz_mig_contract_mysql}"

# Client diagnostics land in a temp file the agent is not expected to read:
# psql and mysql both echo host, user and sometimes the DSN on failure, and a
# redacted one-line hint is no more useful than the path to the real thing.
PROVISION_LOG="$(mktemp -t datazen-live-provision).log"
PROVISION_FAIL=0

ensure_pg() {
  local db="$1" gate="${2:-fixture_name_ok}" h p u pw
  h="${TEST_PG_HOST:-127.0.0.1}"
  p="${TEST_PG_PORT:-5432}"
  u="${TEST_PG_USER:-postgres}"
  pw="${TEST_PG_PASSWORD-}"
  if ! "$gate" "$db"; then
    printf '  %-26s REFUSED (not an accepted fixture name; nothing created)\n' "$db"
    PROVISION_FAIL=1
    return 1
  fi
  if PGPASSWORD="$pw" psql -X -q -tA -h "$h" -p "$p" -U "$u" -d postgres \
       -c "SELECT 1 FROM pg_database WHERE datname = '$db'" 2>>"$PROVISION_LOG" | grep -q 1; then
    printf '  %-26s present\n' "$db"
    return 0
  fi
  if PGPASSWORD="$pw" createdb -h "$h" -p "$p" -U "$u" "$db" 2>>"$PROVISION_LOG"; then
    printf '  %-26s created\n' "$db"
  else
    printf '  %-26s CREATE FAILED\n' "$db"
    PROVISION_FAIL=1
  fi
}

ensure_my() {
  local db="$1" gate="${2:-fixture_name_ok}" h p u pw
  h="${TEST_MYSQL_HOST:-127.0.0.1}"
  p="${TEST_MYSQL_PORT:-3306}"
  u="${TEST_MYSQL_USER:-root}"
  pw="${TEST_MYSQL_PASSWORD-}"
  if ! "$gate" "$db"; then
    printf '  %-26s REFUSED (not an accepted fixture name; nothing created)\n' "$db"
    PROVISION_FAIL=1
    return 1
  fi
  if MYSQL_PWD="$pw" mysql -h "$h" -P "$p" -u "$u" -N -B \
       -e "SELECT 1 FROM information_schema.schemata WHERE schema_name = '$db'" \
       2>>"$PROVISION_LOG" | grep -q 1; then
    printf '  %-26s present\n' "$db"
    return 0
  fi
  # MYSQL_PWD, not --password: argv is world-readable through ps, and an
  # ini-file escaping bug would be a far worse outcome than a brief window in
  # the child process's own environment.
  if MYSQL_PWD="$pw" mysql -h "$h" -P "$p" -u "$u" \
       -e "CREATE DATABASE \`$db\` CHARACTER SET utf8mb4" 2>>"$PROVISION_LOG"; then
    printf '  %-26s created\n' "$db"
  else
    printf '  %-26s CREATE FAILED\n' "$db"
    PROVISION_FAIL=1
  fi
}

if [ "$DRY_RUN" = 1 ] || [ "$PROVISION" = 0 ]; then
  if [ "$DRY_RUN" = 1 ] && [ "$PROVISION" = 1 ]; then
    echo
    echo "--- fixture databases (dry run: nothing connected, nothing created) ---"
    for d in "${TEST_PG_DATABASE-}" "${TEST_PG_DATABASE_B-}" "${TEST_MYSQL_DATABASE-}" "${TEST_MYSQL_DATABASE_B-}"; do
      [ -z "$d" ] && continue
      if fixture_name_ok "$d"; then
        printf '  %-26s would check / create\n' "$d"
      else
        printf '  %-26s REFUSED (not a fixture name)\n' "$d"
      fi
    done
    echo
    echo "--- migration databases (dry run) ---"
    printf '  %-26s %s\n' "$MIG_PG_DB" \
      "$(migration_name_ok "$MIG_PG_DB" && echo 'would check / create' || echo 'REFUSED (not a dz_mig_* name)')"
    printf '  %-26s %s\n' "$MIG_MY_DB" \
      "$(migration_name_ok "$MIG_MY_DB" && echo 'would check / create' || echo 'REFUSED (not a dz_mig_* name)')"
  fi
elif [ "$PROVISION" = 1 ]; then
  echo
  echo "--- fixture databases ---"
  ensure_pg "${TEST_PG_DATABASE-}"
  ensure_pg "${TEST_PG_DATABASE_B-}"
  ensure_my "${TEST_MYSQL_DATABASE-}"
  ensure_my "${TEST_MYSQL_DATABASE_B-}"
  echo
  echo "--- migration databases ---"
  ensure_pg "$MIG_PG_DB" migration_name_ok
  ensure_my "$MIG_MY_DB" migration_name_ok
  if [ "$PROVISION_FAIL" != 0 ]; then
    printf '\nRefusing to run: a fixture database is missing and could not be created.\n'
    printf 'Client diagnostics (may name the server) were written to:\n  %s\n' "$PROVISION_LOG"
    exit 1
  fi
fi

# --------------------------------------------------------------------------
# Migration-tier environment.
# --------------------------------------------------------------------------
# The suites behind migration_gate read a *different* variable family from the
# cross-database tier, and each family gets its own prefix — one `cargo test -p
# pg -p mysql` shares a single environment across both crates, so one shared
# name would point the MySQL suites at the PostgreSQL server. The values are
# copied from the TEST_* variables that were already resolved above, so this
# asks the developer for nothing they have not already supplied.
#
# Only exported for a family that is actually configured: `--crate
# datazen-driver-postgres` legitimately runs with no MySQL settings at all, and
# exporting a default there would make the gate try to reach 127.0.0.1:3306 and
# fail a suite that should simply have been skipped.
MIG_TIER=0
if [ -n "${TEST_PG_HOST-}${TEST_PG_USER-}${TEST_PG_PASSWORD-}${MIGRATION_TEST_HOST-}${MIGRATION_TEST_USER-}${MIGRATION_TEST_PASSWORD-}" ]; then
  # An already-set MIGRATION_TEST_* value wins over the TEST_PG_* one it was
  # derived from. A developer pointing the migration tier at a different server
  # is a real setup, and silently redirecting it at the cross-database fixture
  # server would run destructive suites against the wrong host. `-` rather than
  # `:-` on the password: a deliberately blank credential must not be replaced.
  export MIGRATION_TEST_HOST="${MIGRATION_TEST_HOST:-${TEST_PG_HOST:-127.0.0.1}}"
  export MIGRATION_TEST_PORT="${MIGRATION_TEST_PORT:-${TEST_PG_PORT:-5432}}"
  export MIGRATION_TEST_USER="${MIGRATION_TEST_USER:-${TEST_PG_USER:-postgres}}"
  export MIGRATION_TEST_PASSWORD="${MIGRATION_TEST_PASSWORD-${TEST_PG_PASSWORD-}}"
  export MIGRATION_TEST_DATABASE="$MIG_PG_DB"
  MIG_TIER=$((MIG_TIER + 1))
fi
if [ -n "${TEST_MYSQL_HOST-}${TEST_MYSQL_USER-}${TEST_MYSQL_PASSWORD-}${MYSQL_MIGRATION_TEST_HOST-}${MYSQL_MIGRATION_TEST_USER-}${MYSQL_MIGRATION_TEST_PASSWORD-}" ]; then
  export MYSQL_MIGRATION_TEST_HOST="${MYSQL_MIGRATION_TEST_HOST:-${TEST_MYSQL_HOST:-127.0.0.1}}"
  export MYSQL_MIGRATION_TEST_PORT="${MYSQL_MIGRATION_TEST_PORT:-${TEST_MYSQL_PORT:-3306}}"
  export MYSQL_MIGRATION_TEST_USER="${MYSQL_MIGRATION_TEST_USER:-${TEST_MYSQL_USER:-root}}"
  export MYSQL_MIGRATION_TEST_PASSWORD="${MYSQL_MIGRATION_TEST_PASSWORD-${TEST_MYSQL_PASSWORD-}}"
  export MYSQL_MIGRATION_TEST_DATABASE="$MIG_MY_DB"
  MIG_TIER=$((MIG_TIER + 1))
fi
echo
echo "migration tier: $MIG_TIER of 2 families configured (their databases were reported above)"

# A dry run has now reported everything it can without touching a server. The
# missing-key verdict still has to be the exit code, because that is the whole
# point of asking for one.
if [ "$DRY_RUN" = 1 ]; then
  printf '\nDRY_RUN=1 nothing executed\n'
  exit "$MISSING"
fi
[ "$MISSING" = 0 ] || {
  printf '\nRefusing to run: strict mode would turn every missing key into a failure.\n'
  printf 'If your env file uses names this script does not guess, list them in\n'
  # shellcheck disable=SC2016  # YOUR_NAME/TEST_PG_NAME is literal template text to print, not an expression
  printf '%s as `YOUR_NAME = TEST_PG_NAME` lines (gitignored).\n' "${ALIAS_FILE#"$ROOT"/}"
  exit 1
}

# Strict mode: without it an unreachable server is a *skip*, and a skip is
# indistinguishable from a pass in `test result: ok`.
export DATAZEN_CONTRACT_REQUIRE_LIVE=1

LOG="$(mktemp -t datazen-live-contract).log"
cargo test "${CRATES[@]}" --tests >"$LOG" 2>&1
STATUS=$?

echo
echo "LOG=$LOG"
echo "EXIT=$STATUS"
echo
echo "--- per-target results ---"
grep -E '^test result:' "$LOG" || echo "(no test result lines — see LOG)"
echo
echo "--- failed test names (bodies withheld; they can quote a DSN) ---"
grep -oE '^test [^ ]+ \.\.\. FAILED' "$LOG" | sed 's/ \.\.\. FAILED//'
grep -oE '^---- [^ ]+ stdout ----' "$LOG" | sed 's/^---- //; s/ stdout ----//'
echo
echo "Full output is in LOG; it may contain connection strings."
exit "$STATUS"