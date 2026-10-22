//! Product command bodies + real SQLite + synthetic, digest-pinned POSIX probe.
//! No env, database server, real pg_dump/mysqldump or arbitrary client process.
use super::*;
use crate::platform::{PlatformAdapter, PlatformEntry};
use datazen_platform_api::{
    dto::{execution::EffectOutcome, job::JobState},
    id::PrincipalId,
};
use sha2::{Digest, Sha256};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

struct Fixture {
    directory: tempfile::TempDir,
    state: AppState,
    manifest: serde_json::Value,
    selection: NativeToolSelection,
    marker: PathBuf,
}
async fn state(path: &Path) -> AppState {
    let store = Arc::new(
        crate::store::DesktopRepository::init_with_path(path, crate::store::KeyBackend::File)
            .await
            .unwrap(),
    );
    crate::finish_app_state(
        store,
        Arc::new(datazen_runtime::driver_registry::init_drivers()),
        Arc::new(crate::transfer::adapter_registry::SyncAdapterRegistry::new()),
        None,
    )
}
fn attach(
    state: &mut AppState,
    manifest: &serde_json::Value,
) -> Result<(), datazen_application::error::ApiError> {
    let mut adapter = PlatformAdapter::new_launch(&uuid::Uuid::new_v4().to_string())?;
    adapter.inject(state.store.clone(), state.driver_registry.clone())?;
    adapter.replace_native_fixture(&manifest.to_string())?;
    state.platform = PlatformEntry::ready(adapter);
    Ok(())
}
fn quoted(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}
impl Fixture {
    async fn new(mysql: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut state = state(directory.path()).await;
        let binary = directory
            .path()
            .canonicalize()
            .unwrap()
            .join("synthetic-probe");
        let marker = binary.with_file_name("invocations");
        let version = "synthetic-product-1";
        let line = if mysql {
            format!("mysqldump {version}")
        } else {
            format!("pg_dump (PostgreSQL) {version}")
        };
        // This process only appends an owned marker and prints its fixed version.
        // Scripts are permitted solely by the existing cfg(test) adapter rule.
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\nprintf x >> {}\nprintf '%s\\n' '{}'\n",
                quoted(&marker),
                line
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o500)).unwrap();
        let digest = format!("{:x}", Sha256::digest(std::fs::read(&binary).unwrap()));
        let selection = NativeToolSelection {
            tool_id: if mysql {
                "mysql.mysqldump-version"
            } else {
                "postgres.pg-dump-version"
            }
            .into(),
            version: version.into(),
        };
        let manifest = serde_json::json!([{"contract":"versionProbeV1","toolId":selection.tool_id,"version":selection.version,"executable":binary,"sha256":digest,"expectedVersionLine":line,"maxProcesses":1,"maxDatabaseSockets":0}]);
        attach(&mut state, &manifest).unwrap();
        Self {
            directory,
            state,
            manifest,
            selection,
            marker,
        }
    }
    fn request(&self, key: uuid::Uuid) -> NativeProbeRequest {
        NativeProbeRequest {
            selection: self.selection.clone(),
            idempotency_key: key.to_string(),
        }
    }
    fn payload(&self) -> serde_json::Value {
        let mut payload = NativeToolJobHandler::plan_payload(&self.selection);
        payload["planDigest"] = self
            .state
            .platform_require()
            .unwrap()
            .native_tools()
            .unwrap()
            .freeze(&self.selection)
            .unwrap()
            .into();
        payload
    }
    fn calls(&self) -> usize {
        match std::fs::read(&self.marker) {
            Ok(bytes) => bytes.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
            Err(_) => panic!("synthetic marker unavailable"),
        }
    }
}
async fn terminal(state: &AppState, id: &JobId) -> JobDetails {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let detail = get(state, id.as_str().into()).await.unwrap();
            if detail.job.state.is_terminal() {
                break detail;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("synthetic native Job did not terminalize")
}

#[tokio::test]
async fn native_product_probe_freezes_sqlite_contract_and_same_key_never_dispatches_twice() {
    for mysql in [false, true] {
        let mut fixture = Fixture::new(mysql).await;
        let advertised = choices(&fixture.state).await.unwrap();
        assert_eq!(advertised.len(), 1);
        let public = serde_json::to_value(&advertised).unwrap();
        assert_eq!(public[0]["operation"], "versionProbe");
        assert!(public[0].get("executable").is_none());
        let key = uuid::Uuid::new_v4();
        let first = submit(&fixture.state, fixture.request(key)).await.unwrap();
        let retry = submit(&fixture.state, fixture.request(key)).await.unwrap();
        assert_eq!(retry.job_id, first.job_id);
        let detail = terminal(&fixture.state, &first.job_id).await;
        assert_eq!(detail.job.state, JobState::Succeeded);
        assert_eq!(detail.job.effect_outcome, Some(EffectOutcome::Completed));
        assert_eq!(fixture.calls(), 1);
        let ctx = context(&fixture.state).await.unwrap();
        let record = fixture
            .state
            .desktop_job_host
            .get(&ctx, first.job_id.clone())
            .await
            .unwrap();
        assert_eq!(
            record.definition.payload["nativeToolSelection"],
            serde_json::to_value(&fixture.selection).unwrap()
        );
        let expected = fixture
            .state
            .platform_require()
            .unwrap()
            .native_tools()
            .unwrap()
            .freeze(&fixture.selection)
            .unwrap();
        assert_eq!(record.definition.payload["planDigest"], expected);
        assert_eq!(detail.plan_digest.as_deref(), Some(expected.as_str()));
        let fields = record.definition.payload.as_object().unwrap();
        assert_eq!(fields.len(), 5);
        assert!(fields.keys().all(|key| matches!(
            key.as_str(),
            "planVersion"
                | "handlerVersion"
                | "checkpointVersion"
                | "nativeToolSelection"
                | "planDigest"
        )));
        assert!(detail
            .domain_results
            .iter()
            .any(|result| result.result_code == "nativeCleanupConfirmed"));
        // A separately opened SQLite store retains the exact frozen payload.
        let reopened = crate::store::DesktopRepository::init_with_path(
            fixture.directory.path(),
            crate::store::KeyBackend::File,
        )
        .await
        .unwrap();
        let host = crate::build_desktop_job_host(reopened.app_db());
        let reread = host.get(&ctx, first.job_id.clone()).await.unwrap();
        assert_eq!(reread.definition.payload, record.definition.payload);
        assert_eq!(
            submit(&fixture.state, fixture.request(key))
                .await
                .unwrap()
                .job_id,
            first.job_id
        );
        assert_eq!(fixture.calls(), 1);
        // Same approved id/version but changed deployment contract must conflict,
        // not replay under the new digest or start a second physical invocation.
        fixture.manifest[0]["expectedVersionLine"] = format!(
            "{} revised",
            fixture.manifest[0]["expectedVersionLine"].as_str().unwrap()
        )
        .into();
        attach(&mut fixture.state, &fixture.manifest).unwrap();
        assert!(submit(&fixture.state, fixture.request(key)).await.is_err());
        assert_eq!(fixture.calls(), 1);
        assert_eq!(list(&fixture.state).await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn native_product_default_and_invalid_deployment_deny_before_admission() {
    let fixture = Fixture::new(false).await;
    for malformed in [
        serde_json::json!({}),
        {
            let mut m = fixture.manifest.clone();
            m[0]["contract"] = "nativeBackup".into();
            m
        },
        {
            let mut m = fixture.manifest.clone();
            m[0]["maxProcesses"] = 2.into();
            m
        },
        {
            let mut m = fixture.manifest.clone();
            m[0]["maxDatabaseSockets"] = 1.into();
            m
        },
        {
            let mut m = fixture.manifest.clone();
            m[0]["sha256"] = "invalid".into();
            m
        },
        {
            let mut m = fixture.manifest.clone();
            m[0]["sha256"] = "0".repeat(64).into();
            m
        },
        {
            let mut m = fixture.manifest.clone();
            m[0]["env"] = serde_json::json!({"SYNTHETIC":"forbidden"});
            m
        },
        {
            let mut m = fixture.manifest.clone();
            m[0]["toolId"] = "postgres.pg-dump".into();
            m
        },
        serde_json::json!([fixture.manifest[0].clone(), fixture.manifest[0].clone()]),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut denied_state = state(directory.path()).await;
        assert!(choices(&denied_state).await.unwrap().is_empty());
        assert!(attach(&mut denied_state, &malformed).is_err());
        assert!(choices(&denied_state).await.unwrap().is_empty());
        assert!(submit(&denied_state, fixture.request(uuid::Uuid::new_v4()))
            .await
            .is_err());
        assert!(list(&denied_state).await.unwrap().is_empty());
    }
    assert_eq!(fixture.calls(), 0);
    let key = uuid::Uuid::new_v4();
    let base = serde_json::json!({"selection":fixture.selection,"idempotencyKey":key});
    for field in [
        "path",
        "argv",
        "env",
        "cwd",
        "password",
        "principalId",
        "connectionId",
        "maxDatabaseSockets",
    ] {
        let mut value = base.clone();
        value[field] = serde_json::json!("synthetic-private");
        assert!(serde_json::from_value::<NativeProbeRequest>(value).is_err());
        let mut value = base.clone();
        value["selection"][field] = serde_json::json!("synthetic-private");
        assert!(serde_json::from_value::<NativeProbeRequest>(value).is_err());
    }
    let mut invalid_key = fixture.request(key);
    invalid_key.idempotency_key = "not-a-uuid".into();
    assert!(submit(&fixture.state, invalid_key).await.is_err());
    let mut unknown = fixture.request(key);
    unknown.selection.version = "unregistered".into();
    assert!(submit(&fixture.state, unknown).await.is_err());
    assert!(list(&fixture.state).await.unwrap().is_empty());
    assert_eq!(fixture.calls(), 0);
}

async fn seed(
    state: &AppState,
    ctx: &RequestContext,
    id: &str,
    kind: &str,
    payload: serde_json::Value,
) -> JobId {
    let id = JobId::new(id);
    state
        .desktop_job_host
        .accept(
            ctx,
            JobDefinition {
                job_id: id.clone(),
                organization_id: ctx.organization_id.clone(),
                kind: kind.into(),
                owner: OwnerRef::Job {
                    job_id: id.clone(),
                    stage_id: StageId::new("native-probe"),
                },
                payload,
                created_at: state.desktop_job_host.now(),
            },
            &IdempotencyKey::new(uuid::Uuid::new_v4().to_string()),
        )
        .await
        .unwrap();
    id
}
#[tokio::test]
async fn native_product_get_list_cancel_enforce_sqlite_owner_and_kind() {
    let fixture = Fixture::new(false).await;
    let owner = context(&fixture.state).await.unwrap();
    let mut foreign = owner.clone();
    foreign.principal_id = PrincipalId::new("synthetic-other-native-owner");
    let payload = fixture.payload();
    let owned = seed(
        &fixture.state,
        &owner,
        "owned-native",
        KIND,
        payload.clone(),
    )
    .await;
    let private = seed(&fixture.state, &foreign, "foreign-native", KIND, payload).await;
    let other = seed(
        &fixture.state,
        &owner,
        "other-kind",
        "schemaDiffPrepare",
        serde_json::json!({"planVersion":1,"handlerVersion":1,"checkpointVersion":1}),
    )
    .await;
    assert_eq!(
        get(&fixture.state, owned.as_str().into())
            .await
            .unwrap()
            .job
            .state,
        JobState::Queued
    );
    let visible = list(&fixture.state).await.unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].job_id, owned);
    for id in [&private, &other] {
        assert!(get(&fixture.state, id.as_str().into()).await.is_err());
        assert!(cancel(&fixture.state, id.as_str().into()).await.is_err());
    }
    assert_eq!(
        fixture
            .state
            .desktop_job_host
            .get(&foreign, private)
            .await
            .unwrap()
            .view
            .state,
        JobState::Queued
    );
    assert_eq!(
        fixture
            .state
            .desktop_job_host
            .get(&owner, other)
            .await
            .unwrap()
            .view
            .state,
        JobState::Queued
    );
    let cancelled = cancel(&fixture.state, owned.as_str().into()).await.unwrap();
    assert_eq!(cancelled.state, JobState::Cancelled);
    assert_eq!(cancelled.effect_outcome, Some(EffectOutcome::NotStarted));
    assert_eq!(fixture.calls(), 0);
}

#[tokio::test]
async fn native_product_sqlite_projection_refuses_missing_digest_private_and_wrong_kind_fields() {
    let fixture = Fixture::new(false).await;
    let ctx = context(&fixture.state).await.unwrap();
    let payload = fixture.payload();
    for (kind, candidate) in [
        (KIND, {
            let mut p = payload.clone();
            p.as_object_mut().unwrap().remove("planDigest");
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["planDigest"] = serde_json::Value::Null;
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["planDigest"] = 12.into();
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["planDigest"] = "".into();
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["planDigest"] = "sha256:a".into();
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["planDigest"] = format!("sha256:{}", "g".repeat(64)).into();
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["planDigest"] = format!("sha256:{}", "A".repeat(64)).into();
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p.as_object_mut().unwrap().remove("nativeToolSelection");
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["nativeToolSelection"]["path"] = "synthetic-private-path".into();
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["password"] = "synthetic-private-value".into();
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["nativeToolSelection"]["version"] = "".into();
            p
        }),
        (KIND, {
            let mut p = payload.clone();
            p["nativeToolSelection"]["toolId"] = "mysql.mysqldump".into();
            p
        }),
        ("schemaDiffPrepare", payload),
    ] {
        let id = JobId::new(uuid::Uuid::new_v4().to_string());
        assert!(fixture
            .state
            .desktop_job_host
            .accept(
                &ctx,
                JobDefinition {
                    job_id: id.clone(),
                    organization_id: ctx.organization_id.clone(),
                    kind: kind.into(),
                    owner: OwnerRef::Job {
                        job_id: id,
                        stage_id: StageId::new("native-probe")
                    },
                    payload: candidate,
                    created_at: fixture.state.desktop_job_host.now(),
                },
                &IdempotencyKey::new(uuid::Uuid::new_v4().to_string())
            )
            .await
            .is_err());
    }
    assert!(fixture
        .state
        .desktop_job_host
        .list(
            &ctx,
            JobFilter {
                organization_id: ctx.organization_id.clone(),
                states: Vec::new(),
                owner: None,
                after: None,
                limit: None,
            },
        )
        .await
        .unwrap()
        .is_empty());
    assert_eq!(fixture.calls(), 0);
}
