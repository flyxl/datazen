//! AI store, slice 1 of T087 (`packages/frontend/src/stores/aiStore.ts`).
//!
//! React's `useAiStore` is one zustand store. This slice is the config
//! surface only: provider config, settings profiles, the provider list,
//! remote models, validate/save/delete, and `clearError`. Chat, NL2SQL,
//! workflows and MCP are later slices of the same task and are not
//! Chat, workflows and MCP are later slices of the same task and are not
//! represented here. NL2SQL, diagnosis, EXPLAIN and the smart filter are
//! slice 2, in the same state machine.
//!
//! Rune is immediate-mode, so every `async` action is a synchronous
//! transition plus an [`AiConfigEffect`] the host runs. The host feeds
//! the IPC answer back through the matching `complete_*` / `fail_*`.
//!
//! # Host driving contract
//!
//! * `loadConfig` / `loadSettingsConfig` / `loadProviders` no-op when no
//!   backend is bound (`Skipped`). Otherwise the host performs the IPC
//!   and calls `complete_*` or `fail_*`.
//! * `saveSettingsConfig` sets `saving`, emits [`AiConfigEffect::SaveSettings`],
//!   and on success reloads via `loadConfig` **before** clearing `saving`.
//!   `ack_save_settings` starts that reload; `finish_save_settings` is the
//!   trailing `set({ saving: false })` and returns `true` even when the
//!   reload itself failed (React's `loadConfig` swallows its own error).
//! * `setActiveProfile` is the same shape with `config_loading`: the flag
//!   stays set across the reload and `finish_set_active_profile` clears it.
//! * `saveProfile` / `deleteProfile` only build the next settings object.
//!   They do not write it locally; the value lands when the follow-up
//!   `loadConfig` completes. `deleteProfile` with no settings returns
//!   [`ProfileEdit::Rejected`] and performs no IPC.
//! * `ensureAiConfigChangedListener` is [`AiConfigEffect::FetchConfig`]'s
//!   `arm_listener` bit. The host calls [`AiStore::note_listener_installed`]
//!   with `false` when the platform has no event port, so the next load
//!   tries again. `window.__datazenAiStore` is a DOM test hook and is not
//!   ported.
//!
//! # Deviations
//!
//! * `normalizeAiSettingsConfig` / `normalizeAiProviders` live in
//!   `lib/aiMemorySettings.ts` and `lib/aiProviders.ts`, not in this store.
//!   The host normalizes before `complete_load_*` / `complete_load_providers`.
//! * The synthesized settings object React builds when `settingsConfig` is
//!   null omits `skills`. This port fills `AiSkillsSettings::default()`,
//!   which is what that normalizer would write.
//! * `console.error` on a provider-list failure is dropped. The previous
//!   list is kept and `config_error` is not set, matching the catch body.

#![allow(dead_code)] // S2+ of T087 and the chat panel consume this

/// `defaultAiMemorySettings` (`types/ai.ts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiMemorySettings {
    pub enabled: bool,
    pub auto_learn: bool,
    pub cloud_policy: &'static str,
    pub privacy_acknowledged: bool,
}

impl Default for AiMemorySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_learn: false,
            cloud_policy: "normal_only",
            privacy_acknowledged: false,
        }
    }
}

/// `defaultAiSkillsSettings`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiSkillsSettings {
    pub enabled: bool,
    pub custom_dir: Option<String>,
    pub auto_use: bool,
    pub selected: Vec<String>,
}

impl Default for AiSkillsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            custom_dir: None,
            auto_use: false,
            selected: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub content: String,
    pub database_types: Vec<String>,
    pub is_default: bool,
}

/// The profile fields `saveProfile` / `deleteProfile` actually branch on,
/// plus the name the settings form round-trips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiModelProfile {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiSettingsConfig {
    pub active_profile_id: String,
    pub profiles: Vec<AiModelProfile>,
    pub global_instructions: String,
    pub prompt_templates: Vec<PromptTemplate>,
    pub memory: AiMemorySettings,
    pub skills: AiSkillsSettings,
}

/// Provider config as the store holds it. The store never opens the
/// fields; they round-trip through save / load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiProviderConfig {
    pub provider_type: String,
    pub model: String,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProviderListItem {
    pub provider_type: String,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelInfo {
    pub id: String,
    pub display_name: String,
}

/// `Nl2SqlState` (`stores/ai/types.ts`). `memory_used` is only cleared by
/// this slice; the chat slice fills the references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Nl2SqlState {
    pub input: String,
    pub streaming_sql: String,
    pub streaming_preview: String,
    pub generated_sql: String,
    pub memory_used: Vec<String>,
    pub is_generating: bool,
    pub request_id: Option<String>,
}

impl Default for Nl2SqlState {
    fn default() -> Self {
        Self {
            input: String::new(),
            streaming_sql: String::new(),
            streaming_preview: String::new(),
            generated_sql: String::new(),
            memory_used: Vec::new(),
            is_generating: false,
            request_id: None,
        }
    }
}

/// Opaque diagnosis payload. The store does not read its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosisResult {
    pub summary: String,
}

/// Opaque EXPLAIN analysis payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExplainAnalysis {
    pub summary: String,
}

/// One smart-filter condition. The store stores the list; it does not
/// interpret the fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FilterCondition {
    pub column: String,
    pub operator: String,
    pub value: String,
}

/// What the host must do next. Payloads are the request, not a promise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AiConfigEffect {
    /// `getConfig` + `getSettingsConfig`, then `complete_load_config`.
    /// `arm_listener` is the first successful chance to subscribe to
    /// `ai:config-changed`.
    FetchConfig {
        arm_listener: bool,
    },
    /// `getSettingsConfig` + `getConfig` for `loadSettingsConfig`.
    FetchSettings,
    SaveSettings(AiSettingsConfig),
    SetActiveProfile(String),
    FetchProviders,
    FetchRemoteModels {
        protocol: String,
        endpoint: String,
        api_key: String,
    },
    Validate(AiProviderConfig),
    SaveProvider(AiProviderConfig),
    DeleteProvider,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigStart {
    /// No backend bound. React returns before `set`.
    Skipped,
    Started,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProfileEdit {
    /// `deleteProfile` when `settingsConfig` is null. No IPC.
    Rejected,
    Save(AiConfigEffect),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiStore {
    pub config: Option<AiProviderConfig>,
    pub settings_config: Option<AiSettingsConfig>,
    pub is_configured: bool,
    pub providers: Vec<ProviderListItem>,
    pub config_loading: bool,
    pub config_error: Option<String>,
    pub validating: bool,
    pub saving: bool,
    pub remote_models: Vec<ModelInfo>,
    pub fetching_remote_models: bool,
    pub nl2sql: Nl2SqlState,
    pub nl2sql_error: Option<String>,
    pub diagnosis: Option<DiagnosisResult>,
    pub is_diagnosing: bool,
    pub diagnosis_error: Option<String>,
    pub explain_analysis: Option<ExplainAnalysis>,
    pub is_analyzing_explain: bool,
    pub explain_error: Option<String>,
    pub nl_filter_input: String,
    pub parsed_filters: Option<Vec<FilterCondition>>,
    pub is_parsing_filter: bool,
    pub nl_filter_error: Option<String>,
    pub chat_session: Option<crate::ai_store_chat::ChatSession>,
    pub chat_sessions: Vec<crate::ai_store_chat::ChatSessionSummary>,
    pub session_history_loading: bool,
    pub session_history_error: Option<String>,
    pub session_migration_error: Option<String>,
    pub session_migration_success: u32,
    pub memory_candidate_notice: Option<crate::ai_store_chat::MemoryCandidateNotice>,
    /// Module-level `chatInitToken`. Bumped by init, select, and new chat.
    pub chat_init_token: u64,
    pub(crate) init_message_baseline: Option<usize>,
    listener_armed: bool,
}

impl Default for AiStore {
    fn default() -> Self {
        Self {
            config: None,
            settings_config: None,
            is_configured: false,
            providers: Vec::new(),
            config_loading: false,
            config_error: None,
            validating: false,
            saving: false,
            remote_models: Vec::new(),
            fetching_remote_models: false,
            nl2sql: Nl2SqlState::default(),
            nl2sql_error: None,
            diagnosis: None,
            is_diagnosing: false,
            diagnosis_error: None,
            explain_analysis: None,
            is_analyzing_explain: false,
            explain_error: None,
            nl_filter_input: String::new(),
            parsed_filters: None,
            is_parsing_filter: false,
            nl_filter_error: None,
            chat_session: None,
            chat_sessions: Vec::new(),
            session_history_loading: false,
            session_history_error: None,
            session_migration_error: None,
            session_migration_success: 0,
            memory_candidate_notice: None,
            chat_init_token: 0,
            init_message_baseline: None,
            listener_armed: false,
        }
    }
}

impl AiStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// `loadConfig`.
    pub(crate) fn begin_load_config(
        &mut self,
        backend_bound: bool,
    ) -> (ConfigStart, Option<AiConfigEffect>) {
        match self.arm_config_fetch(backend_bound) {
            Some(effect) => (ConfigStart::Started, Some(effect)),
            None => (ConfigStart::Skipped, None),
        }
    }

    pub(crate) fn note_listener_installed(&mut self, installed: bool) {
        self.listener_armed = installed;
    }

    pub(crate) fn complete_load_config(
        &mut self,
        config: Option<AiProviderConfig>,
        settings: Option<AiSettingsConfig>,
    ) {
        self.is_configured = is_configured(config.as_ref(), settings.as_ref());
        self.config = config;
        self.settings_config = settings;
        self.config_loading = false;
    }

    pub(crate) fn fail_load_config(&mut self, message: &str) {
        self.config_loading = false;
        self.config_error = Some(message.to_owned());
    }

    /// `loadSettingsConfig`. Does not touch `config_loading`.
    pub(crate) fn begin_load_settings(&mut self, backend_bound: bool) -> ConfigStart {
        if !backend_bound {
            return ConfigStart::Skipped;
        }
        ConfigStart::Started
    }

    pub(crate) fn complete_load_settings(
        &mut self,
        config: Option<AiProviderConfig>,
        settings: AiSettingsConfig,
    ) {
        self.is_configured = is_configured(config.as_ref(), Some(&settings));
        self.config = config;
        self.settings_config = Some(settings);
    }

    pub(crate) fn fail_load_settings(&mut self, message: &str) {
        self.config_error = Some(message.to_owned());
    }

    /// `saveSettingsConfig` up to the IPC. Local settings are unchanged.
    pub(crate) fn begin_save_settings(&mut self, config: AiSettingsConfig) -> AiConfigEffect {
        self.saving = true;
        self.config_error = None;
        AiConfigEffect::SaveSettings(config)
    }

    /// IPC succeeded. Start the `loadConfig()` React awaits next, while
    /// `saving` is still true. `None` when no backend is bound (the reload
    /// no-ops).
    pub(crate) fn ack_save_settings(&mut self, backend_bound: bool) -> Option<AiConfigEffect> {
        self.arm_config_fetch(backend_bound)
    }

    /// The `set({ saving: false }); return true` after the reload settles,
    /// including a reload that failed inside `loadConfig`.
    pub(crate) fn finish_save_settings(&mut self) -> bool {
        self.saving = false;
        true
    }

    pub(crate) fn fail_save_settings(&mut self, message: &str) -> bool {
        self.saving = false;
        self.config_error = Some(message.to_owned());
        false
    }

    pub(crate) fn begin_set_active_profile(
        &mut self,
        profile_id: impl Into<String>,
    ) -> AiConfigEffect {
        self.config_loading = true;
        self.config_error = None;
        AiConfigEffect::SetActiveProfile(profile_id.into())
    }

    pub(crate) fn ack_set_active_profile(&mut self, backend_bound: bool) -> Option<AiConfigEffect> {
        self.arm_config_fetch(backend_bound)
    }

    pub(crate) fn finish_set_active_profile(&mut self) -> bool {
        self.config_loading = false;
        true
    }

    pub(crate) fn fail_set_active_profile(&mut self, message: &str) -> bool {
        self.config_loading = false;
        self.config_error = Some(message.to_owned());
        false
    }

    /// `saveProfile`: build the next settings and start `saveSettingsConfig`.
    pub(crate) fn save_profile(&mut self, profile: AiModelProfile) -> AiConfigEffect {
        let next = next_settings_for_save(self.settings_config.as_ref(), profile);
        self.begin_save_settings(next)
    }

    /// `deleteProfile`.
    pub(crate) fn delete_profile(&mut self, profile_id: &str) -> ProfileEdit {
        let Some(next) = next_settings_for_delete(self.settings_config.as_ref(), profile_id) else {
            return ProfileEdit::Rejected;
        };
        ProfileEdit::Save(self.begin_save_settings(next))
    }

    pub(crate) fn begin_load_providers(&mut self, backend_bound: bool) -> ConfigStart {
        if !backend_bound {
            ConfigStart::Skipped
        } else {
            ConfigStart::Started
        }
    }

    pub(crate) fn complete_load_providers(&mut self, providers: Vec<ProviderListItem>) {
        self.providers = providers;
    }

    pub(crate) fn fail_load_providers(&mut self) {
        // React's catch is `console.error` only: the list and `config_error`
        // are left alone. The parameter keeps the host call symmetrical.
        let _ = &self.providers;
    }

    pub(crate) fn begin_fetch_remote_models(
        &mut self,
        protocol: impl Into<String>,
        endpoint: impl Into<String>,
        api_key: impl Into<String>,
    ) -> AiConfigEffect {
        self.fetching_remote_models = true;
        self.config_error = None;
        AiConfigEffect::FetchRemoteModels {
            protocol: protocol.into(),
            endpoint: endpoint.into(),
            api_key: api_key.into(),
        }
    }

    pub(crate) fn complete_fetch_remote_models(
        &mut self,
        models: Vec<ModelInfo>,
    ) -> Vec<ModelInfo> {
        self.remote_models = models.clone();
        self.fetching_remote_models = false;
        models
    }

    pub(crate) fn fail_fetch_remote_models(&mut self, message: &str) -> Vec<ModelInfo> {
        self.fetching_remote_models = false;
        self.config_error = Some(message.to_owned());
        Vec::new()
    }

    pub(crate) fn begin_validate(&mut self, config: AiProviderConfig) -> AiConfigEffect {
        self.validating = true;
        self.config_error = None;
        AiConfigEffect::Validate(config)
    }

    pub(crate) fn complete_validate(&mut self) -> bool {
        self.validating = false;
        true
    }

    pub(crate) fn fail_validate(&mut self, message: &str) -> bool {
        self.validating = false;
        self.config_error = Some(message.to_owned());
        false
    }

    pub(crate) fn begin_save_config(&mut self, config: AiProviderConfig) -> AiConfigEffect {
        self.saving = true;
        self.config_error = None;
        AiConfigEffect::SaveProvider(config)
    }

    pub(crate) fn complete_save_config(&mut self, config: AiProviderConfig) -> bool {
        self.config = Some(config);
        self.is_configured = true;
        self.saving = false;
        true
    }

    pub(crate) fn fail_save_config(&mut self, message: &str) -> bool {
        self.saving = false;
        self.config_error = Some(message.to_owned());
        false
    }

    pub(crate) fn begin_delete_config(&mut self) -> AiConfigEffect {
        AiConfigEffect::DeleteProvider
    }

    pub(crate) fn complete_delete_config(&mut self) {
        self.config = None;
        self.is_configured = false;
    }

    pub(crate) fn fail_delete_config(&mut self, message: &str) {
        self.config_error = Some(message.to_owned());
    }

    pub(crate) fn clear_error(&mut self) {
        self.config_error = None;
    }

    /// `setNl2SqlInput`. Other NL2SQL fields stay.
    pub(crate) fn set_nl2sql_input(&mut self, input: impl Into<String>) {
        self.nl2sql.input = input.into();
    }

    /// `generateSql`. Blank or whitespace-only input returns `false` and
    /// does not touch state. On success React leaves `is_generating` set;
    /// streaming completion (not this slice) clears it. `fail_generate_sql`
    /// only clears the flag and records the error.
    ///
    /// `console.debug` lines are dropped.
    pub(crate) fn begin_generate_sql(&mut self, request_id: impl Into<String>) -> bool {
        if self.nl2sql.input.trim().is_empty() {
            return false;
        }
        self.nl2sql.streaming_sql.clear();
        self.nl2sql.streaming_preview.clear();
        self.nl2sql.generated_sql.clear();
        self.nl2sql.memory_used.clear();
        self.nl2sql.is_generating = true;
        self.nl2sql.request_id = Some(request_id.into());
        self.nl2sql_error = None;
        true
    }

    pub(crate) fn fail_generate_sql(&mut self, message: &str) {
        self.nl2sql.is_generating = false;
        self.nl2sql_error = Some(message.to_owned());
    }

    /// `clearNl2Sql`.
    pub(crate) fn clear_nl2sql(&mut self) {
        self.nl2sql = Nl2SqlState::default();
        self.nl2sql_error = None;
    }

    /// `diagnoseError`. The host passes `sql` and `error_message` already
    /// run through `redactSensitiveText` (`lib/aiQueryActions.ts`). That
    /// helper is a different file and is not part of this slice.
    pub(crate) fn begin_diagnose(&mut self) {
        self.is_diagnosing = true;
        self.diagnosis_error = None;
        self.diagnosis = None;
    }

    pub(crate) fn complete_diagnose(&mut self, result: DiagnosisResult) {
        self.diagnosis = Some(result);
        self.is_diagnosing = false;
    }

    pub(crate) fn fail_diagnose(&mut self, message: &str) {
        self.is_diagnosing = false;
        self.diagnosis_error = Some(message.to_owned());
    }

    pub(crate) fn clear_diagnosis(&mut self) {
        self.diagnosis = None;
        self.is_diagnosing = false;
        self.diagnosis_error = None;
    }

    /// `analyzeExplain`.
    pub(crate) fn begin_analyze_explain(&mut self) {
        self.is_analyzing_explain = true;
        self.explain_analysis = None;
        self.explain_error = None;
    }

    pub(crate) fn complete_analyze_explain(&mut self, result: ExplainAnalysis) {
        self.explain_analysis = Some(result);
        self.is_analyzing_explain = false;
    }

    pub(crate) fn fail_analyze_explain(&mut self, message: &str) {
        self.is_analyzing_explain = false;
        self.explain_error = Some(message.to_owned());
    }

    pub(crate) fn clear_explain_analysis(&mut self) {
        self.explain_analysis = None;
        self.is_analyzing_explain = false;
        self.explain_error = None;
    }

    pub(crate) fn set_nl_filter_input(&mut self, input: impl Into<String>) {
        self.nl_filter_input = input.into();
    }

    /// `parseFilter`. Blank input returns `None` and does not clear a
    /// previous result. Success stores and returns the filters.
    pub(crate) fn begin_parse_filter(&mut self) -> bool {
        if self.nl_filter_input.trim().is_empty() {
            return false;
        }
        self.is_parsing_filter = true;
        self.nl_filter_error = None;
        self.parsed_filters = None;
        true
    }

    pub(crate) fn complete_parse_filter(&mut self, filters: Vec<FilterCondition>) -> Vec<FilterCondition> {
        self.parsed_filters = Some(filters.clone());
        self.is_parsing_filter = false;
        filters
    }

    pub(crate) fn fail_parse_filter(&mut self, message: &str) {
        self.is_parsing_filter = false;
        self.nl_filter_error = Some(message.to_owned());
    }

    /// `clearNlFilter`.
    pub(crate) fn clear_nl_filter(&mut self) {
        self.nl_filter_input.clear();
        self.parsed_filters = None;
        self.is_parsing_filter = false;
        self.nl_filter_error = None;
    }

    fn arm_config_fetch(&mut self, backend_bound: bool) -> Option<AiConfigEffect> {
        if !backend_bound {
            return None;
        }
        self.config_loading = true;
        self.config_error = None;
        let arm_listener = !self.listener_armed;
        Some(AiConfigEffect::FetchConfig { arm_listener })
    }
}

/// `config !== null || (settings !== null && settings.profiles.length > 0)`.
fn is_configured(config: Option<&AiProviderConfig>, settings: Option<&AiSettingsConfig>) -> bool {
    config.is_some() || settings.is_some_and(|settings| !settings.profiles.is_empty())
}

fn blank_settings(active_profile_id: String) -> AiSettingsConfig {
    AiSettingsConfig {
        active_profile_id,
        profiles: Vec::new(),
        global_instructions: String::new(),
        prompt_templates: Vec::new(),
        memory: AiMemorySettings::default(),
        skills: AiSkillsSettings::default(),
    }
}

/// `saveProfile`'s local merge. Does not write the store.
pub(crate) fn next_settings_for_save(
    current: Option<&AiSettingsConfig>,
    profile: AiModelProfile,
) -> AiSettingsConfig {
    let mut next = match current {
        Some(current) => current.clone(),
        None => blank_settings(profile.id.clone()),
    };
    let exists = next.profiles.iter().any(|item| item.id == profile.id);
    if exists {
        for item in &mut next.profiles {
            if item.id == profile.id {
                *item = profile.clone();
            }
        }
    } else {
        next.profiles.push(profile.clone());
    }
    if profile.is_default {
        for item in &mut next.profiles {
            item.is_default = item.id == profile.id;
        }
    }
    if profile.is_default || next.active_profile_id.is_empty() {
        next.active_profile_id = profile.id;
    }
    next
}

/// `deleteProfile`'s local merge. `None` when there is no settings object.
pub(crate) fn next_settings_for_delete(
    current: Option<&AiSettingsConfig>,
    profile_id: &str,
) -> Option<AiSettingsConfig> {
    let current = current?;
    let mut next = current.clone();
    next.profiles.retain(|item| item.id != profile_id);
    if next.active_profile_id == profile_id {
        next.active_profile_id = next
            .profiles
            .first()
            .map(|item| item.id.clone())
            .unwrap_or_default();
    }
    Some(next)
}
