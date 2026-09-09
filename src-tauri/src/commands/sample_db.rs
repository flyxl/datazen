use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::ConnectionConfig;
use crate::store::Store;
use rusqlite::Connection;
use tauri::State;

const SAMPLE_CONN_ID: &str = "sample_sqlite";
const SAMPLE_DB_NAME: &str = "Sample E-Commerce (SQLite)";
const SAMPLE_FILE_NAME: &str = "sample_ecommerce.sqlite";

const SCHEMA_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS customers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    country TEXT NOT NULL,
    signup_date DATE NOT NULL
);

CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    price REAL NOT NULL,
    stock INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS orders (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    customer_id INTEGER NOT NULL REFERENCES customers(id),
    product_id INTEGER NOT NULL REFERENCES products(id),
    quantity INTEGER NOT NULL,
    total_amount REAL NOT NULL,
    status TEXT NOT NULL,
    order_date DATE NOT NULL
);
"#;

const SEED_DATA: &str = r#"
INSERT INTO customers (id, name, email, country, signup_date) VALUES
(1, 'Alice Smith', 'alice@example.com', 'United States', '2026-01-15'),
(2, 'Bob Jones', 'bob@example.co.uk', 'United Kingdom', '2026-01-20'),
(3, 'Chloe Dubois', 'chloe@example.fr', 'France', '2026-02-01'),
(4, 'David Chen', 'david@example.com', 'Canada', '2026-02-10'),
(5, 'Elena Rossi', 'elena@example.it', 'Italy', '2026-02-14'),
(6, 'Fumiya Tanaka', 'tanaka@example.jp', 'Japan', '2026-02-22'),
(7, 'Grace Miller', 'grace@example.com', 'Germany', '2026-03-01'),
(8, 'Hassan Ali', 'hassan@example.ae', 'United Arab Emirates', '2026-03-05');

INSERT INTO products (id, name, category, price, stock) VALUES
(1, 'Pro Wireless Headphone', 'Electronics', 199.99, 45),
(2, 'Ultra Mechanical Keyboard', 'Electronics', 129.50, 80),
(3, '4K Ergonomic Monitor', 'Electronics', 449.00, 20),
(4, 'Ergonomic Standing Desk', 'Home & Office', 389.00, 15),
(5, 'Breathable Mesh Chair', 'Home & Office', 249.99, 30),
(6, 'Desk LED Lamp Pro', 'Home & Office', 59.90, 120),
(7, 'Merino Wool Hoodie', 'Apparel', 89.00, 60),
(8, 'Waterproof Trail Backpack', 'Apparel', 115.00, 40);

INSERT INTO orders (customer_id, product_id, quantity, total_amount, status, order_date) VALUES
(1, 1, 1, 199.99, 'completed', '2026-03-01'),
(1, 2, 1, 129.50, 'completed', '2026-03-01'),
(2, 3, 1, 449.00, 'completed', '2026-03-02'),
(3, 4, 1, 389.00, 'shipped', '2026-03-02'),
(4, 5, 2, 499.98, 'completed', '2026-03-03'),
(5, 7, 2, 178.00, 'completed', '2026-03-04'),
(6, 1, 2, 399.98, 'completed', '2026-03-04'),
(7, 6, 3, 179.70, 'shipped', '2026-03-05'),
(8, 8, 1, 115.00, 'processing', '2026-03-06'),
(2, 1, 1, 199.99, 'completed', '2026-03-06'),
(3, 2, 2, 259.00, 'completed', '2026-03-07'),
(4, 8, 1, 115.00, 'completed', '2026-03-07');
"#;

fn ensure_sample_database(db_path: &std::path::Path) -> Result<(), CommandError> {
    let conn = Connection::open(db_path).map_err(|e| {
        CommandError::Internal(format!("Failed to open sample sqlite database: {}", e))
    })?;

    conn.execute_batch(SCHEMA_DDL)
        .map_err(|e| CommandError::Internal(format!("Failed to create sample schema: {}", e)))?;

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM customers", [], |r| r.get(0))
        .map_err(|e| CommandError::Internal(format!("Failed to count customers: {}", e)))?;

    if count == 0 {
        conn.execute_batch(SEED_DATA)
            .map_err(|e| CommandError::Internal(format!("Failed to seed sample data: {}", e)))?;
    }

    Ok(())
}

pub(crate) async fn init_sample_database_impl(
    state: &AppState,
) -> Result<ConnectionConfig, CommandError> {
    let data_dir = Store::default_app_data_dir()
        .map_err(|e| CommandError::Internal(format!("Failed to determine app data dir: {}", e)))?;
    let db_path = data_dir.join(SAMPLE_FILE_NAME);

    ensure_sample_database(&db_path)?;

    let config = ConnectionConfig {
        id: SAMPLE_CONN_ID.to_string(),
        name: SAMPLE_DB_NAME.to_string(),
        database_type: "sqlite".to_string(),
        database: Some(db_path.to_string_lossy().to_string()),
        schema: None,
        host: None,
        port: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 10,
        max_pool_size: 10,
        ssh_tunnel: None,
        color_tag: None,
        group: Some("Samples".to_string()),
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: true,
    };

    state
        .store
        .save_connection(config.clone())
        .await
        .cmd_err("init_sample_database")?;

    tracing::info!("Initialized sample sqlite database connection successfully");
    Ok(config)
}

#[tauri::command]
pub async fn init_sample_database(
    state: State<'_, AppState>,
) -> Result<ConnectionConfig, CommandError> {
    init_sample_database_impl(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state::TestAppState;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn init_sample_database_creates_schema_and_seeds_data() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let temp = tempfile::tempdir().expect("tempdir");
        std::env::set_var("DATAZEN_DATA_DIR", temp.path());

        let test = TestAppState::new().await;
        let config = init_sample_database_impl(&test.state)
            .await
            .expect("init sample database");

        assert_eq!(config.id, SAMPLE_CONN_ID);
        assert_eq!(config.name, SAMPLE_DB_NAME);
        assert_eq!(config.database_type, "sqlite");
        assert!(config.pinned);
        assert_eq!(config.group.as_deref(), Some("Samples"));

        let db_path = temp.path().join(SAMPLE_FILE_NAME);
        assert!(db_path.exists());

        let conn = Connection::open(&db_path).expect("open sample db");
        let customers: i64 = conn
            .query_row("SELECT COUNT(*) FROM customers", [], |r| r.get(0))
            .expect("count customers");
        let products: i64 = conn
            .query_row("SELECT COUNT(*) FROM products", [], |r| r.get(0))
            .expect("count products");
        let orders: i64 = conn
            .query_row("SELECT COUNT(*) FROM orders", [], |r| r.get(0))
            .expect("count orders");

        assert_eq!(customers, 8);
        assert_eq!(products, 8);
        assert_eq!(orders, 12);

        let saved = test
            .state
            .store
            .get_connection(SAMPLE_CONN_ID)
            .await
            .expect("saved connection");
        assert_eq!(saved.id, SAMPLE_CONN_ID);

        init_sample_database_impl(&test.state)
            .await
            .expect("idempotent init");
        let customers_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM customers", [], |r| r.get(0))
            .expect("count customers after re-init");
        assert_eq!(customers_after, 8);

        std::env::remove_var("DATAZEN_DATA_DIR");
    }
}
