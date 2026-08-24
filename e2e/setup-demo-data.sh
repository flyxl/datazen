#!/usr/bin/env bash
# Seed isolated demo databases for e2e/specs/zz-screenshots.ts marketing screenshots.
# Uses dedicated PG/MySQL databases and a restricted demo role.
# Idempotent. Called from setup-e2e-env.sh; safe to run standalone.
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
PGSUPER="${E2E_PG_SUPER:-${E2E_PG_USER:-postgres}}"
PGPASSWORD="${E2E_PG_PASSWORD:-}"
export PGHOST PGPORT PGPASSWORD

DEMO_PG_DB="${E2E_DEMO_PG_DB:-datazen_demo}"
DEMO_PG_DB2="${E2E_DEMO_PG_DB2:-datazen_demo_analytics}"
DEMO_PG_USER="${E2E_DEMO_PG_USER:-datazen_demo}"
DEMO_PG_PASSWORD="${E2E_DEMO_PG_PASSWORD:-datazen_demo}"

DEMO_MYSQL_DB="${E2E_DEMO_MYSQL_DB:-datazen_demo}"
DEMO_MYSQL_USER="${E2E_DEMO_MYSQL_USER:-datazen_demo}"
DEMO_MYSQL_PASSWORD="${E2E_DEMO_MYSQL_PASSWORD:-datazen_demo}"

MYSQL_HOST="${E2E_MYSQL_HOST:-127.0.0.1}"
MYSQL_PORT="${E2E_MYSQL_PORT:-3306}"
MYSQL_SUPER="${E2E_MYSQL_USER:-root}"
MYSQL_PASSWORD="${E2E_MYSQL_PASSWORD:-}"

psql_super() {
  psql -h "$PGHOST" -p "$PGPORT" -U "$PGSUPER" -d "${E2E_PG_ADMIN_DB:-postgres}" "$@"
}

mysql_super() {
  local args=(-h "$MYSQL_HOST" -P "$MYSQL_PORT" -u "$MYSQL_SUPER")
  if [[ -n "$MYSQL_PASSWORD" ]]; then
    args+=(-p"$MYSQL_PASSWORD")
  fi
  mysql "${args[@]}" "$@"
}

echo "=== Demo screenshot databases (zz-screenshots) ==="

# ── PostgreSQL: demo role + databases ──
if ! psql_super -tAc "SELECT 1 FROM pg_roles WHERE rolname='${DEMO_PG_USER}'" | grep -q 1; then
  psql_super -c "CREATE ROLE ${DEMO_PG_USER} WITH LOGIN PASSWORD '${DEMO_PG_PASSWORD}'"
  echo "  Created PG role ${DEMO_PG_USER}"
else
  psql_super -c "ALTER ROLE ${DEMO_PG_USER} WITH LOGIN PASSWORD '${DEMO_PG_PASSWORD}'"
fi

for db in "$DEMO_PG_DB" "$DEMO_PG_DB2"; do
  if ! psql_super -tAc "SELECT 1 FROM pg_database WHERE datname='${db}'" | grep -q 1; then
    psql_super -c "CREATE DATABASE ${db} OWNER ${DEMO_PG_USER}"
    echo "  Created PG database ${db}"
  fi
  psql_super -c "REVOKE CONNECT ON DATABASE ${db} FROM PUBLIC"
  psql_super -c "GRANT CONNECT ON DATABASE ${db} TO ${DEMO_PG_USER}"
  psql_super -c "GRANT ALL PRIVILEGES ON DATABASE ${db} TO ${DEMO_PG_USER}"
done

psql_super -d "$DEMO_PG_DB" <<'SQL'
CREATE TABLE IF NOT EXISTS demo_sales (
  id        SERIAL PRIMARY KEY,
  sale_date DATE NOT NULL,
  category  TEXT NOT NULL,
  region    TEXT NOT NULL,
  amount    NUMERIC(10,2) NOT NULL,
  quantity  INT NOT NULL
);
DELETE FROM demo_sales;

INSERT INTO demo_sales (sale_date, category, region, amount, quantity)
SELECT d::date,
       c.name,
       r.name,
       CASE
         WHEN c.name = '电子产品' AND r.name = '华东' AND EXTRACT(DAY FROM d) <= 20
           THEN 850 + (EXTRACT(DAY FROM d) * 7 % 300)
         WHEN c.name = '电子产品' THEN 320 + (EXTRACT(DAY FROM d) * 13 % 260)
         WHEN c.name = '家居'    THEN 180 + (EXTRACT(DAY FROM d) * 11 % 220)
         WHEN c.name = '服装'    THEN 140 + (EXTRACT(DAY FROM d) * 17 % 200)
         ELSE 60 + (EXTRACT(DAY FROM d) * 23 % 120)
       END::numeric(10,2),
       (1 + (EXTRACT(DAY FROM d) * 3 + length(c.name)) % 9)::int
FROM generate_series('2026-06-01'::date, '2026-06-30'::date, interval '1 day') AS d
CROSS JOIN (VALUES ('电子产品'), ('服装'), ('食品'), ('家居')) AS c(name)
CROSS JOIN (VALUES ('华东'), ('华北'), ('华南')) AS r(name);

CREATE TABLE IF NOT EXISTS demo_customers (
  id      SERIAL PRIMARY KEY,
  name    TEXT NOT NULL,
  region  TEXT NOT NULL DEFAULT '华东',
  points  INT NOT NULL DEFAULT 0
);
DELETE FROM demo_customers;
INSERT INTO demo_customers (name, region, points) VALUES
  ('张伟', '华东', 3200), ('王芳', '华北', 2100), ('李娜', '华南', 1750),
  ('刘强', '华东', 980),  ('陈静', '华北', 640),  ('杨洋', '华南', 420);

CREATE TABLE IF NOT EXISTS test_orders (
  order_id     TEXT PRIMARY KEY,
  uid          TEXT NOT NULL,
  product_name TEXT NOT NULL,
  amount       NUMERIC(10,2) NOT NULL,
  status       TEXT NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL
);
DELETE FROM test_orders;
INSERT INTO test_orders (order_id, uid, product_name, amount, status, created_at) VALUES
  ('ORD-2026-005', 'U001', '机械键盘 K870',   599.00,  '已发货', '2026-06-28 10:15:00+08'),
  ('ORD-2026-002', 'U001', '无线鼠标 M330',   199.00,  '运输中', '2026-06-21 16:40:00+08'),
  ('ORD-2026-001', 'U001', '降噪耳机 H900',   1299.00, '已完成', '2026-06-12 09:05:00+08'),
  ('ORD-2026-003', 'U002', '4K 显示器 U2723', 3299.00, '已完成', '2026-06-10 14:22:00+08'),
  ('ORD-2026-004', 'U002', 'USB-C 扩展坞',    459.00,  '已完成', '2026-06-08 11:30:00+08');
SQL
psql_super -d "$DEMO_PG_DB" -c "GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO ${DEMO_PG_USER}"
psql_super -d "$DEMO_PG_DB" -c "GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO ${DEMO_PG_USER}"
echo "  PostgreSQL ${DEMO_PG_DB} seeded"

psql_super -d "$DEMO_PG_DB2" <<'SQL'
CREATE TABLE IF NOT EXISTS daily_summary (
  summary_date DATE PRIMARY KEY,
  total_amount NUMERIC(12,2) NOT NULL,
  order_count  INT NOT NULL
);
DELETE FROM daily_summary;
INSERT INTO daily_summary (summary_date, total_amount, order_count)
SELECT d::date,
       (1200 + EXTRACT(DAY FROM d) * 37)::numeric(12,2),
       (15 + EXTRACT(DAY FROM d)::int % 8)
FROM generate_series('2026-06-01'::date, '2026-06-07'::date, interval '1 day') AS d;
SQL
psql_super -d "$DEMO_PG_DB2" -c "GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO ${DEMO_PG_USER}"
echo "  PostgreSQL ${DEMO_PG_DB2} seeded"

# ── MySQL: dedicated demo database ──
mysql_super <<SQL
CREATE DATABASE IF NOT EXISTS \`${DEMO_MYSQL_DB}\`;
CREATE USER IF NOT EXISTS '${DEMO_MYSQL_USER}'@'%' IDENTIFIED BY '${DEMO_MYSQL_PASSWORD}';
CREATE USER IF NOT EXISTS '${DEMO_MYSQL_USER}'@'localhost' IDENTIFIED BY '${DEMO_MYSQL_PASSWORD}';
GRANT ALL PRIVILEGES ON \`${DEMO_MYSQL_DB}\`.* TO '${DEMO_MYSQL_USER}'@'%';
GRANT ALL PRIVILEGES ON \`${DEMO_MYSQL_DB}\`.* TO '${DEMO_MYSQL_USER}'@'localhost';
FLUSH PRIVILEGES;
USE \`${DEMO_MYSQL_DB}\`;
CREATE TABLE IF NOT EXISTS test_logistics (
  order_id    VARCHAR(32) PRIMARY KEY,
  carrier     VARCHAR(64) NOT NULL,
  tracking_no VARCHAR(64) NOT NULL,
  status      VARCHAR(32) NOT NULL
);
DELETE FROM test_logistics;
INSERT INTO test_logistics (order_id, carrier, tracking_no, status) VALUES
  ('ORD-2026-001', '顺丰速运', 'SF1432456789012', '已签收'),
  ('ORD-2026-002', '中通快递', 'ZT7543210987654', '派送中'),
  ('ORD-2026-005', '京东物流', 'JD8876543210123', '运输中');
SQL
echo "  MySQL ${DEMO_MYSQL_DB}.test_logistics seeded"

# ── PG: ER-diagram relations (FK graph for 16-er.png) ──
# Separate tables so existing demo_sales/test_orders consumers stay stable.
psql_super -d "$DEMO_PG_DB" <<SQL
CREATE TABLE IF NOT EXISTS demo_products (
  id          SERIAL PRIMARY KEY,
  name        TEXT NOT NULL UNIQUE,
  category    TEXT NOT NULL,
  price       NUMERIC(10,2) NOT NULL,
  stock       INT NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS demo_order_items (
  id            SERIAL PRIMARY KEY,
  order_id      TEXT NOT NULL REFERENCES test_orders(order_id) ON DELETE CASCADE,
  product_id    INT  NOT NULL REFERENCES demo_products(id),
  quantity      INT  NOT NULL,
  unit_price    NUMERIC(10,2) NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_demo_products_category ON demo_products(category);
CREATE INDEX IF NOT EXISTS idx_demo_order_items_order ON demo_order_items(order_id);
CREATE INDEX IF NOT EXISTS idx_demo_order_items_product ON demo_order_items(product_id);
-- Richer structure view (23): unique + check + secondary index on demo_customers.
CREATE UNIQUE INDEX IF NOT EXISTS uq_demo_customers_name ON demo_customers(name);
CREATE INDEX IF NOT EXISTS idx_demo_customers_region ON demo_customers(region);
DELETE FROM demo_order_items;
DELETE FROM demo_products;
INSERT INTO demo_products (name, category, price, stock) VALUES
  ('降噪耳机 H900',   '电子产品', 1299.00, 120),
  ('无线鼠标 M330',   '电子产品',  199.00, 340),
  ('机械键盘 K870',   '电子产品',  599.00, 210),
  ('4K 显示器 U2723', '电子产品', 3299.00,  45),
  ('USB-C 扩展坞',    '电子产品',  459.00, 150),
  ('羽绒外套',        '服装',      899.00,  80),
  ('羊毛衫',          '服装',      459.00, 130),
  ('坚果礼盒',        '食品',      168.00, 500),
  ('精品咖啡豆',      '食品',       98.00, 420),
  ('香薰蜡烛',        '家居',      129.00, 260),
  ('记忆棉枕头',      '家居',      239.00, 180);
INSERT INTO demo_order_items (order_id, product_id, quantity, unit_price)
SELECT o.order_id, p.id, (1 + (length(o.order_id) + ascii(right(o.product_name, 1))) % 3), p.price
FROM test_orders o
JOIN demo_products p ON p.name = o.product_name;
SQL
psql_super -d "$DEMO_PG_DB" -c "GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO ${DEMO_PG_USER}"
psql_super -d "$DEMO_PG_DB" -c "GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO ${DEMO_PG_USER}"
echo "  PostgreSQL ${DEMO_PG_DB} ER relations seeded"

# ── Redis: dedicated demo keyspace ──
# The E2E Redis host is frequently the developer's own instance: NEVER FLUSHDB.
# Seed only namespaced keys inside an isolated logical DB (default db 5).
REDIS_HOST="${E2E_REDIS_HOST:-127.0.0.1}"
REDIS_PORT="${E2E_REDIS_PORT:-6379}"
REDIS_DB="${E2E_REDIS_DEMO_DB:-5}"
redis_cli() {
  if [[ -n "${E2E_REDIS_PASSWORD:-}" ]]; then
    redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -a "$E2E_REDIS_PASSWORD" --no-auth-warning -n "$REDIS_DB" "$@"
  else
    redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -n "$REDIS_DB" "$@"
  fi
}
if redis_cli PING >/dev/null 2>&1; then
  while read -r key; do redis_cli DEL "$key" >/dev/null; done < <(redis_cli --scan --pattern 'demo:*')
  redis_cli SET demo:app:name "DataZen Demo" EX 86400 >/dev/null
  redis_cli SET demo:app:version "0.0.8" EX 86400 >/dev/null
  redis_cli HSET demo:user:1001 name "张伟" region "华东" points 3200 level gold EX 86400 >/dev/null
  redis_cli HSET demo:user:1002 name "王芳" region "华北" points 2100 level silver EX 86400 >/dev/null
  redis_cli RPUSH demo:queue:orders '{"order":"ORD-2026-005","status":"已发货"}' '{"order":"ORD-2026-002","status":"运输中"}' >/dev/null
  redis_cli SADD demo:regions 华东 华北 华南 >/dev/null
  redis_cli ZADD demo:sales:rank 1299 "降噪耳机 H900" 599 "机械键盘 K870" 199 "无线鼠标 M330" >/dev/null
  echo "  Redis db${REDIS_DB} seeded with demo:* keys"
else
  echo "  [warn] Redis unreachable at ${REDIS_HOST}:${REDIS_PORT} — skip seeding (15-redis will [skip])"
fi

echo "=== Demo data ready (${DEMO_PG_DB} + ${DEMO_PG_DB2} + ${DEMO_MYSQL_DB} + redis db${REDIS_DB:-?}) ==="
