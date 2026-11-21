use crate::{build_compare_report_text, CompareReportRow, CompareSummaryStats};

pub const EXPLAIN_DIFF_NO_AI_KEY: &str = "sync.explainDiffNoAi";
pub const EXPLAIN_DIFF_PROMPT_KEY: &str = "sync.explainDiffPrompt";
pub const EXPLAIN_DIFF_FAILED_KEY: &str = "sync.explainDiffFailed";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplainInput {
    pub is_ai_configured: bool,
    pub source_db_session_id: Option<String>,
    pub target_db_session_id: Option<String>,
    pub source_database: String,
    pub target_database: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplainRequest {
    pub db_session_id: Option<String>,
    pub database: Option<String>,
    pub prompt: String,
    pub role: &'static str,
    pub include_schema: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplainEffects {
    pub explain_open: bool,
    pub explain_loading: bool,
    pub explain_text: String,
    pub error_message: Option<String>,
    pub error_open: bool,
}

pub fn explain_gate(input: &ExplainInput) -> Option<&'static str> {
    if input.is_ai_configured {
        None
    } else {
        Some(EXPLAIN_DIFF_NO_AI_KEY)
    }
}

pub fn build_explain_prompt<F>(report: &str, translate: F) -> String
where
    F: Fn(&str, &[(&str, String)]) -> String,
{
    translate(EXPLAIN_DIFF_PROMPT_KEY, &[("report", report.to_owned())])
}

pub fn explain_request(input: &ExplainInput, prompt: &str) -> ExplainRequest {
    let database = if !input.source_database.is_empty() {
        Some(input.source_database.clone())
    } else if !input.target_database.is_empty() {
        Some(input.target_database.clone())
    } else {
        None
    };
    ExplainRequest {
        db_session_id: input
            .source_db_session_id
            .clone()
            .or_else(|| input.target_db_session_id.clone()),
        database,
        prompt: prompt.to_owned(),
        role: "user",
        include_schema: false,
    }
}

pub fn begin_explain() -> ExplainEffects {
    ExplainEffects {
        explain_open: true,
        explain_loading: true,
        explain_text: String::new(),
        error_message: None,
        error_open: false,
    }
}

pub fn success_explain(text: String) -> ExplainEffects {
    ExplainEffects {
        explain_open: true,
        explain_loading: false,
        explain_text: text,
        error_message: None,
        error_open: false,
    }
}

pub fn failure_explain(detail: &str) -> ExplainEffects {
    ExplainEffects {
        explain_open: false,
        explain_loading: false,
        explain_text: String::new(),
        error_message: Some(format!("{EXPLAIN_DIFF_FAILED_KEY}: {detail}")),
        error_open: true,
    }
}

pub fn build_explain_report(rows: &[CompareReportRow], stats: &CompareSummaryStats) -> String {
    build_compare_report_text(rows, Some(stats))
}

pub const DATA_SYNC_EXPLAIN_SURFACE: [&str; 8] = [
    "useDataSyncExplain",
    "handleExplainDiff",
    "explainGate",
    "buildExplainPrompt",
    "explainRequest",
    "beginExplain",
    "successExplain",
    "DATA_SYNC_EXPLAIN_GAP",
];

pub const DATA_SYNC_EXPLAIN_GAP: [&str; 6] = [
    "React hook state and useCallback wiring",
    "aiCommands.chat IPC and the configured AI provider",
    "crypto.randomUUID request ids",
    "i18n t() for prompt and error messages",
    "the dialog that renders explainText",
    "feedback setErrorMsg / setErrorOpen host callbacks",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ExplainInput {
        ExplainInput {
            is_ai_configured: true,
            source_db_session_id: Some("s1".to_owned()),
            target_db_session_id: Some("t1".to_owned()),
            source_database: String::new(),
            target_database: "shop".to_owned(),
        }
    }

    #[test]
    fn unconfigured_ai_blocks_with_the_exact_error_key() {
        let mut item = input();
        item.is_ai_configured = false;
        assert_eq!(explain_gate(&item), Some("sync.explainDiffNoAi"));
        assert_eq!(explain_gate(&input()), None);
    }

    #[test]
    fn prompt_template_receives_the_report_as_a_named_param() {
        let prompt = build_explain_prompt("REPORT", |key, params| {
            assert_eq!(key, "sync.explainDiffPrompt");
            assert_eq!(params[0].0, "report");
            format!("{}:{}", key, params[0].1)
        });
        assert_eq!(prompt, "sync.explainDiffPrompt:REPORT");
    }

    #[test]
    fn request_prefers_the_source_session_and_database() {
        let request = explain_request(&input(), "prompt");
        assert_eq!(request.db_session_id.as_deref(), Some("s1"));
        assert_eq!(request.database.as_deref(), Some("shop"));
        assert_eq!(request.role, "user");
        assert!(!request.include_schema);

        let mut fallback = input();
        fallback.source_db_session_id = None;
        fallback.source_database = "src".to_owned();
        let request = explain_request(&fallback, "prompt");
        assert_eq!(request.db_session_id.as_deref(), Some("t1"));
        assert_eq!(request.database.as_deref(), Some("src"));

        let mut empty = input();
        empty.source_database.clear();
        empty.target_database.clear();
        assert_eq!(explain_request(&empty, "p").database, None);
    }

    #[test]
    fn state_transitions_pin_open_loading_and_error_behavior() {
        let begin = begin_explain();
        assert!(begin.explain_open);
        assert!(begin.explain_loading);
        assert_eq!(begin.explain_text, "");

        let success = success_explain("answer".to_owned());
        assert!(success.explain_open);
        assert!(!success.explain_loading);
        assert_eq!(success.explain_text, "answer");
        assert_eq!(success.error_message, None);

        let failure = failure_explain("boom");
        assert!(!failure.explain_open);
        assert!(!failure.explain_loading);
        assert_eq!(failure.explain_text, "");
        assert_eq!(
            failure.error_message.as_deref(),
            Some("sync.explainDiffFailed: boom")
        );
        assert!(failure.error_open);
    }

    #[test]
    fn report_delegates_to_the_shared_compare_report() {
        let rows: Vec<CompareReportRow> = Vec::new();
        let stats = CompareSummaryStats::default();
        assert_eq!(
            build_explain_report(&rows, &stats),
            build_compare_report_text(&rows, Some(&stats))
        );
        assert_eq!(DATA_SYNC_EXPLAIN_SURFACE.len(), 8);
        assert_eq!(DATA_SYNC_EXPLAIN_GAP.len(), 6);
        assert_eq!(crate::DATA_SYNC_EXPLAIN_GAP.len(), 6);
    }
}
