#!/usr/bin/env bash
# 清空**共享夹具库**：data-sync / data-transfer / schema-diff 三个家族共用的
# `datazen_sync_src` / `datazen_sync_tgt`（PG）与
# `datazen_sync_mysql_src` / `datazen_sync_mysql_tgt`（MySQL）。
#
# 为什么要有这个脚本：这些库此前只在整套开始时（setup-e2e-env.sh →
# setup-sync-dbs.sh）清一次，于是「某个 spec 建的表还在不在」取决于它前面跑过
# 谁——同一个 spec 单跑绿、整包红，红点还会漂到别的 spec 头上。按 spec 重置之后，
# 每个 spec 看到的共享库都是同一个起点。
#
# 边界：
#   - 只动上面四个夹具库，**不碰** `datazen_e2e` / `datazen_test`（种子库）和
#     每个会话自己的 `e2e_w*` 工作库。
#   - PG 保留 `product`（与 setup-sync-dbs.sh 的历史语义一致，改它需要单独评估）。
#   - 幂等：库里有表就删，没表也不报错。
#
# 调用方：e2e/run.mjs（隔离模式每个 spec 前一次）、e2e/services/shared-fixture-reset.ts
# （串跑模式每个 spec 文件前一次）、setup-sync-dbs.sh（全套开始前一次）。
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
export PGHOST PGPORT PGPASSWORD

MYSQL_HOST="${E2E_MYSQL_HOST:-127.0.0.1}"
MYSQL_PORT="${E2E_MYSQL_PORT:-3306}"
MYSQL_USER="${E2E_MYSQL_USER:-root}"
MYSQL_PASSWORD="${E2E_MYSQL_PASSWORD:-}"

PG_SYNC_DBS=(datazen_sync_src datazen_sync_tgt)
MYSQL_SYNC_DBS=(datazen_sync_mysql_src datazen_sync_mysql_tgt)
# 只读角色在 datazen_sync_tgt 上的 SELECT 是 setup-sync-dbs.sh 按「当时的表」发一次
# 的；表被删了再建，per-table 授权不会跟着回来，所以重置后要把这层授权补回去，
# 否则只读断言会因为「同一份夹具、跑在不同顺序下」而变红。
PG_READONLY="${E2E_PG_RO_USER:-datazen_readonly}"
PG_RO_DB="datazen_sync_tgt"

mysql_cmd() {
  local args=(-h "$MYSQL_HOST" -P "$MYSQL_PORT" -u "$MYSQL_USER")
  if [[ -n "$MYSQL_PASSWORD" ]]; then
    args+=(-p"$MYSQL_PASSWORD")
  fi
  mysql "${args[@]}" "$@"
}

# 静默模式：run.mjs 每个 spec 调一次，只在失败时说话。
QUIET="${E2E_RESET_SYNC_QUIET:-0}"
say() { [[ "$QUIET" == "1" ]] || echo "$@"; }

# 库不存在时直接跳过：首次运行前 setup-sync-dbs.sh 会建库。
pg_database_exists() {
  psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "${E2E_PG_ADMIN_DB:-postgres}" \
    -tAc "SELECT 1 FROM pg_database WHERE datname='$1'" | grep -q 1
}

reset_pg() {
  local db="$1"
  pg_database_exists "$db" || return 0
  psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$db" -v ON_ERROR_STOP=1 -q <<'SQL'
DO $$
DECLARE r record;
BEGIN
  FOR r IN
    SELECT tablename FROM pg_tables
    WHERE schemaname = 'public' AND tablename <> 'product'
  LOOP
    EXECUTE format('DROP TABLE IF EXISTS %I CASCADE', r.tablename);
  END LOOP;
END $$;
SQL
  say "  reset ${db}"
}

reset_mysql() {
  local db="$1"
  # 库里一张表都没有时 GROUP_CONCAT 为 NULL，因此先取列表再拼语句。
  local drops
  drops="$(mysql_cmd -N -B -e "
    SELECT CONCAT('DROP TABLE IF EXISTS \`', table_name, '\`;')
    FROM information_schema.tables
    WHERE table_schema = '${db}' AND table_type = 'BASE TABLE';
  " 2>/dev/null || true)"
  [[ -n "$drops" ]] || return 0
  # 外键会让删除顺序变得重要；夹具表之间没有需要保留的关系。
  mysql_cmd "$db" -e "SET FOREIGN_KEY_CHECKS=0; ${drops//$'\n'/ }" >/dev/null 2>&1 || true
  say "  reset ${db}"
}

for db in "${PG_SYNC_DBS[@]}"; do
  reset_pg "$db"
done
for db in "${MYSQL_SYNC_DBS[@]}"; do
  reset_mysql "$db"
done

# 补回只读授权（角色不存在时跳过：首次 setup 之前本来就还没有这个角色）。
if psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "${E2E_PG_ADMIN_DB:-postgres}" \
  -tAc "SELECT 1 FROM pg_roles WHERE rolname='${PG_READONLY}'" | grep -q 1; then
  psql -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PG_RO_DB" -q -v ON_ERROR_STOP=1 <<SQL
GRANT USAGE ON SCHEMA public TO ${PG_READONLY};
GRANT SELECT ON ALL TABLES IN SCHEMA public TO ${PG_READONLY};
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO ${PG_READONLY};
SQL
fi
