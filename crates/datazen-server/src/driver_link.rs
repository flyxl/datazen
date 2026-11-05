//! 驱动链接闭包（W8-a P1）：`extern crate` 强制链接，驱动 crate 里的
//! `datazen_driver_api::inventory::submit!`（inventory）随之入表——与桌面宿主
//! 由 resolve-drivers.mjs codegen 出的 `driver_init.rs` 同一机制（path 驱动的
//! 产物形态）。
//!
//! 每行挂本 crate 的同名 `driver-*` feature；`deploy-drivers` 聚合 feature
//! 一次点亮全部 15 个 path 驱动。mysql crate 自带同源方言工厂（MariaDB /
//! Doris / Starrocks / Manticore / OceanBase-Oracle），postgres crate 自带
//! QuestDB 别名工厂——链接即随本 crate 的其余工厂一起入表，无需单列。
//!
//! git 驱动（kiwi/superset）不在此列：其目录是构建期注入的（gitignored），
//! 干净检出不存在，静态声明会炸 `cargo metadata`（check-managed-stubs 的
//! git 红线，桌面同款）。
//!
//! 缺省（不开任何 driver-*）本模块为空：inventory 无来源，注册表即空表，
//! 任何连接目标照旧显式答「驱动不可用」（WR-F7 闸序）。

#[cfg(feature = "driver-clickhouse")]
extern crate datazen_driver_clickhouse;
#[cfg(feature = "driver-duckdb")]
extern crate datazen_driver_duckdb;
#[cfg(feature = "driver-elasticsearch")]
extern crate datazen_driver_elasticsearch;
#[cfg(feature = "driver-hbase")]
extern crate datazen_driver_hbase;
#[cfg(feature = "driver-influxdb")]
extern crate datazen_driver_influxdb;
#[cfg(feature = "driver-mongodb")]
extern crate datazen_driver_mongodb;
#[cfg(feature = "driver-mysql")]
extern crate datazen_driver_mysql;
#[cfg(feature = "driver-postgres")]
extern crate datazen_driver_postgres;
#[cfg(feature = "driver-redis")]
extern crate datazen_driver_redis;
#[cfg(feature = "driver-rqlite")]
extern crate datazen_driver_rqlite;
#[cfg(feature = "driver-sqlite")]
extern crate datazen_driver_sqlite;
#[cfg(feature = "driver-sqlserver")]
extern crate datazen_driver_sqlserver;
#[cfg(feature = "driver-turso")]
extern crate datazen_driver_turso;
#[cfg(feature = "driver-vector")]
extern crate datazen_driver_vector;
#[cfg(feature = "driver-victoriametrics")]
extern crate datazen_driver_victoriametrics;
