#!/usr/bin/env bash
# Prepare local databases and credentials for Host E2E.
# Idempotent. Loads e2e/.env when present. Safe to re-run before every suite.
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

mysql_cmd() {
  local args=(-h "$MYSQL_HOST" -P "$MYSQL_PORT" -u "$MYSQL_USER")
  if [[ -n "$MYSQL_PASSWORD" ]]; then
    args+=(-p"$MYSQL_PASSWORD")
  fi
  mysql "${args[@]}" "$@"
}

echo "=== PostgreSQL E2E database: $E2E_DB @ $PGHOST:$PGPORT ==="

if psql_admin -tAc "SELECT 1 FROM pg_database WHERE datname='$E2E_DB'" | grep -q 1; then
  echo "  DB $E2E_DB already exists"
else
  psql_admin -c "CREATE DATABASE $E2E_DB"
  echo "  Created DB $E2E_DB"
fi

psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$E2E_DB" <<'SQL'
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

-- Stable tables used by the Host contract journeys. They must exist before
-- the app opens the connection so the initial schema snapshot contains them.
DO $$
DECLARE table_name text;
BEGIN
  FOREACH table_name IN ARRAY ARRAY[
    'e2e_contract_conn', 'e2e_contract_data', 'e2e_contract_filter',
    'e2e_contract_edit', 'e2e_contract_struct', 'e2e_contract_index',
    'e2e_contract_export'
  ] LOOP
    EXECUTE format('CREATE TABLE IF NOT EXISTS %I (id SERIAL PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL)', table_name);
  END LOOP;
END $$;

SQL
echo "  Seeded product (active=2, pending=1, inactive=1)"

echo ""
echo "=== MySQL E2E database: $MYSQL_DB @ $MYSQL_HOST:$MYSQL_PORT ==="
mysql_cmd -e "CREATE DATABASE IF NOT EXISTS \`$MYSQL_DB\`;"
mysql_cmd "$MYSQL_DB" -e "
  CREATE TABLE IF NOT EXISTS product (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    status VARCHAR(30) NOT NULL
  );
  CREATE TABLE IF NOT EXISTS e2e_contract_conn (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    status VARCHAR(30) NOT NULL
  );
  CREATE TABLE IF NOT EXISTS e2e_contract_data LIKE e2e_contract_conn;
  CREATE TABLE IF NOT EXISTS e2e_contract_filter LIKE e2e_contract_conn;
  CREATE TABLE IF NOT EXISTS e2e_contract_edit LIKE e2e_contract_conn;
  CREATE TABLE IF NOT EXISTS e2e_contract_struct LIKE e2e_contract_conn;
  CREATE TABLE IF NOT EXISTS e2e_contract_index LIKE e2e_contract_conn;
  CREATE TABLE IF NOT EXISTS e2e_contract_export LIKE e2e_contract_conn;
  DELETE FROM product;
"
echo "  Ensured DB $MYSQL_DB"

echo ""
bash "$SCRIPT_DIR/setup-sync-dbs.sh"

# Demo data for the marketing screenshot spec (zz-screenshots.ts).
bash "$SCRIPT_DIR/setup-demo-data.sh" || echo "  WARNING: setup-demo-data.sh failed; zz-screenshots may fail."

echo ""
echo "=== Connectivity check ==="
psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$E2E_DB" -tAc "SELECT COUNT(*) FROM product" >/dev/null
echo "  PostgreSQL $E2E_DB OK"
mysql_cmd -e "USE \`$MYSQL_DB\`; SELECT 1;" >/dev/null
echo "  MySQL $MYSQL_DB OK"
echo ""
echo "=== E2E env ready ==="
