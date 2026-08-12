//! Prompt resolution with driver + user override support.
//!
//! Resolution order (highest priority first):
//! 1. User override for (driver_type, scenario) — exact match
//! 2. User override for (*, scenario) — global override
//! 3. Driver-specific prompt from `DatabaseDriver::prompt_overrides()`
//! 4. Built-in default from resource files (`resources/prompts/*.txt`)

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
    pub system_zh: String,
    pub system_en: String,
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
    pub system_zh: String,
    pub system_en: String,
    pub default_zh: String,
    pub default_en: String,
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
    /// lang -> (scenario_key -> template_content)
    template_cache: RwLock<HashMap<String, HashMap<String, String>>>,
    overrides_loaded: AtomicBool,
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
            load_lock: Mutex::new(()),
        }
    }

    /// Load overrides + language templates on first AI/prompt use.
    pub async fn ensure_ready(&self, lang: &str) {
        self.ensure_overrides_loaded().await;
        // load_language is cheap when already cached
        if !self.template_cache.read().await.contains_key("en") {
            self.load_language(lang).await;
        } else if lang != "en" && !self.template_cache.read().await.contains_key(lang) {
            self.load_language(lang).await;
        }
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

    /// Load prompt templates for a language from the prompts directory.
    /// Always loads "en" first as fallback.
    /// For `zh-*` languages, tries exact match (e.g. `zh-TW`) first, then `zh-CN`.
    pub async fn load_language(&self, lang: &str) {
        let Some(dir) = &self.prompts_dir else { return };

        self.scan_and_cache_lang(dir, "en").await;

        if lang != "en" {
            self.scan_and_cache_lang(dir, lang).await;

            if lang.starts_with("zh") && lang != "zh-CN" {
                self.scan_and_cache_lang(dir, "zh-CN").await;
            }
        }
    }

    async fn scan_and_cache_lang(&self, base_dir: &Path, lang: &str) {
        let lang_dir = base_dir.join(lang);
        if !lang_dir.is_dir() {
            return;
        }

        let mut templates = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&lang_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "txt").unwrap_or(false) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            templates.insert(stem.to_string(), content);
                        }
                    }
                }
            }
        }

        if !templates.is_empty() {
            tracing::info!(
                "[prompts] loaded {} templates for lang={lang}",
                templates.len()
            );
            self.template_cache
                .write()
                .await
                .insert(lang.to_string(), templates);
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
    pub async fn resolve(
        &self,
        scenario: PromptScenario,
        driver: Option<&dyn DatabaseDriver>,
        lang: &str,
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
                return Self::select_lang(&entry.system_zh, &entry.system_en, lang);
            }
        }

        // 2. User override for (*, scenario)
        if let Some(entry) = overrides
            .iter()
            .find(|o| o.driver_type == "*" && o.scenario == scenario)
        {
            return Self::select_lang(&entry.system_zh, &entry.system_en, lang);
        }

        // 3. Driver-specific prompt
        if let Some(d) = driver {
            let driver_prompts = d.prompt_overrides();
            if let Some(tpl) = driver_prompts.get(&scenario) {
                return Self::select_lang(&tpl.system_zh, &tpl.system_en, lang);
            }
        }

        // 4. Template from files (cached) → zh-CN fallback → en fallback → embedded
        let key = scenario_to_key(scenario);

        let cache = self.template_cache.read().await;
        // Try exact lang
        if let Some(tpl) = cache.get(lang).and_then(|m| m.get(&key)) {
            return tpl.clone();
        }
        // For zh-* variants, fall back to zh-CN
        if lang.starts_with("zh") && lang != "zh-CN" {
            if let Some(tpl) = cache.get("zh-CN").and_then(|m| m.get(&key)) {
                return tpl.clone();
            }
        }
        // Fall back to en
        if !lang.starts_with("en") {
            if let Some(tpl) = cache.get("en").and_then(|m| m.get(&key)) {
                return tpl.clone();
            }
        }
        drop(cache);

        embedded_default(scenario).to_string()
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
                let en_fallback = embedded_default(scenario).to_string();
                let default_zh = cache
                    .get("zh-CN")
                    .and_then(|m| m.get(&key))
                    .cloned()
                    .unwrap_or_else(|| en_fallback.clone());
                let default_en = cache
                    .get("en")
                    .and_then(|m| m.get(&key))
                    .cloned()
                    .unwrap_or(en_fallback);
                let mut source = PromptSource::Default;
                let mut system_zh = default_zh.clone();
                let mut system_en = default_en.clone();

                // Check driver override
                if let Some(tpl) = driver_prompts.get(&scenario) {
                    source = PromptSource::Driver;
                    system_zh = tpl.system_zh.clone();
                    system_en = tpl.system_en.clone();
                }

                // Check user override (exact driver match)
                if let Some(ref dt) = driver_type_name {
                    if let Some(entry) = overrides
                        .iter()
                        .find(|o| o.driver_type == *dt && o.scenario == scenario)
                    {
                        source = PromptSource::User;
                        system_zh = entry.system_zh.clone();
                        system_en = entry.system_en.clone();
                    }
                }

                // Check user override (global)
                if source != PromptSource::User {
                    if let Some(entry) = overrides
                        .iter()
                        .find(|o| o.driver_type == "*" && o.scenario == scenario)
                    {
                        source = PromptSource::User;
                        system_zh = entry.system_zh.clone();
                        system_en = entry.system_en.clone();
                    }
                }

                PromptInfo {
                    scenario,
                    label: scenario.label().to_string(),
                    source,
                    system_zh,
                    system_en,
                    default_zh,
                    default_en,
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

    fn select_lang(zh: &str, en: &str, lang: &str) -> String {
        if lang.starts_with("zh") {
            zh.to_string()
        } else {
            en.to_string()
        }
    }
}

/// Convert a `PromptScenario` to its file-system key (matches `.txt` file stems).
fn scenario_to_key(scenario: PromptScenario) -> String {
    serde_json::to_value(&scenario)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{scenario:?}").to_lowercase())
}

/// Embedded English defaults compiled into the binary as a last-resort fallback.
fn embedded_default(scenario: PromptScenario) -> &'static str {
    match scenario {
        PromptScenario::Nl2Sql => include_str!("../../resources/prompts/en/nl2sql.txt"),
        PromptScenario::Diagnose => include_str!("../../resources/prompts/en/diagnose.txt"),
        PromptScenario::NlFilter => include_str!("../../resources/prompts/en/nl_filter.txt"),
        PromptScenario::SchemaDocSelectTables => {
            include_str!("../../resources/prompts/en/schema_doc_select_tables.txt")
        }
        PromptScenario::SchemaDoc => include_str!("../../resources/prompts/en/schema_doc.txt"),
        PromptScenario::ConnectionDiagnose => {
            include_str!("../../resources/prompts/en/connection_diagnose.txt")
        }
        PromptScenario::QuerySummary => {
            include_str!("../../resources/prompts/en/query_summary.txt")
        }
        PromptScenario::ExplainAnalysis => {
            include_str!("../../resources/prompts/en/explain_analysis.txt")
        }
        PromptScenario::Chat => include_str!("../../resources/prompts/en/chat.txt"),
        PromptScenario::WorkflowGenerate => {
            include_str!("../../resources/prompts/en/workflow_generate.txt")
        }
    }
}

/// Replace `{{key}}` placeholders in a template.
pub fn render_template(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();
    for (&key, &val) in vars {
        result = result.replace(&format!("{{{{{key}}}}}"), val);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

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
                system_zh: "自定义中文".into(),
                system_en: "Custom English".into(),
            })
            .await
            .unwrap();

        let result = resolver.resolve(PromptScenario::Chat, None, "zh-CN").await;
        assert_eq!(result, "自定义中文");

        let result = resolver.resolve(PromptScenario::Chat, None, "en").await;
        assert_eq!(result, "Custom English");
    }

    #[tokio::test]
    async fn test_resolver_remove_override() {
        let tmp = tempfile::tempdir().unwrap();
        let resolver = PromptResolver::new(tmp.path(), None);

        resolver
            .set_override(PromptOverrideEntry {
                driver_type: "*".into(),
                scenario: PromptScenario::Chat,
                system_zh: "自定义".into(),
                system_en: "Custom".into(),
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
                    system_zh: "PG专属".into(),
                    system_en: "PG specific".into(),
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
            assert!(!p.default_zh.is_empty());
            assert!(!p.default_en.is_empty());
        }
    }

    #[tokio::test]
    async fn test_load_language_from_prompts_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let prompts_dir = tmp.path().join("prompts");
        let en_dir = prompts_dir.join("en");
        std::fs::create_dir_all(&en_dir).unwrap();
        std::fs::write(en_dir.join("chat.txt"), "Custom chat prompt from file").unwrap();

        let data_dir = tmp.path().join("data");
        let resolver = PromptResolver::new(&data_dir, Some(prompts_dir));
        resolver.load_language("en").await;
        let result = resolver.resolve(PromptScenario::Chat, None, "en").await;
        assert_eq!(result, "Custom chat prompt from file");
    }

    #[tokio::test]
    async fn test_ensure_ready_loads_templates() {
        let tmp = tempfile::tempdir().unwrap();
        let prompts_dir = tmp.path().join("prompts");
        let en_dir = prompts_dir.join("en");
        std::fs::create_dir_all(&en_dir).unwrap();
        std::fs::write(en_dir.join("nl2sql.txt"), "File-backed NL2SQL").unwrap();

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
                system_zh: "PG中文".into(),
                system_en: "PG English".into(),
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
                system_zh: "全局诊断".into(),
                system_en: "Global diagnose".into(),
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
}
