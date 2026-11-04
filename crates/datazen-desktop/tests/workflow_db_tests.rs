//! Cross-database workflow tests — the real-DB half of the former
//! `workflow_tests.rs` (tc02/04/05/06/07/13), which was split on 2026-11-02:
//!
//! - 纯逻辑用例（YAML 解析 / 结果序列化，原 tc01/03/08/09/11/12）归位到
//!   `crates/datazen/src/workflow/domain/model_tests.rs`——它们只测 datazen 模型
//!   词汇，随每条 feature 腿编译、由无选择器腿执行；
//! - 历史持久化（原 tc10）删除——宿主 `src/workflow/history.rs` 的内联
//!   测试（record/list/get、clear、跨重载持久化）已逐例覆盖；
//! - 本文件保留真 DB 用例：直接以 sqlx 连 PostgreSQL 与 MySQL，验证
//!   `scripts/test-cross-db-workflow.yaml` 描述的跨库数据流。
//!
//! 运行形态：每个用例带 `#[ignore]`——常规 CI 的宿主集成测试步只**编译**
//! 本文件（防烂），libtest 把用例记成显式 ignored；真跑发生在
//! `.github/workflows/workflow-db-tests.yml`（定时任务，PG/MySQL service
//! 容器 + `scripts/setup-workflow-testdata.sh` 灌数据后
//! `--include-ignored`）或本地：
//!
//! ```text
//! scripts/setup-workflow-testdata.sh   # 灌数据 + 写 .env
//! cargo test -p datazen_desktop --test workflow_db_tests -- --include-ignored --nocapture
//! ```
//!
//! `load_env()` 与连接预检保留：显式 opt-in 的运行若缺 `.env` 或数据库
//! 不可达，仍按原样跳过该用例（视为环境未就绪，而非断言失败）。

use std::path::PathBuf;

fn load_env() -> Option<DbConfig> {
    let env_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join(".env");

    if !env_path.exists() {
        eprintln!("⏭  Skipping workflow tests: .env not found");
        return None;
    }

    let content = std::fs::read_to_string(&env_path).ok()?;
    let mut cfg = DbConfig::default();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "TEST_PG_HOST" => cfg.pg_host = v.trim().to_string(),
                "TEST_PG_PORT" => cfg.pg_port = v.trim().parse().unwrap_or(5432),
                "TEST_PG_USER" => cfg.pg_user = v.trim().to_string(),
                "TEST_PG_PASSWORD" => cfg.pg_password = v.trim().to_string(),
                "TEST_PG_DATABASE" => cfg.pg_database = v.trim().to_string(),
                "TEST_MYSQL_HOST" => cfg.mysql_host = v.trim().to_string(),
                "TEST_MYSQL_PORT" => cfg.mysql_port = v.trim().parse().unwrap_or(3306),
                "TEST_MYSQL_USER" => cfg.mysql_user = v.trim().to_string(),
                "TEST_MYSQL_PASSWORD" => cfg.mysql_password = v.trim().to_string(),
                "TEST_MYSQL_DATABASE" => cfg.mysql_database = v.trim().to_string(),
                _ => {}
            }
        }
    }
    Some(cfg)
}

#[derive(Default)]
struct DbConfig {
    pg_host: String,
    pg_port: u16,
    pg_user: String,
    pg_password: String,
    pg_database: String,
    mysql_host: String,
    mysql_port: u16,
    mysql_user: String,
    mysql_password: String,
    mysql_database: String,
}

impl DbConfig {
    fn pg_url(&self) -> String {
        if self.pg_password.is_empty() {
            format!(
                "postgres://{}@{}:{}/{}",
                self.pg_user, self.pg_host, self.pg_port, self.pg_database
            )
        } else {
            format!(
                "postgres://{}:{}@{}:{}/{}",
                self.pg_user, self.pg_password, self.pg_host, self.pg_port, self.pg_database
            )
        }
    }

    fn mysql_url(&self) -> String {
        if self.mysql_password.is_empty() {
            format!(
                "mysql://{}@{}:{}/{}",
                self.mysql_user, self.mysql_host, self.mysql_port, self.mysql_database
            )
        } else {
            format!(
                "mysql://{}:{}@{}:{}/{}",
                self.mysql_user,
                self.mysql_password,
                self.mysql_host,
                self.mysql_port,
                self.mysql_database
            )
        }
    }
}

// ─── TC-02: Cross-DB Query + Template Resolution ────────────────────────────

#[tokio::test]
#[ignore = "requires local PostgreSQL/MySQL (scripts/setup-workflow-testdata.sh); run with -- --include-ignored"]
async fn tc02_cross_db_query_and_template_resolution() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let mysql_pool = match sqlx::MySqlPool::connect(&cfg.mysql_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to MySQL: {e}");
            return;
        }
    };

    let orders: Vec<(String,)> = sqlx::query_as(
        "SELECT order_id FROM test_orders WHERE uid = 'U001' ORDER BY created_at DESC",
    )
    .fetch_all(&pg_pool)
    .await
    .unwrap();

    assert!(!orders.is_empty(), "U001 should have orders");
    let order_ids: Vec<&str> = orders.iter().map(|r| r.0.as_str()).collect();
    println!("PG orders for U001: {:?}", order_ids);

    let in_clause: String = order_ids
        .iter()
        .map(|id| format!("'{id}'"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT order_id, carrier, tracking_no, status FROM test_logistics WHERE order_id IN ({in_clause})"
    );

    let logistics: Vec<(String, String, String, String)> =
        sqlx::query_as(&sql).fetch_all(&mysql_pool).await.unwrap();

    assert!(
        !logistics.is_empty(),
        "Should find logistics for U001's orders"
    );
    for (oid, carrier, tracking, status) in &logistics {
        println!("  {oid}: {carrier} {tracking} [{status}]");
        assert!(order_ids.contains(&oid.as_str()));
    }

    pg_pool.close().await;
    mysql_pool.close().await;
}

// ─── TC-04: Condition with Real Data ────────────────────────────────────────

#[tokio::test]
#[ignore = "requires local PostgreSQL/MySQL (scripts/setup-workflow-testdata.sh); run with -- --include-ignored"]
async fn tc04_condition_with_real_data() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM test_orders WHERE uid = 'U001'")
        .fetch_one(&pg_pool)
        .await
        .unwrap();
    assert!(
        count.0 > 0,
        "U001 should have orders, condition should be true"
    );

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM test_orders WHERE uid = 'U999'")
        .fetch_one(&pg_pool)
        .await
        .unwrap();
    assert_eq!(
        count.0, 0,
        "U999 should have no orders, condition should be false"
    );

    pg_pool.close().await;
}

// ─── TC-05: ForEach Batch Query ─────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires local PostgreSQL/MySQL (scripts/setup-workflow-testdata.sh); run with -- --include-ignored"]
async fn tc05_foreach_batch_query() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let mysql_pool = match sqlx::MySqlPool::connect(&cfg.mysql_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to MySQL: {e}");
            return;
        }
    };

    let orders: Vec<(String,)> = sqlx::query_as("SELECT order_id FROM test_orders ORDER BY id")
        .fetch_all(&pg_pool)
        .await
        .unwrap();

    assert_eq!(orders.len(), 5, "Should have 5 orders total");

    let mut logistics_found = 0;
    for (order_id,) in &orders {
        let result: Vec<(String, String)> =
            sqlx::query_as("SELECT carrier, status FROM test_logistics WHERE order_id = ?")
                .bind(order_id)
                .fetch_all(&mysql_pool)
                .await
                .unwrap();

        if !result.is_empty() {
            logistics_found += 1;
            println!("  {order_id}: {} [{}]", result[0].0, result[0].1);
        }
    }

    assert_eq!(
        logistics_found, 5,
        "All 5 orders should have logistics entries"
    );

    pg_pool.close().await;
    mysql_pool.close().await;
}

// ─── TC-06: Error Handling — Invalid SQL ────────────────────────────────────

#[tokio::test]
#[ignore = "requires local PostgreSQL/MySQL (scripts/setup-workflow-testdata.sh); run with -- --include-ignored"]
async fn tc06_error_handling_invalid_sql() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let result: Result<Vec<(i64,)>, _> = sqlx::query_as("SELECT id FROM non_existent_table_xyz")
        .fetch_all(&pg_pool)
        .await;

    assert!(result.is_err(), "Query to non-existent table should fail");
    let err = result.err().unwrap().to_string();
    println!("Expected error: {err}");

    pg_pool.close().await;
}

// ─── TC-07: Timeout Behavior ────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires local PostgreSQL/MySQL (scripts/setup-workflow-testdata.sh); run with -- --include-ignored"]
async fn tc07_timeout_behavior() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    // Should complete within timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sqlx::query("SELECT pg_sleep(0.1)").fetch_all(&pg_pool),
    )
    .await;

    assert!(result.is_ok(), "Short query should complete within timeout");

    // Should timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(50),
        sqlx::query("SELECT pg_sleep(2)").fetch_all(&pg_pool),
    )
    .await;

    assert!(result.is_err(), "Long query should timeout");

    pg_pool.close().await;
}

// ─── TC-13: Full Cross-DB Data Flow ─────────────────────────────────────────

#[tokio::test]
#[ignore = "requires local PostgreSQL/MySQL (scripts/setup-workflow-testdata.sh); run with -- --include-ignored"]
async fn tc13_full_cross_db_data_flow() {
    let cfg = match load_env() {
        Some(c) => c,
        None => return,
    };

    let pg_pool = match sqlx::PgPool::connect(&cfg.pg_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PG: {e}");
            return;
        }
    };

    let mysql_pool = match sqlx::MySqlPool::connect(&cfg.mysql_url()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to MySQL: {e}");
            return;
        }
    };

    // 1. Query PG for U001's orders
    let orders: Vec<(String, String, f64)> = sqlx::query_as(
        "SELECT order_id, product_name, amount::float8 FROM test_orders WHERE uid = 'U001' ORDER BY created_at DESC",
    )
    .fetch_all(&pg_pool)
    .await
    .unwrap();

    assert_eq!(orders.len(), 3, "U001 should have 3 orders");

    // 2. Condition check
    assert!(!orders.is_empty());

    // 3. Build IN clause (simulating wildcard template resolution)
    let in_clause: String = orders
        .iter()
        .map(|(oid, _, _)| format!("'{oid}'"))
        .collect::<Vec<_>>()
        .join(",");

    // 4. Query MySQL logistics
    let sql = format!(
        "SELECT order_id, carrier, tracking_no, status FROM test_logistics WHERE order_id IN ({in_clause})"
    );
    let logistics: Vec<(String, String, String, String)> =
        sqlx::query_as(&sql).fetch_all(&mysql_pool).await.unwrap();

    assert_eq!(logistics.len(), 3, "Should find logistics for all 3 orders");

    // 5. Cross-reference validation
    let pg_order_ids: Vec<&str> = orders.iter().map(|(oid, _, _)| oid.as_str()).collect();
    for (oid, carrier, tracking, status) in &logistics {
        assert!(
            pg_order_ids.contains(&oid.as_str()),
            "Logistics order_id {oid} should be in PG results"
        );
        assert!(!carrier.is_empty());
        assert!(!tracking.is_empty());
        assert!(!status.is_empty());
        println!("  Cross-DB verified: {oid} -> {carrier} {tracking} [{status}]");
    }

    // 6. Specific data assertions
    let delivered: Vec<_> = logistics
        .iter()
        .filter(|(_, _, _, s)| s == "delivered")
        .collect();
    assert!(
        !delivered.is_empty(),
        "At least one order should be delivered"
    );

    println!("\n✅ Full cross-DB data flow verified successfully");
    println!(
        "   PG orders: {} -> MySQL logistics: {}",
        orders.len(),
        logistics.len()
    );

    pg_pool.close().await;
    mysql_pool.close().await;
}
