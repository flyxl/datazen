use super::{string, Fixture};
use datazen_driver_api::{DatabaseDriver, SqlTarget, Value};
use datazen_driver_postgres::PostgresDriver;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_foreign_browsing_shares_the_winner_and_closes_evicted_postgres_pools() {
    let mut fixture = Fixture::open().await;
    let primary = fixture
        .config
        .database
        .as_deref()
        .expect("marked primary database");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let targets: Vec<_> = (0..9)
        .map(|n| format!("dz_fixture_pool_{tag}_{n}"))
        .collect();
    for target in &targets {
        sqlx::query(&format!("CREATE DATABASE {target}"))
            .execute(&mut fixture.oracle)
            .await
            .expect("isolated foreign browse catalog");
    }
    let driver = Arc::new(PostgresDriver::new());
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("pooled primary handle");
    let barrier = Arc::new(tokio::sync::Barrier::new(13));
    let mut workers = Vec::new();
    for _ in 0..12 {
        let driver = Arc::clone(&driver);
        let barrier = Arc::clone(&barrier);
        let handle = handle.clone();
        let target = targets[0].clone();
        workers.push(tokio::spawn(async move {
            barrier.wait().await;
            driver
                .query_at(
                    &handle,
                    "SELECT current_database()::text, pg_backend_pid()",
                    SqlTarget::new(Some(&target), None),
                )
                .await
        }));
    }
    barrier.wait().await;
    let mut failures = Vec::new();
    let mut pids = std::collections::HashSet::new();
    for worker in workers {
        match tokio::time::timeout(Duration::from_secs(8), worker)
            .await
            .expect("worker not stalled")
            .expect("active foreign browse")
        {
            Ok(result) => match result.rows[0].as_slice() {
                [Some(Value::String(name)), Some(Value::Integer(pid))] if name == &targets[0] => {
                    pids.insert(*pid);
                }
                other => failures.push(format!("unmatched target: {other:?}")),
            },
            Err(error) => failures.push(error.to_string()),
        }
    }
    assert!(
        failures.is_empty(),
        "parallel readers all use the winning live foreign pool: {failures:?}"
    );
    assert!(
        !pids.is_empty() && pids.len() <= 2,
        "all reads are genuinely served by at most two cached backends"
    );
    let independent = fixture.connect_to(&targets[0]).await;
    drop(independent);
    assert_eq!(
        driver
            .open_databases(&handle)
            .await
            .expect("one actual foreign cache entry"),
        vec![targets[0].clone()]
    );

    let inspect = async |target: &str| {
        let observed = driver
            .query_at(
                &handle,
                "SELECT current_database()::text",
                SqlTarget::new(Some(target), None),
            )
            .await
            .expect("a new physical PostgreSQL database is opened");
        assert_eq!(observed.rows, vec![vec![string(target)]]);
    };
    for target in &targets[1..8] {
        inspect(target).await;
    }
    inspect(&targets[0]).await; // keep the previously oldest database alive
    inspect(&targets[8]).await;
    let open = driver
        .open_databases(&handle)
        .await
        .expect("LRU foreign pool list");
    assert_eq!(open.len(), 8);
    assert!(!open.contains(&targets[1]));
    assert!(open.contains(&targets[0]) && open.contains(&targets[8]));
    assert!(
        !open.iter().any(|name| name == primary),
        "the primary resource is not a foreign pool"
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let connections: i64 =
                sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=$1")
                    .bind(&targets[1])
                    .fetch_one(&mut fixture.oracle)
                    .await
                    .expect("independent physical eviction proof");
            if connections == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("evicted database socket actually closes");
    assert!(driver
        .close_database(&handle, &targets[8])
        .await
        .expect("close last foreign pool"));
    assert!(!driver
        .close_database(&handle, &targets[8])
        .await
        .expect("idempotent close"));
    assert!(driver.close_database(&handle, primary).await.is_err());
    assert_eq!(
        driver
            .query(&handle, "SELECT current_database()::text")
            .await
            .expect("primary never moves")
            .rows,
        vec![vec![string(primary)]]
    );
    driver
        .disconnect(handle)
        .await
        .expect("disconnect all foreign and primary connections");
    for target in &targets {
        sqlx::query(&format!("DROP DATABASE {target}"))
            .execute(&mut fixture.oracle)
            .await
            .expect("independent cleanup sees no leaked database sessions");
    }
}
