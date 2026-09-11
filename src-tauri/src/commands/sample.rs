//! Sample dataset seeding for the onboarding wizard (startup journey Q4).
//!
//! `seed_sample_db` creates `{appData}/sample/playground.db` with an English
//! `demo_sales(region, amount, quarter)` table (4 regions × 2 quarters).
//! Idempotent: returns the existing path when the db file already exists.

use super::error::CommandError;
use tauri::{AppHandle, Manager};
use std::fs;

const SEED_SQL: &str =
    "CREATE TABLE IF NOT EXISTS demo_sales (region TEXT, amount REAL, quarter TEXT);
     INSERT INTO demo_sales VALUES
     ('East',1200,'Q1'),('East',950,'Q2'),
     ('North',800,'Q1'),('North',1100,'Q2'),
     ('South',650,'Q1'),('South',900,'Q2'),
     ('West',750,'Q1'),('West',950,'Q2');";

pub(crate) fn seed_sample_db_at(data_dir: &std::path::Path) -> Result<String, CommandError> {
    let dir = data_dir.join("sample");
    fs::create_dir_all(&dir).map_err(CommandError::from)?;
    let db_path = dir.join("playground.db");
    if db_path.exists() {
        return Ok(db_path.to_string_lossy().to_string());
    }
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    conn.execute_batch(SEED_SQL)
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    Ok(db_path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn seed_sample_db(app: AppHandle) -> Result<String, CommandError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| CommandError::Internal(format!("seed_sample_db: {e}")))?;
    seed_sample_db_at(&data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_creates_playground_db_with_eight_rows() {
        let dir = tempfile::tempdir().unwrap();
        let first = seed_sample_db_at(dir.path()).unwrap();
        assert!(first.ends_with("playground.db"));

        let conn = rusqlite::Connection::open(dir.path().join("sample/playground.db")).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM demo_sales", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 8);

        let east_q1: f64 = conn
            .query_row(
                "SELECT amount FROM demo_sales WHERE region='East' AND quarter='Q1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(east_q1, 1200.0);

        let regions: Vec<String> = conn
            .prepare("SELECT DISTINCT region FROM demo_sales ORDER BY region")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(regions, vec!["East", "North", "South", "West"]);

        // Idempotent: second call keeps existing rows (no duplicates).
        let second = seed_sample_db_at(dir.path()).unwrap();
        assert_eq!(first, second);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM demo_sales", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 8);
    }
}
