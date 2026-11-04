#!/usr/bin/env bash
# Create the durable PostgreSQL fixture database used for manual Schema Tree
# testing. This database is deliberately outside the E2E cleanup name patterns.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
ENV_FILE="$REPO_ROOT/e2e/.env"

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

PGHOST="${E2E_PG_HOST:-${PG_HOST:-127.0.0.1}}"
PGPORT="${E2E_PG_PORT:-${PG_PORT:-5432}}"
PGUSER="${E2E_PG_USER:-${PG_USER:-postgres}}"
PGPASSWORD="${E2E_PG_PASSWORD:-${PG_PASSWORD:-}}"
PG_ADMIN_DB="${E2E_PG_ADMIN_DB:-postgres}"
MANUAL_DB="${E2E_PG_MANUAL_DB:-datazen_manual_schema_tree}"
export PGHOST PGPORT PGUSER PGPASSWORD

if [[ ! "$MANUAL_DB" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]]; then
  echo "E2E_PG_MANUAL_DB must be a simple PostgreSQL database identifier." >&2
  exit 2
fi

psql -Xq -v ON_ERROR_STOP=1 -d "$PG_ADMIN_DB" -v manual_db="$MANUAL_DB" <<'SQL'
SELECT format('CREATE DATABASE %I', :'manual_db')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'manual_db')
\gexec
SQL

psql -Xq -v ON_ERROR_STOP=1 -d "$MANUAL_DB" -f "$SCRIPT_DIR/fixtures/manual-schema-tree.sql"
psql -Xq -v ON_ERROR_STOP=1 -d "$MANUAL_DB" <<'SQL'
DO $$
BEGIN
  IF to_regclass('manual_schema_tree.manual_schema_tree_groups') IS NULL
    OR to_regclass('manual_schema_tree.manual_schema_tree_entries') IS NULL
    OR to_regclass('manual_schema_tree.manual_schema_tree_active_entries') IS NULL
    OR to_regclass('manual_schema_tree.manual_schema_tree_group_id_seq') IS NULL THEN
    RAISE EXCEPTION 'manual Schema Tree relation fixture is incomplete';
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'manual_schema_tree'
      AND p.proname = 'manual_schema_tree_normalize_code'
      AND p.prokind = 'f'
  ) OR NOT EXISTS (
    SELECT 1 FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = 'manual_schema_tree'
      AND p.proname = 'manual_schema_tree_archive_entry'
      AND p.prokind = 'p'
  ) OR NOT EXISTS (
    SELECT 1 FROM pg_trigger t
    JOIN pg_class c ON c.oid = t.tgrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = 'manual_schema_tree'
      AND t.tgname = 'manual_schema_tree_entries_touch_updated_at'
      AND NOT t.tgisinternal
  ) THEN
    RAISE EXCEPTION 'manual Schema Tree routine or trigger fixture is incomplete';
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM pg_type t
    JOIN pg_namespace n ON n.oid = t.typnamespace
    WHERE n.nspname = 'manual_schema_tree'
      AND t.typname = 'manual_schema_tree_state'
      AND t.typtype = 'e'
  ) OR NOT EXISTS (
    SELECT 1 FROM pg_type t
    JOIN pg_namespace n ON n.oid = t.typnamespace
    WHERE n.nspname = 'manual_schema_tree'
      AND t.typname = 'manual_schema_tree_code'
      AND t.typtype = 'd'
  ) THEN
    RAISE EXCEPTION 'manual Schema Tree user-defined type fixture is incomplete';
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM manual_schema_tree.manual_schema_tree_entries
    WHERE code = 'MANUAL-001'
  ) THEN
    RAISE EXCEPTION 'manual Schema Tree fixture seed row is missing';
  END IF;
END $$;
SQL
echo "Installed durable PostgreSQL Schema Tree fixtures in database: $MANUAL_DB (schema: manual_schema_tree)"
