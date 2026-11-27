use super::*;
use datazen_driver_api::async_trait;
use datazen_driver_api::PromptTemplate;

#[test]
fn test_embedded_defaults_cover_all_scenarios() {
    for scenario in PromptScenario::all() {
        let en = embedded_default(*scenario);
        assert!(!en.is_empty(), "{scenario:?} en prompt is empty");
    }
}

#[test]
fn test_render_template() {
    let template = "Database: {{db_type}}\nSchema:\n{{schema}}";
    let mut vars = HashMap::new();
    vars.insert("db_type", "PostgreSQL");
    vars.insert("schema", "users (id int PK)");
    let result = render_template(template, &vars);
    assert!(result.contains("PostgreSQL"));
    assert!(result.contains("users (id int PK)"));
}

#[test]
fn test_render_template_missing_var() {
    let template = "DB: {{db_type}} Version: {{version}}";
    let mut vars = HashMap::new();
    vars.insert("db_type", "MySQL");
    let result = render_template(template, &vars);
    assert!(result.contains("MySQL"));
    assert!(!result.contains("{{version}}"));
}

#[tokio::test]
async fn test_resolver_default_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(tmp.path(), None);
    let result = resolver.resolve(PromptScenario::Nl2Sql, None, "en").await;
    assert!(result.contains("SQL expert"));
}

#[tokio::test]
async fn test_resolver_user_override() {
    let tmp = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(tmp.path(), None);

    resolver
        .set_override(PromptOverrideEntry {
            driver_type: "*".into(),
            scenario: PromptScenario::Chat,
            system: "Custom English prompt".into(),
        })
        .await
        .unwrap();

    let result = resolver.resolve(PromptScenario::Chat, None, "zh-CN").await;
    assert_eq!(result, "Custom English prompt");

    let result = resolver.resolve(PromptScenario::Chat, None, "en").await;
    assert_eq!(result, "Custom English prompt");
}

#[tokio::test]
async fn test_resolver_remove_override() {
    let tmp = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(tmp.path(), None);

    resolver
        .set_override(PromptOverrideEntry {
            driver_type: "*".into(),
            scenario: PromptScenario::Chat,
            system: "Custom".into(),
        })
        .await
        .unwrap();

    resolver
        .remove_override("*", PromptScenario::Chat)
        .await
        .unwrap();

    let result = resolver.resolve(PromptScenario::Chat, None, "en").await;
    assert!(result.contains("database assistant"));
}

#[tokio::test]
async fn test_resolver_persist_and_reload() {
    let tmp = tempfile::tempdir().unwrap();

    {
        let resolver = PromptResolver::new(tmp.path(), None);
        resolver
            .set_override(PromptOverrideEntry {
                driver_type: "PostgreSQL".into(),
                scenario: PromptScenario::Nl2Sql,
                system: "PG specific".into(),
            })
            .await
            .unwrap();
    }

    {
        let resolver = PromptResolver::new(tmp.path(), None);
        resolver.load().await.unwrap();
        let overrides = resolver.get_all_overrides().await;
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].driver_type, "PostgreSQL");
        assert_eq!(overrides[0].system, "PG specific");
    }
}

#[tokio::test]
async fn test_list_prompts() {
    let tmp = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(tmp.path(), None);
    let prompts = resolver.list_prompts(None).await;
    assert_eq!(prompts.len(), PromptScenario::all().len());
    for p in &prompts {
        assert_eq!(p.source, PromptSource::Default);
        assert!(!p.default_system.is_empty());
        assert!(!p.system.is_empty());
    }
}

#[tokio::test]
async fn test_load_templates_from_prompts_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let prompts_dir = tmp.path().join("prompts");
    std::fs::create_dir_all(&prompts_dir).unwrap();
    std::fs::write(prompts_dir.join("chat.md"), "Custom chat prompt from file").unwrap();

    let data_dir = tmp.path().join("data");
    let resolver = PromptResolver::new(&data_dir, Some(prompts_dir));
    resolver.load_templates().await;
    let result = resolver.resolve(PromptScenario::Chat, None, "en").await;
    assert_eq!(result, "Custom chat prompt from file");
}

#[tokio::test]
async fn test_ensure_ready_loads_templates() {
    let tmp = tempfile::tempdir().unwrap();
    let prompts_dir = tmp.path().join("prompts");
    std::fs::create_dir_all(&prompts_dir).unwrap();
    std::fs::write(prompts_dir.join("nl2sql.md"), "File-backed NL2SQL").unwrap();

    let data_dir = tmp.path().join("data");
    let resolver = PromptResolver::new(&data_dir, Some(prompts_dir));
    resolver.ensure_ready("en").await;
    let result = resolver.resolve(PromptScenario::Nl2Sql, None, "en").await;
    assert_eq!(result, "File-backed NL2SQL");
}

#[tokio::test]
async fn test_resolver_driver_specific_override() {
    let tmp = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(tmp.path(), None);
    resolver
        .set_override(PromptOverrideEntry {
            driver_type: "PostgreSQL".into(),
            scenario: PromptScenario::Chat,
            system: "PG English".into(),
        })
        .await
        .unwrap();

    let result = resolver.resolve(PromptScenario::Chat, None, "en").await;
    // Without a driver instance, driver-specific override is not matched; global default applies.
    assert!(result.contains("database assistant"));

    resolver
        .set_override(PromptOverrideEntry {
            driver_type: "*".into(),
            scenario: PromptScenario::Diagnose,
            system: "Global diagnose".into(),
        })
        .await
        .unwrap();
    let result = resolver.resolve(PromptScenario::Diagnose, None, "en").await;
    assert_eq!(result, "Global diagnose");
}

#[test]
fn test_scenario_to_key() {
    assert_eq!(scenario_to_key(PromptScenario::Nl2Sql), "nl2sql");
    assert_eq!(scenario_to_key(PromptScenario::Chat), "chat");
}

/// A stub driver for testing dialect_notes integration.
struct StubDialectDriver {
    dialect_notes: Option<String>,
    prompt_overrides: std::collections::HashMap<PromptScenario, PromptTemplate>,
}

impl StubDialectDriver {
    fn new(dialect_notes: Option<String>) -> Self {
        Self {
            dialect_notes,
            prompt_overrides: std::collections::HashMap::new(),
        }
    }

    fn with_prompt_override(
        mut self,
        scenario: PromptScenario,
        system: &str,
        dialect_notes: Option<String>,
    ) -> Self {
        self.prompt_overrides.insert(
            scenario,
            PromptTemplate {
                system: system.to_string(),
                dialect_notes,
            },
        );
        self
    }
}

#[async_trait]
impl DatabaseDriver for StubDialectDriver {
    fn driver_type(&self) -> datazen_driver_api::DatabaseType {
        "stub-dialect".into()
    }

    fn dialect_notes(&self) -> Option<String> {
        self.dialect_notes.clone()
    }

    fn prompt_overrides(&self) -> std::collections::HashMap<PromptScenario, PromptTemplate> {
        self.prompt_overrides.clone()
    }

    async fn connect(
        &self,
        _config: &datazen_driver_api::ConnectionConfig,
    ) -> Result<datazen_driver_api::ConnectionHandle, datazen_driver_api::DriverError> {
        Ok(datazen_driver_api::ConnectionHandle {
            id: "c".into(),
            pool_id: "p".into(),
        })
    }

    async fn test_connection(
        &self,
        _config: &datazen_driver_api::ConnectionConfig,
    ) -> Result<datazen_driver_api::ServerInfo, datazen_driver_api::DriverError> {
        Ok(datazen_driver_api::ServerInfo {
            server_version: String::new(),
            server_type: self.driver_type(),
        })
    }

    async fn disconnect(
        &self,
        _handle: datazen_driver_api::ConnectionHandle,
    ) -> Result<(), datazen_driver_api::DriverError> {
        Ok(())
    }

    async fn get_databases(
        &self,
        _handle: &datazen_driver_api::ConnectionHandle,
    ) -> Result<Vec<String>, datazen_driver_api::DriverError> {
        Ok(vec![])
    }

    async fn get_tables(
        &self,
        _handle: &datazen_driver_api::ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<datazen_driver_api::TableInfo>, datazen_driver_api::DriverError> {
        Ok(vec![])
    }

    async fn get_table_schema(
        &self,
        _handle: &datazen_driver_api::ConnectionHandle,
        _table: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<datazen_driver_api::TableSchema, datazen_driver_api::DriverError> {
        Ok(datazen_driver_api::TableSchema {
            table_name: String::new(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        })
    }

    async fn query(
        &self,
        _handle: &datazen_driver_api::ConnectionHandle,
        _sql: &str,
    ) -> Result<datazen_driver_api::QueryResult, datazen_driver_api::DriverError> {
        Ok(datazen_driver_api::QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            execution_time_ms: 0,
        })
    }

    async fn query_multi(
        &self,
        _handle: &datazen_driver_api::ConnectionHandle,
        _sql: &str,
        _limit: Option<u32>,
    ) -> Result<datazen_driver_api::MultiQueryResult, datazen_driver_api::DriverError> {
        Ok(datazen_driver_api::MultiQueryResult {
            results: vec![],
            total_time_ms: 0,
        })
    }

    async fn query_with_params(
        &self,
        _handle: &datazen_driver_api::ConnectionHandle,
        _sql: &str,
        _params: &[datazen_driver_api::Value],
    ) -> Result<datazen_driver_api::QueryResult, datazen_driver_api::DriverError> {
        Ok(datazen_driver_api::QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            execution_time_ms: 0,
        })
    }

    async fn execute(
        &self,
        _handle: &datazen_driver_api::ConnectionHandle,
        _sql: &str,
    ) -> Result<u64, datazen_driver_api::DriverError> {
        Ok(0)
    }

    async fn cancel_query(
        &self,
        _handle: &datazen_driver_api::ConnectionHandle,
    ) -> Result<(), datazen_driver_api::DriverError> {
        Ok(())
    }
}

#[tokio::test]
async fn test_resolve_with_dialect_notes_builtin_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(tmp.path(), None);
    let driver = StubDialectDriver::new(Some(
        "PostgreSQL uses LIMIT/OFFSET, ILIKE for case-insensitive".into(),
    ));
    let result = resolver
        .resolve_with_dialect(PromptScenario::Nl2Sql, Some(&driver), "en")
        .await;
    assert!(result.contains("SQL expert"));
    assert!(result.contains("PostgreSQL uses LIMIT/OFFSET"));
}

#[tokio::test]
async fn test_resolve_with_dialect_notes_from_prompt_template() {
    let tmp = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(tmp.path(), None);
    let driver = StubDialectDriver::new(Some("Global notes".into())).with_prompt_override(
        PromptScenario::Chat,
        "Custom prompt with {{dialect_notes}}",
        Some("Template-level notes".into()),
    );
    let result = resolver
        .resolve_with_dialect(PromptScenario::Chat, Some(&driver), "en")
        .await;
    assert_eq!(result, "Custom prompt with Template-level notes");
}

#[tokio::test]
async fn test_resolve_with_dialect_no_notes_removes_placeholder() {
    let tmp = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(tmp.path(), None);
    let driver = StubDialectDriver::new(None);
    let result = resolver
        .resolve_with_dialect(PromptScenario::Nl2Sql, Some(&driver), "en")
        .await;
    // {{dialect_notes}} should be removed when no notes available
    assert!(!result.contains("{{dialect_notes}}"));
    assert!(result.contains("SQL expert"));
}

#[tokio::test]
async fn test_dialect_notes_in_prompt_template_struct() {
    let tpl = PromptTemplate {
        system: "Hello {{dialect_notes}}".into(),
        dialect_notes: Some("PG specific".into()),
    };
    assert_eq!(tpl.dialect_notes.as_deref(), Some("PG specific"));
}

// A real temp-file writer that fails after putting a partial prefix on disk.
// Do not chmod the parent to reproduce this: that is nondeterministic when
// tests run with an elevated user and does not prove what happens after I/O.
fn fail_after_partial_write(path: &Path, bytes: &[u8]) -> io::Result<tempfile::NamedTempFile> {
    stage_overrides_with(path, bytes, |staged, bytes| {
        assert!(bytes.len() > 8);
        staged.write_all(&bytes[..8])?;
        assert_eq!(staged.metadata()?.len(), 8);
        Err(io::Error::other("fixture partial staging write"))
    })
}

fn prompt_entry(scope: &str, scenario: PromptScenario, system: &str) -> PromptOverrideEntry {
    PromptOverrideEntry { driver_type: scope.into(), scenario, system: system.into() }
}

#[tokio::test]
async fn failed_partial_save_or_remove_never_replaces_prior_file_or_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(dir.path(), None);
    let path = dir.path().join("prompt_overrides.json");
    let original = "prior first line\n原始第二行";
    resolver.set_override(prompt_entry("*", PromptScenario::Chat, original)).await.unwrap();
    let bytes = fs::read(&path).unwrap();

    let save = resolver.commit_overrides(
        |snapshot| snapshot[0].system = "must not commit".into(),
        fail_after_partial_write,
    ).await;
    assert_eq!(save.unwrap_err(), "fixture partial staging write");
    let remove = resolver.commit_overrides(
        |snapshot| snapshot.clear(),
        fail_after_partial_write,
    ).await;
    assert_eq!(remove.unwrap_err(), "fixture partial staging write");

    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "failed stages must clean up");
    assert_eq!(resolver.get_all_overrides().await[0].system, original);
    let reopened = PromptResolver::new(dir.path(), None);
    reopened.load().await.unwrap();
    assert_eq!(reopened.get_all_overrides().await[0].system, original);
}

#[tokio::test]
async fn concurrent_writes_inside_one_resolver_preserve_different_scopes() {
    let dir = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(dir.path(), None);
    let (left, right) = tokio::join!(
        resolver.set_override(prompt_entry("*", PromptScenario::Chat, "global")),
        resolver.set_override(prompt_entry("sqlite", PromptScenario::Chat, "sqlite-only")),
    );
    left.unwrap();
    right.unwrap();
    let reopened = PromptResolver::new(dir.path(), None);
    reopened.load().await.unwrap();
    let entries = reopened.get_all_overrides().await;
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.driver_type == "*" && e.system == "global"));
    assert!(entries.iter().any(|e| e.driver_type == "sqlite" && e.system == "sqlite-only"));
    let (remove, other) = tokio::join!(
        resolver.remove_override("sqlite", PromptScenario::Chat),
        resolver.set_override(prompt_entry("*", PromptScenario::Chat, "global revisited")),
    );
    remove.unwrap();
    other.unwrap();
    reopened.load().await.unwrap();
    let entries = reopened.get_all_overrides().await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].system, "global revisited");
}

#[cfg(unix)]
#[tokio::test]
async fn atomic_replacement_keeps_existing_unix_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let resolver = PromptResolver::new(dir.path(), None);
    let path = dir.path().join("prompt_overrides.json");
    resolver.set_override(prompt_entry("*", PromptScenario::Chat, "old")).await.unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    resolver.set_override(prompt_entry("*", PromptScenario::Chat, "new")).await.unwrap();
    assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o640);
}
