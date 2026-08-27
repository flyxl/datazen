#!/usr/bin/env bash
# Reset databases after Host E2E. Idempotent; safe to re-run.
# Drops ephemeral tables (name contains "e2e" or prefix sync_) and re-seeds fixtures.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ENV_FILE="$SCRIPT_DIR/.env"

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

PGHOST="${E2E_PG_HOST:-127.0.0.1}"
PGPORT="${E2E_PG_PORT:-5432}"
PGUSER="${E2E_PG_USER:-postgres}"
PGPASSWORD="${E2E_PG_PASSWORD:-}"
export PGHOST PGPORT PGUSER PGPASSWORD

PG_ADMIN_DB="${E2E_PG_ADMIN_DB:-postgres}"
E2E_DB="${E2E_PG_DB:-datazen_e2e}"

MYSQL_HOST="${E2E_MYSQL_HOST:-127.0.0.1}"
MYSQL_PORT="${E2E_MYSQL_PORT:-3306}"
MYSQL_USER="${E2E_MYSQL_USER:-root}"
MYSQL_PASSWORD="${E2E_MYSQL_PASSWORD:-}"
MYSQL_DB="${E2E_MYSQL_DB:-datazen_test}"

psql_admin() {
  psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PG_ADMIN_DB" "$@"
}

psql_db() {
  local db="$1"
  shift
  psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$db" "$@"
}

mysql_cmd() {
  local args=(-h "$MYSQL_HOST" -P "$MYSQL_PORT" -u "$MYSQL_USER")
  if [[ -n "$MYSQL_PASSWORD" ]]; then
    args+=(-p"$MYSQL_PASSWORD")
  fi
  mysql "${args[@]}" "$@"
}

drop_pg_ephemeral_tables() {
  local db="$1"
  if ! psql_db "$db" -tAc "SELECT 1" >/dev/null 2>&1; then
    echo "  SKIP $db (not reachable)"
    return 0
  fi
  psql_db "$db" <<'SQL'
DO $$
DECLARE r record;
BEGIN
  FOR r IN
    SELECT tablename FROM pg_tables
    WHERE schemaname = 'public'
      AND tablename <> 'product'
      AND (
        tablename ILIKE '%e2e%'
        OR tablename LIKE 'sync\_%' ESCAPE '\'
        OR tablename LIKE '_e2e\_%' ESCAPE '\'
      )
  LOOP
    EXECUTE format('DROP TABLE IF EXISTS %I CASCADE', r.tablename);
  END LOOP;
END $$;
SQL
  echo "  Dropped ephemeral tables in $db"
}

drop_mysql_ephemeral_tables() {
  local db="$1"
  if ! mysql_cmd -e "USE \`$db\`; SELECT 1;" >/dev/null 2>&1; then
    echo "  SKIP MySQL $db (not reachable)"
    return 0
  fi
  mysql_cmd -N -e "
    SELECT CONCAT('DROP TABLE IF EXISTS \`', table_name, '\`;')
    FROM information_schema.tables
    WHERE table_schema = '${db}'
      AND (
        table_name LIKE '%e2e%'
        OR table_name LIKE 'sync\_%'
        OR table_name LIKE '_e2e\_%'
      );
  " | mysql_cmd "$db" || true
  echo "  Dropped ephemeral tables in MySQL $db"
}

seed_product_table() {
  psql_db "$E2E_DB" <<'SQL'
CREATE TABLE IF NOT EXISTS product (
  id SERIAL PRIMARY KEY,
  name TEXT NOT NULL DEFAULT 'item',
  status TEXT NOT NULL
);
DELETE FROM product;
INSERT INTO product (name, status) VALUES
  ('Widget', 'active'),
  ('Gadget', 'active'),
  ('Thing', 'pending'),
  ('Gizmo', 'inactive');
SQL
  echo "  Re-seeded product in $E2E_DB"
}

echo "=== E2E teardown: PostgreSQL ==="
for db in "$E2E_DB" datazen_sync_src datazen_sync_tgt; do
  drop_pg_ephemeral_tables "$db"
done
seed_product_table

echo ""
echo "=== E2E teardown: MySQL ==="
drop_mysql_ephemeral_tables "$MYSQL_DB"
drop_mysql_ephemeral_tables datazen_sync_mysql_tgt

echo ""
echo "=== E2E teardown complete ==="
