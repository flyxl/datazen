mod common;

use common::JobAdapter;
use datazen_ui_api::{AdapterDelivery, AdapterJob, DesktopAdapter};

#[test]
fn job_adapter_delivers_only_the_work_it_accepts() {
    let mut host = JobAdapter::new();
    assert!(!host.begin_job(AdapterJob::ReadQueryHistory {
        generation: 1,
        connection_id: "connection".into(),
    }));
    assert!(host.begin_job(AdapterJob::RunQuery {
        generation: 2,
        sql: "SELECT 1".into()
    }));

    assert!(host.poll_jobs().is_empty());
    assert!(matches!(
        host.poll_jobs().as_slice(),
        [AdapterDelivery::QueryPage {
            generation: 2,
            page: Ok(_),
            execution_stats: None,
            csv_source: None,
        }]
    ));
}
