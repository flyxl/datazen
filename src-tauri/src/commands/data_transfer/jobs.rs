//! In-process Data Transfer cancel flags keyed by job id.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use tokio::sync::Mutex;

static JOBS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub async fn ensure_job(job_id: &str) -> Arc<AtomicBool> {
    let mut jobs = JOBS.lock().await;
    jobs.entry(job_id.to_string())
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

pub async fn cancel_job(job_id: &str) -> bool {
    let flag = ensure_job(job_id).await;
    flag.store(true, Ordering::SeqCst);
    true
}

pub async fn remove_job(job_id: &str) {
    JOBS.lock().await.remove(job_id);
}
