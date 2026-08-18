-- GoEcoRide 测试数据库初始化脚本
-- 生态出行平台：用户、车辆、行程、评价、站点

BEGIN;

-- ============================================================
-- 扩展
-- ============================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ============================================================
-- 枚举类型
-- ============================================================
CREATE TYPE vehicle_type AS ENUM ('electric_car', 'electric_scooter', 'electric_bike', 'hybrid', 'hydrogen');
CREATE TYPE ride_status   AS ENUM ('requested', 'accepted', 'in_progress', 'completed', 'cancelled');
CREATE TYPE user_role     AS ENUM ('rider', 'driver', 'admin');

-- ============================================================
-- 1. users — 目标 100,000 行
-- ============================================================
CREATE TABLE users (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    username      VARCHAR(50)  NOT NULL UNIQUE,
    email         VARCHAR(120) NOT NULL UNIQUE,
    phone         VARCHAR(20),
    role          user_role    NOT NULL DEFAULT 'rider',
    carbon_saved  NUMERIC(10,2) NOT NULL DEFAULT 0,
    rating        NUMERIC(3,2) DEFAULT 5.00,
    is_verified   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_users_role       ON users (role);
CREATE INDEX idx_users_created_at ON users (created_at);

-- ============================================================
-- 2. stations — 200 个站点
-- ============================================================
CREATE TABLE stations (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name       VARCHAR(100) NOT NULL,
    city       VARCHAR(60)  NOT NULL,
    lat        NUMERIC(9,6) NOT NULL,
    lng        NUMERIC(9,6) NOT NULL,
    capacity   INT NOT NULL DEFAULT 20,
    is_active  BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================
-- 3. vehicles — 5,000 辆
-- ============================================================
CREATE TABLE vehicles (
    id              BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    owner_id        BIGINT NOT NULL REFERENCES users(id),
    plate_number    VARCHAR(20) NOT NULL UNIQUE,
    type            vehicle_type NOT NULL,
    brand           VARCHAR(40),
    model           VARCHAR(40),
    battery_capacity_kwh NUMERIC(6,2),
    range_km        INT,
    seats           SMALLINT NOT NULL DEFAULT 4,
    co2_per_km      NUMERIC(5,2) NOT NULL DEFAULT 0,
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    registered_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_vehicles_owner ON vehicles (owner_id);
CREATE INDEX idx_vehicles_type  ON vehicles (type);

-- ============================================================
-- 4. rides — 200,000 行程
-- ============================================================
CREATE TABLE rides (
    id              BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    rider_id        BIGINT NOT NULL REFERENCES users(id),
    driver_id       BIGINT REFERENCES users(id),
    vehicle_id      BIGINT REFERENCES vehicles(id),
    from_station_id BIGINT REFERENCES stations(id),
    to_station_id   BIGINT REFERENCES stations(id),
    status          ride_status NOT NULL DEFAULT 'requested',
    distance_km     NUMERIC(7,2),
    duration_min    INT,
    fare            NUMERIC(8,2),
    carbon_saved_kg NUMERIC(6,2),
    requested_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ
);

CREATE INDEX idx_rides_rider   ON rides (rider_id);
CREATE INDEX idx_rides_driver  ON rides (driver_id);
CREATE INDEX idx_rides_status  ON rides (status);
CREATE INDEX idx_rides_req_at  ON rides (requested_at);

-- ============================================================
-- 5. ride_reviews — 约 150,000 条评价
-- ============================================================
CREATE TABLE ride_reviews (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ride_id     BIGINT NOT NULL REFERENCES rides(id),
    reviewer_id BIGINT NOT NULL REFERENCES users(id),
    rating      SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment     TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_reviews_ride ON ride_reviews (ride_id);

-- ============================================================
-- 6. carbon_ledger — 碳减排账本，约 200,000 条
-- ============================================================
CREATE TABLE carbon_ledger (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES users(id),
    ride_id     BIGINT REFERENCES rides(id),
    saved_kg    NUMERIC(6,2) NOT NULL,
    description VARCHAR(200),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_carbon_user ON carbon_ledger (user_id);

-- ============================================================
-- 数据填充
-- ============================================================

-- 城市池
CREATE TEMP TABLE _cities (name VARCHAR(60));
INSERT INTO _cities VALUES
  ('北京'),('上海'),('广州'),('深圳'),('杭州'),
  ('成都'),('武汉'),('南京'),('重庆'),('西安'),
  ('苏州'),('长沙'),('郑州'),('青岛'),('大连'),
  ('厦门'),('合肥'),('福州'),('昆明'),('贵阳');

-- 1) users: 100,000 行 (分批插入)
INSERT INTO users (username, email, phone, role, carbon_saved, rating, is_verified, created_at)
SELECT
    'user_' || i,
    'user_' || i || '@goecoride.test',
    '1' || lpad((3000000000 + i)::text, 10, '0'),
    (ARRAY['rider','driver','rider','rider','rider','driver','rider','admin']::user_role[])
        [1 + (i % 8)],
    round((random() * 500)::numeric, 2),
    round((3.0 + random() * 2.0)::numeric, 2),
    random() > 0.3,
    now() - (random() * interval '730 days')
FROM generate_series(1, 100000) AS s(i);

-- 2) stations: 200 个
INSERT INTO stations (name, city, lat, lng, capacity)
SELECT
    '站点-' || i,
    (SELECT name FROM _cities ORDER BY random() LIMIT 1),
    round((22.0 + random() * 18)::numeric, 6),
    round((100.0 + random() * 25)::numeric, 6),
    10 + (random() * 90)::int
FROM generate_series(1, 200) AS s(i);

-- 3) vehicles: 5,000 辆 (owner 从 driver 角色中取)
INSERT INTO vehicles (owner_id, plate_number, type, brand, model, battery_capacity_kwh, range_km, seats, co2_per_km, registered_at)
SELECT
    d.id,
    '京' || chr(65 + (row_number() OVER () % 26)::int)
        || lpad((row_number() OVER ())::text, 5, '0'),
    (ARRAY['electric_car','electric_scooter','electric_bike','hybrid','hydrogen']::vehicle_type[])
        [1 + (row_number() OVER () % 5)],
    (ARRAY['比亚迪','特斯拉','蔚来','小鹏','理想','广汽','吉利','北汽'])
        [1 + (row_number() OVER () % 8)],
    'Model-' || (row_number() OVER () % 20),
    round((30 + random() * 70)::numeric, 2),
    (150 + random() * 500)::int,
    2 + (random() * 5)::smallint,
    round((random() * 20)::numeric, 2),
    now() - (random() * interval '365 days')
FROM (SELECT id FROM users WHERE role = 'driver' ORDER BY id LIMIT 5000) d;

-- 4) rides: 200,000 行程
INSERT INTO rides (rider_id, driver_id, vehicle_id, from_station_id, to_station_id,
                   status, distance_km, duration_min, fare, carbon_saved_kg,
                   requested_at, started_at, completed_at)
SELECT
    (SELECT id FROM users WHERE role = 'rider' ORDER BY random() LIMIT 1),
    v.owner_id,
    v.id,
    (1 + (random() * 199)::int),
    (1 + (random() * 199)::int),
    (ARRAY['completed','completed','completed','completed','in_progress','cancelled']::ride_status[])
        [1 + (i % 6)],
    round((1 + random() * 50)::numeric, 2)       AS dist,
    (5  + random() * 90)::int                     AS dur,
    round((5 + random() * 150)::numeric, 2)       AS fare,
    round((0.1 + random() * 5)::numeric, 2)       AS co2,
    ts,
    CASE WHEN i % 6 < 5 THEN ts + interval '3 minutes' END,
    CASE WHEN i % 6 < 4 THEN ts + ((5 + random()*90)::int || ' minutes')::interval END
FROM generate_series(1, 200000) AS s(i),
     LATERAL (SELECT (now() - (random() * interval '365 days')) AS ts) t,
     LATERAL (SELECT id, owner_id FROM vehicles ORDER BY random() LIMIT 1) v;

-- 5) ride_reviews: 约 150,000 条 (completed 行程的 75%)
INSERT INTO ride_reviews (ride_id, reviewer_id, rating, comment, created_at)
SELECT
    r.id,
    r.rider_id,
    (1 + (random() * 4)::int)::smallint,
    CASE (random() * 5)::int
        WHEN 0 THEN '很棒的出行体验！'
        WHEN 1 THEN '司机很准时，车辆整洁'
        WHEN 2 THEN '路线不太合理，但总体还好'
        WHEN 3 THEN '低碳出行，为环保贡献力量'
        WHEN 4 THEN '等待时间有点长'
        ELSE '下次还会选择 GoEcoRide'
    END,
    r.completed_at + interval '1 hour'
FROM rides r
WHERE r.status = 'completed'
  AND random() < 0.75;

-- 6) carbon_ledger: 与 completed rides 对应
INSERT INTO carbon_ledger (user_id, ride_id, saved_kg, description, recorded_at)
SELECT
    r.rider_id,
    r.id,
    r.carbon_saved_kg,
    '行程 #' || r.id || ' 碳减排',
    COALESCE(r.completed_at, r.requested_at) + interval '5 minutes'
FROM rides r
WHERE r.status = 'completed';

-- ============================================================
-- 更新 users.carbon_saved 汇总
-- ============================================================
UPDATE users u
SET carbon_saved = sub.total
FROM (
    SELECT user_id, COALESCE(SUM(saved_kg), 0) AS total
    FROM carbon_ledger
    GROUP BY user_id
) sub
WHERE u.id = sub.user_id;

-- ============================================================
-- 统计摘要视图
-- ============================================================
CREATE VIEW v_ride_summary AS
SELECT
    date_trunc('month', r.requested_at) AS month,
    COUNT(*)                            AS total_rides,
    COUNT(*) FILTER (WHERE r.status = 'completed') AS completed,
    ROUND(AVG(r.distance_km), 2)        AS avg_distance_km,
    ROUND(SUM(r.carbon_saved_kg), 2)    AS total_carbon_saved_kg
FROM rides r
GROUP BY 1
ORDER BY 1;

COMMIT;

-- 最终数据行数统计
SELECT 'users'         AS table_name, COUNT(*) AS row_count FROM users
UNION ALL SELECT 'stations',      COUNT(*) FROM stations
UNION ALL SELECT 'vehicles',      COUNT(*) FROM vehicles
UNION ALL SELECT 'rides',         COUNT(*) FROM rides
UNION ALL SELECT 'ride_reviews',  COUNT(*) FROM ride_reviews
UNION ALL SELECT 'carbon_ledger', COUNT(*) FROM carbon_ledger
ORDER BY table_name;
