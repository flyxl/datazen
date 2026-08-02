#!/usr/bin/env bash
# Setup test data for skill workflow cross-database E2E tests.
# Requires: psql, mysql CLI tools, and a .env file with connection info.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

if [ -f "$ROOT_DIR/.env" ]; then
  set -a; source "$ROOT_DIR/.env"; set +a
fi

PG_HOST="${TEST_PG_HOST:-127.0.0.1}"
PG_PORT="${TEST_PG_PORT:-5432}"
PG_USER="${TEST_PG_USER:-goecoride}"
PG_DB="${TEST_PG_DATABASE:-postgres}"

MYSQL_HOST="${TEST_MYSQL_HOST:-127.0.0.1}"
MYSQL_PORT="${TEST_MYSQL_PORT:-3306}"
MYSQL_USER="${TEST_MYSQL_USER:-root}"
MYSQL_PASS="${TEST_MYSQL_PASSWORD:-}"
MYSQL_DB="${TEST_MYSQL_DATABASE:-datazen_test}"

echo "=== Setting up PostgreSQL test data (orders) ==="
PGPASSWORD="${TEST_PG_PASSWORD:-}" psql -h "$PG_HOST" -p "$PG_PORT" -U "$PG_USER" -d "$PG_DB" <<'SQL'
DROP TABLE IF EXISTS test_orders;
CREATE TABLE test_orders (
    id SERIAL PRIMARY KEY,
    uid VARCHAR(50) NOT NULL,
    order_id VARCHAR(50) NOT NULL UNIQUE,
    product_name VARCHAR(100),
    amount DECIMAL(10,2),
    created_at TIMESTAMP DEFAULT NOW()
);
INSERT INTO test_orders (uid, order_id, product_name, amount) VALUES
    ('U001', 'ORD-2026-001', 'MacBook Pro', 14999.00),
    ('U001', 'ORD-2026-002', 'AirPods Pro', 1899.00),
    ('U002', 'ORD-2026-003', 'iPad Air', 4799.00),
    ('U003', 'ORD-2026-004', 'Apple Watch', 2999.00),
    ('U001', 'ORD-2026-005', 'Magic Keyboard', 699.00);
SQL
echo "PostgreSQL: test_orders created with 5 rows"

echo ""
echo "=== Setting up MySQL test data (logistics) ==="
MYSQL_ARGS="-h $MYSQL_HOST -P $MYSQL_PORT -u $MYSQL_USER"
[ -n "$MYSQL_PASS" ] && MYSQL_ARGS="$MYSQL_ARGS -p$MYSQL_PASS"

mysql $MYSQL_ARGS <<SQL
CREATE DATABASE IF NOT EXISTS \`$MYSQL_DB\`;
USE \`$MYSQL_DB\`;
DROP TABLE IF EXISTS test_logistics;
CREATE TABLE test_logistics (
    id INT AUTO_INCREMENT PRIMARY KEY,
    order_id VARCHAR(50) NOT NULL,
    carrier VARCHAR(50),
    tracking_no VARCHAR(100),
    status VARCHAR(20),
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO test_logistics (order_id, carrier, tracking_no, status) VALUES
    ('ORD-2026-001', '顺丰', 'SF1234567890', 'delivered'),
    ('ORD-2026-002', '中通', 'ZT9876543210', 'in_transit'),
    ('ORD-2026-003', '圆通', 'YT1122334455', 'shipped'),
    ('ORD-2026-004', '韵达', 'YD5566778899', 'delivered'),
    ('ORD-2026-005', '极兔', 'JT6677889900', 'pending');
SQL
echo "MySQL: test_logistics created in $MYSQL_DB with 5 rows"

echo ""
echo "=== Test data setup complete ==="
