//! Prompt resolution with driver + user override support.
//!
//! All built-in templates are in English. Free-text output language is governed
//! by the application's active language setting injected at invocation time.
//!
//! Resolution order (highest priority first):
//! 1. User override for (driver_type, scenario) — exact match
//! 2. User override for (*, scenario) — global override
//! 3. Driver-specific prompt from `DatabaseDriver::prompt_overrides()`
//! 4. Built-in default from resource files (`resources/prompts/*.md`)
//! 5. Embedded English fallback compiled into the binary

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use datazen_driver_api::{DatabaseDriver, PromptScenario};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

/// A single user prompt override entry persisted on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOverrideEntry {
    /// `"*"` means global override for all driver types.
    pub driver_type: String,
    pub scenario: PromptScenario,
    #[serde(
        default,
        alias = "systemEn",
        alias = "system_en",
        alias = "systemZh",
        alias = "system_zh"
    )]
    pub system: String,
}

/// Persisted file format for prompt overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PromptOverridesFile {
    pub overrides: Vec<PromptOverrideEntry>,
}

/// Prompt metadata returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptInfo {
    pub scenario: PromptScenario,
    pub label: String,
    pub source: PromptSource,
    pub system: String,
    pub default_system: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptSource {
    Default,
    Driver,
    User,
}

pub struct PromptResolver {
    file_path: PathBuf,
    prompts_dir: Option<PathBuf>,
    user_overrides: RwLock<Vec<PromptOverrideEntry>>,
    /// scenario_key -> template_content
    template_cache: RwLock<HashMap<String, String>>,
    overrides_loaded: AtomicBool,
    templates_loaded: AtomicBool,
    load_lock: Mutex<()>,
}

impl PromptResolver {
    pub fn new(data_dir: &Path, prompts_dir: Option<PathBuf>) -> Self {
        Self {
            file_path: data_dir.join("prompt_overrides.json"),
            prompts_dir,
            user_overrides: RwLock::new(Vec::new()),
            template_cache: RwLock::new(HashMap::new()),
            overrides_loaded: AtomicBool::new(false),
            templates_loaded: AtomicBool::new(false),
            load_lock: Mutex::new(()),
        }
    }

    /// Load overrides + templates on first AI/prompt use.
    pub async fn ensure_ready(&self, _lang: &str) {
        self.ensure_overrides_loaded().await;
        self.ensure_templates_loaded().await;
    }

    async fn ensure_overrides_loaded(&self) {
        if self.overrides_loaded.load(Ordering::Acquire) {
            return;
        }
        let _guard = self.load_lock.lock().await;
        if self.overrides_loaded.load(Ordering::Acquire) {
            return;
        }
        if let Err(e) = self.load().await {
            tracing::warn!("Failed to load prompt overrides: {e}");
        }
        self.overrides_loaded.store(true, Ordering::Release);
    }

    async fn ensure_templates_loaded(&self) {
        if self.templates_loaded.load(Ordering::Acquire) {
            return;
        }
        let _guard = self.load_lock.lock().await;
        if self.templates_loaded.load(Ordering::Acquire) {
            return;
        }
        self.load_templates().await;
        self.templates_loaded.store(true, Ordering::Release);
    }

    pub async fn load(&self) -> Result<(), String> {
        if self.file_path.exists() {
            let data = tokio::fs::read_to_string(&self.file_path)
                .await
                .map_err(|e| e.to_string())?;
            let file: PromptOverridesFile =
                serde_json::from_str(&data).map_err(|e| e.to_string())?;
            *self.user_overrides.write().await = file.overrides;
        }
        Ok(())
    }

    /// Load prompt templates from the prompts directory.
    pub async fn load_templates(&self) {
        let Some(dir) = &self.prompts_dir else { return };
        if !dir.is_dir() {
            return;
        }

        let mut templates = HashMap::new();
        match tokio::fs::read_dir(dir).await {
            Ok(mut entries) => {
                while let Some(entry_result) = entries.next_entry().await.unwrap_or(None) {
                    let path = entry_result.path();
                    let is_template = path
                        .extension()
                        .map(|e| e == "md" || e == "txt")
                        .unwrap_or(false);
                    if is_template {
                        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                                templates.insert(stem.to_string(), content);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("[prompts] failed to read prompts dir: {e}");
            }
        }

        if !templates.is_empty() {
            tracing::info!("[prompts] loaded {} templates from disk", templates.len());
            *self.template_cache.write().await = templates;
        }
    }

    async fn save(&self) -> Result<(), String> {
        let overrides = self.user_overrides.read().await.clone();
        let file = PromptOverridesFile { overrides };
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::write(&self.file_path, json)
            .await
            .map_err(|e| e.to_string())
    }

    /// Resolve the effective system prompt for a scenario.
    ///
    /// `driver_type_name` is e.g. `"PostgreSQL"`, `"MySQL"`, etc.
    /// `{{dialect_notes}}` in the resolved prompt is replaced with the driver's
    /// dialect notes (if available) or removed entirely.
    pub async fn resolve(
        &self,
        scenario: PromptScenario,
        driver: Option<&dyn DatabaseDriver>,
        _lang: &str,
    ) -> String {
        self.resolve_with_dialect(scenario, driver, _lang).await
    }

    /// Resolve the effective system prompt with optional dialect notes.
    ///
    /// Same as [`Self::resolve`] but explicitly passes through dialect notes
    /// from the driver. This is the primary entry point used by callers that
    /// have a connected driver instance.
    pub async fn resolve_with_dialect(
        &self,
        scenario: PromptScenario,
        driver: Option<&dyn DatabaseDriver>,
        _lang: &str,
    ) -> String {
        let driver_type_name = driver.and_then(|d| {
            serde_json::to_value(&d.driver_type())
                .ok()
                .and_then(|v| v.as_str().map(String::from))
        });

        let overrides = self.user_overrides.read().await;

        // 1. User override for (driver_type, scenario)
        if let Some(ref dt) = driver_type_name {
            if let Some(entry) = overrides
                .iter()
                .find(|o| o.driver_type == *dt && o.scenario == scenario)
            {
                return entry.system.clone();
            }
        }

        // 2. User override for (*, scenario)
        if let Some(entry) = overrides
            .iter()
            .find(|o| o.driver_type == "*" && o.scenario == scenario)
        {
            return entry.system.clone();
        }

        // 3. Driver-specific prompt (may include dialect_notes)
        if let Some(d) = driver {
            let driver_prompts = d.prompt_overrides();
            if let Some(tpl) = driver_prompts.get(&scenario) {
                let mut result = tpl.system.clone();
                // Inject dialect_notes from template if present, otherwise from driver trait
                if let Some(ref template_notes) = tpl.dialect_notes {
                    result = result.replace("{{dialect_notes}}", template_notes);
                } else if let Some(driver_notes) = d.dialect_notes() {
                    result = result.replace("{{dialect_notes}}", &driver_notes);
                } else {
                    result = result.replace("{{dialect_notes}}", "");
                }
                return result;
            }
        }

        // 4. Template from files (cached)
        let key = scenario_to_key(scenario);
        let cache = self.template_cache.read().await;
        if let Some(tpl) = cache.get(&key) {
            // BUG-05: Apply dialect_notes even for cached templates (steps 3 & 5 already do this).
            let mut result = tpl.clone();
            if let Some(d) = driver {
                if let Some(notes) = d.dialect_notes() {
                    result = result.replace("{{dialect_notes}}", &notes);
                } else {
                    result = result.replace("{{dialect_notes}}", "");
                }
            } else {
                result = result.replace("{{dialect_notes}}", "");
            }
            return result;
        }
        drop(cache);

        // 5. Embedded fallback
        let mut result = embedded_default(scenario).to_string();
        // Apply dialect_notes from driver trait even for built-in templates
        if let Some(d) = driver {
            if let Some(notes) = d.dialect_notes() {
                result = result.replace("{{dialect_notes}}", &notes);
            } else {
                result = result.replace("{{dialect_notes}}", "");
            }
        } else {
            result = result.replace("{{dialect_notes}}", "");
        }
        result
    }

    /// Get all prompt infos for a specific driver type (for settings UI).
    pub async fn list_prompts(&self, driver: Option<&dyn DatabaseDriver>) -> Vec<PromptInfo> {
        let driver_type_name = driver.and_then(|d| {
            serde_json::to_value(&d.driver_type())
                .ok()
                .and_then(|v| v.as_str().map(String::from))
        });
        let driver_prompts = driver.map(|d| d.prompt_overrides()).unwrap_or_default();
        let overrides = self.user_overrides.read().await;
        let cache = self.template_cache.read().await;

        PromptScenario::all()
            .iter()
            .map(|&scenario| {
                let key = scenario_to_key(scenario);
                let fallback = embedded_default(scenario).to_string();
                let default_system = cache.get(&key).cloned().unwrap_or(fallback);
                let mut source = PromptSource::Default;
                let mut system = default_system.clone();

                // Check driver override
                if let Some(tpl) = driver_prompts.get(&scenario) {
                    source = PromptSource::Driver;
                    system = tpl.system.clone();
                }

                // Check user override (exact driver match)
                if let Some(ref dt) = driver_type_name {
                    if let Some(entry) = overrides
                        .iter()
                        .find(|o| o.driver_type == *dt && o.scenario == scenario)
                    {
                        source = PromptSource::User;
                        system = entry.system.clone();
                    }
                }

                // Check user override (global)
                if source != PromptSource::User {
                    if let Some(entry) = overrides
                        .iter()
                        .find(|o| o.driver_type == "*" && o.scenario == scenario)
                    {
                        source = PromptSource::User;
                        system = entry.system.clone();
                    }
                }

                PromptInfo {
                    scenario,
                    label: scenario.label().to_string(),
                    source,
                    system,
                    default_system,
                }
            })
            .collect()
    }

    /// Save a user prompt override.
    pub async fn set_override(&self, entry: PromptOverrideEntry) -> Result<(), String> {
        {
            let mut overrides = self.user_overrides.write().await;
            overrides
                .retain(|o| !(o.driver_type == entry.driver_type && o.scenario == entry.scenario));
            overrides.push(entry);
        }
        self.save().await
    }

    /// Remove a user prompt override, reverting to driver/default.
    pub async fn remove_override(
        &self,
        driver_type: &str,
        scenario: PromptScenario,
    ) -> Result<(), String> {
        {
            let mut overrides = self.user_overrides.write().await;
            overrides.retain(|o| !(o.driver_type == driver_type && o.scenario == scenario));
        }
        self.save().await
    }

    /// Get all user overrides (raw).
    pub async fn get_all_overrides(&self) -> Vec<PromptOverrideEntry> {
        self.user_overrides.read().await.clone()
    }
}

/// Convert a `PromptScenario` to its file-system key (matches `.md` / `.txt` file stems).
fn scenario_to_key(scenario: PromptScenario) -> String {
    serde_json::to_value(&scenario)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{scenario:?}").to_lowercase())
}

/// Embedded English defaults compiled into the binary as a last-resort fallback.
fn embedded_default(scenario: PromptScenario) -> &'static str {
    match scenario {
        PromptScenario::Nl2Sql => include_str!("../../resources/prompts/nl2sql.md"),
        PromptScenario::Diagnose => include_str!("../../resources/prompts/diagnose.md"),
        PromptScenario::NlFilter => include_str!("../../resources/prompts/nl_filter.md"),
        PromptScenario::SchemaDocSelectTables => {
            include_str!("../../resources/prompts/schema_doc_select_tables.md")
        }
        PromptScenario::SchemaDoc => include_str!("../../resources/prompts/schema_doc.md"),
        PromptScenario::ConnectionDiagnose => {
            include_str!("../../resources/prompts/connection_diagnose.md")
        }
        PromptScenario::QuerySummary => {
            include_str!("../../resources/prompts/query_summary.md")
        }
        PromptScenario::ExplainAnalysis => {
            include_str!("../../resources/prompts/explain_analysis.md")
        }
        PromptScenario::Chat => include_str!("../../resources/prompts/chat.md"),
        PromptScenario::WorkflowGenerate => {
            include_str!("../../resources/prompts/workflow_generate.md")
        }
    }
}

/// Replace `{{key}}` placeholders in a template.
///
/// BUG-11: After rendering, warn about and strip any residual `{{...}}` placeholders
/// so they are not sent verbatim to the LLM.
pub fn render_template(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();
    for (&key, &val) in vars {
        result = result.replace(&format!("{{{{{key}}}}}"), val);
    }
    // Check for residual placeholders and strip them
    if result.contains("{{") {
        tracing::warn!(
            residual = %result.matches("{{").count(),
            "render_template: residual {{...}} placeholders found after rendering"
        );
        // Remove all remaining {{...}} patterns
        while let Some(start) = result.find("{{") {
            if let Some(end) = result[start..].find("}}") {
                result.replace_range(start..start + end + 2, "");
            } else {
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
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
        assert!(result.contains("{{version}}"));
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
        ) -> Result<Vec<datazen_driver_api::TableInfo>, datazen_driver_api::DriverError> {
            Ok(vec![])
        }

        async fn get_table_schema(
            &self,
            _handle: &datazen_driver_api::ConnectionHandle,
            _table: &str,
        ) -> Result<datazen_driver_api::TableSchema, datazen_driver_api::DriverError> {
            Ok(datazen_driver_api::TableSchema {
                table_name: String::new(),
                columns: vec![],
                primary_keys: vec![],
                indexes: vec![],
                foreign_keys: vec![],
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
}
