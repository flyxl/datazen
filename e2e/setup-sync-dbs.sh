#!/usr/bin/env bash
# Idempotent setup for data-sync E2E tests.
# Creates test databases and a restricted user in both PostgreSQL and MySQL.
# Always resets the RO user password so leftover accounts match e2e/.env.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ENV_FILE="$SCRIPT_DIR/.env"
if [[ -f "$ENV_FILE" && -z "${E2E_PG_RO_PASSWORD:-}" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

PGHOST="${E2E_PG_HOST:-127.0.0.1}"
PGPORT="${E2E_PG_PORT:-5432}"
PG_SUPER="${E2E_PG_SUPER:-postgres}"
PG_USER="${E2E_PG_USER:-postgres}"
PGPASSWORD="${E2E_PG_PASSWORD:-}"
export PGHOST PGPORT PGPASSWORD

PG_READONLY="${E2E_PG_RO_USER:-datazen_readonly}"
PG_READONLY_PW="${E2E_PG_RO_PASSWORD:?Set E2E_PG_RO_PASSWORD before running this script}"

MYSQL_HOST="${E2E_MYSQL_HOST:-127.0.0.1}"
MYSQL_PORT="${E2E_MYSQL_PORT:-3306}"
MYSQL_USER="${E2E_MYSQL_USER:-root}"
MYSQL_PASSWORD="${E2E_MYSQL_PASSWORD:-}"
MYSQL_READONLY="${E2E_MYSQL_RO_USER:-datazen_readonly}"
MYSQL_READONLY_PW="${E2E_MYSQL_RO_PASSWORD:?Set E2E_MYSQL_RO_PASSWORD before running this script}"
MYSQL_DB="${E2E_MYSQL_DB:-datazen_test}"

PG_DB="${E2E_PG_ADMIN_DB:-postgres}"

psql_as() {
  local user="$1"; shift
  PGPASSWORD="${PGPASSWORD}" psql -h "$PGHOST" -p "$PGPORT" -U "$user" "$@"
}

mysql_cmd() {
  local args=(-h "$MYSQL_HOST" -P "$MYSQL_PORT" -u "$MYSQL_USER")
  if [[ -n "$MYSQL_PASSWORD" ]]; then
    args+=(-p"$MYSQL_PASSWORD")
  fi
  mysql "${args[@]}" "$@"
}

echo "=== PostgreSQL setup ==="

for db in datazen_sync_src datazen_sync_tgt; do
  if psql_as "$PG_USER" -d "$PG_DB" -tAc "SELECT 1 FROM pg_database WHERE datname='$db'" | grep -q 1; then
    echo "  DB $db already exists"
  else
    psql_as "$PG_USER" -d "$PG_DB" -c "CREATE DATABASE $db"
    echo "  Created DB $db"
  fi
done

if psql_as "$PG_SUPER" -d "$PG_DB" -tAc "SELECT 1 FROM pg_roles WHERE rolname='$PG_READONLY'" | grep -q 1; then
  echo "  Role $PG_READONLY already exists"
else
  psql_as "$PG_SUPER" -d "$PG_DB" -c "CREATE USER $PG_READONLY WITH PASSWORD '$PG_READONLY_PW';"
  echo "  Created role $PG_READONLY"
fi
psql_as "$PG_SUPER" -d "$PG_DB" -c "ALTER USER $PG_READONLY WITH PASSWORD '$PG_READONLY_PW';"
echo "  Reset password for $PG_READONLY"

for db in datazen_sync_src datazen_sync_tgt; do
  psql_as "$PG_SUPER" -d "$db" <<SQL
GRANT CONNECT ON DATABASE ${db} TO ${PG_USER};
GRANT USAGE, CREATE ON SCHEMA public TO ${PG_USER};
SQL
done
echo "  Granted writable access for E2E user $PG_USER on both sync databases"

psql_as "$PG_SUPER" -d datazen_sync_tgt <<SQL
GRANT CONNECT ON DATABASE datazen_sync_tgt TO ${PG_READONLY};
GRANT USAGE ON SCHEMA public TO ${PG_READONLY};
GRANT SELECT ON ALL TABLES IN SCHEMA public TO ${PG_READONLY};
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO ${PG_READONLY};
SQL
echo "  Granted read-only access on datazen_sync_tgt"

echo ""
echo "=== MySQL setup ==="

mysql_cmd -e "CREATE DATABASE IF NOT EXISTS datazen_sync_mysql_src;"
mysql_cmd -e "CREATE DATABASE IF NOT EXISTS datazen_sync_mysql_tgt;"
mysql_cmd -e "CREATE DATABASE IF NOT EXISTS \`$MYSQL_DB\`;"
echo "  Ensured DBs datazen_sync_mysql_src, datazen_sync_mysql_tgt and $MYSQL_DB"

mysql_cmd <<SQL
CREATE USER IF NOT EXISTS '${MYSQL_READONLY}'@'localhost' IDENTIFIED BY '${MYSQL_READONLY_PW}';
CREATE USER IF NOT EXISTS '${MYSQL_READONLY}'@'127.0.0.1' IDENTIFIED BY '${MYSQL_READONLY_PW}';
ALTER USER '${MYSQL_READONLY}'@'localhost' IDENTIFIED BY '${MYSQL_READONLY_PW}';
ALTER USER '${MYSQL_READONLY}'@'127.0.0.1' IDENTIFIED BY '${MYSQL_READONLY_PW}';
GRANT SELECT ON datazen_sync_mysql_tgt.* TO '${MYSQL_READONLY}'@'localhost';
GRANT SELECT ON datazen_sync_mysql_tgt.* TO '${MYSQL_READONLY}'@'127.0.0.1';
GRANT SELECT ON \`${MYSQL_DB}\`.* TO '${MYSQL_READONLY}'@'localhost';
GRANT SELECT ON \`${MYSQL_DB}\`.* TO '${MYSQL_READONLY}'@'127.0.0.1';
FLUSH PRIVILEGES;
SQL
echo "  Reset read-only user $MYSQL_READONLY and granted SELECT"

echo ""
echo "=== Setup complete ==="
