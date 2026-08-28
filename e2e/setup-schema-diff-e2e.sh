#!/usr/bin/env bash
# Idempotent preflight for Schema Diff E2E: sync DBs + drop leftover sd_* fixture tables.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ENV_FILE="$SCRIPT_DIR/.env"
if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

bash "$SCRIPT_DIR/setup-sync-dbs.sh"

PGHOST="${E2E_PG_HOST:-127.0.0.1}"
PGPORT="${E2E_PG_PORT:-5432}"
PGUSER="${E2E_PG_USER:-postgres}"
PGPASSWORD="${E2E_PG_PASSWORD:-}"
export PGHOST PGPORT PGPASSWORD

MYSQL_HOST="${E2E_MYSQL_HOST:-127.0.0.1}"
MYSQL_PORT="${E2E_MYSQL_PORT:-3306}"
MYSQL_USER="${E2E_MYSQL_USER:-root}"
MYSQL_PASSWORD="${E2E_MYSQL_PASSWORD:-}"

psql_db() {
  local db="$1"
  shift
  psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$db" "$@"
}

mysql_cmd() {
  local args=(-h "$MYSQL_HOST" -P "$MYSQL_PORT" -u "$MYSQL_USER")
  [[ -n "$MYSQL_PASSWORD" ]] && args+=(-p"$MYSQL_PASSWORD")
  mysql "${args[@]}" "$@"
}

drop_pg_sd_tables() {
  local db="$1"
  psql_db "$db" <<'SQL'
DO $$
DECLARE r record;
BEGIN
  FOR r IN
    SELECT tablename FROM pg_tables
    WHERE schemaname = 'public'
      AND (tablename LIKE 'sd\_%' ESCAPE '\' OR tablename ILIKE '%e2e%sd%')
  LOOP
    EXECUTE format('DROP TABLE IF EXISTS %I CASCADE', r.tablename);
  END LOOP;
END $$;
SQL
  echo "  Cleaned sd_* / e2e sd tables in PostgreSQL $db"
}

drop_mysql_sd_tables() {
  local db="$1"
  mysql_cmd -N -e "
    SELECT CONCAT('DROP TABLE IF EXISTS \`', table_name, '\`;')
    FROM information_schema.tables
    WHERE table_schema = '$db'
      AND (table_name LIKE 'sd\\_%' OR table_name LIKE '%e2e%sd%');
  " | mysql_cmd "$db" 2>/dev/null || true
  echo "  Cleaned sd_* / e2e sd tables in MySQL $db"
}

echo "=== Schema Diff E2E preflight ==="
for db in datazen_sync_src datazen_sync_tgt; do
  drop_pg_sd_tables "$db" || true
done
drop_mysql_sd_tables datazen_sync_mysql_tgt || true
echo "=== Schema Diff E2E preflight complete ==="
