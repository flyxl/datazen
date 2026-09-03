//! MCP prompt handlers.

use super::server::DataZenMcpServer;
use super::types::*;
use crate::ai::budget;
use crate::ai::prompt_resolver;
use datazen_driver_api::PromptScenario;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{prompt, prompt_router, ErrorData as McpError};
use std::collections::HashMap;

#[prompt_router(vis = "pub(crate)")]
impl DataZenMcpServer {
    #[prompt(
        name = "nl2sql",
        description = "Convert natural language to SQL based on the database schema"
    )]
    pub(crate) async fn nl2sql_prompt(
        &self,
        Parameters(args): Parameters<Nl2SqlArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let (conn_id, driver, _handle) = self.resolve_connection(&args.connection_id).await?;
        let lang = self.app_state.store.get_settings().await.language;
        let db_type = driver.driver_type();
        let db = args.database.as_deref().unwrap_or("");

        let context = self
            .app_state
            .schema_context_builder
            .build_sql_context(&conn_id, db, None, &[], budget::FALLBACK_DDL)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut vars = HashMap::new();
        vars.insert("db_type", db_type.as_str());
        vars.insert("version", "");
        vars.insert("schema", context.schema_ddl.as_str());
        vars.insert("recent", "");
        let system = self
            .app_state
            .prompt_resolver
            .resolve(PromptScenario::Nl2Sql, Some(driver.as_ref()), &lang)
            .await;
        let system_content = prompt_resolver::render_template(&system, &vars);

        Ok(GetPromptResult::new(vec![
            PromptMessage::new_text(Role::User, system_content),
            PromptMessage::new_text(Role::User, args.question.clone()),
        ])
        .with_description("Natural language to SQL conversion with schema context"))
    }

    #[prompt(
        name = "diagnose_error",
        description = "Diagnose a SQL error and suggest fixes"
    )]
    pub(crate) async fn diagnose_error_prompt(
        &self,
        Parameters(args): Parameters<DiagnoseErrorArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let (_conn_id, driver, _handle) = self.resolve_connection(&args.connection_id).await?;
        let lang = self.app_state.store.get_settings().await.language;
        let db_type = driver.driver_type();

        let mut vars = HashMap::new();
        vars.insert("db_type", db_type.as_str());
        vars.insert("version", "");
        vars.insert("schema", "");
        vars.insert("recent", "");
        let system = self
            .app_state
            .prompt_resolver
            .resolve(PromptScenario::Diagnose, Some(driver.as_ref()), &lang)
            .await;
        let system_content = prompt_resolver::render_template(&system, &vars);

        Ok(GetPromptResult::new(vec![
            PromptMessage::new_text(Role::User, system_content),
            PromptMessage::new_text(
                Role::User,
                format!("SQL:\n```\n{}\n```\n\nError:\n{}", args.sql, args.error),
            ),
        ])
        .with_description("SQL error diagnosis with fix suggestions"))
    }

    #[prompt(
        name = "explain_plan",
        description = "Analyze a query execution plan and suggest optimizations"
    )]
    pub(crate) async fn explain_plan_prompt(
        &self,
        Parameters(args): Parameters<ExplainPlanArgs>,
    ) -> Result<GetPromptResult, McpError> {
        let (_conn_id, driver, handle) = self.resolve_connection(&args.connection_id).await?;
        let lang = self.app_state.store.get_settings().await.language;
        let db_type = driver.driver_type();

        let explain_result = driver
            .explain(&handle, &args.sql)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let explain_text = serde_json::to_string_pretty(&explain_result)
            .unwrap_or_else(|_| "Failed to serialize EXPLAIN output".to_string());

        let mut vars = HashMap::new();
        vars.insert("db_type", db_type.as_str());
        vars.insert("version", "");
        vars.insert("schema", "");
        vars.insert("recent", "");
        let system = self
            .app_state
            .prompt_resolver
            .resolve(
                PromptScenario::ExplainAnalysis,
                Some(driver.as_ref()),
                &lang,
            )
            .await;
        let system_content = prompt_resolver::render_template(&system, &vars);

        Ok(GetPromptResult::new(vec![
            PromptMessage::new_text(Role::User, system_content),
            PromptMessage::new_text(
                Role::User,
                format!(
                    "SQL:\n```\n{}\n```\n\nEXPLAIN output:\n```\n{}\n```",
                    args.sql, explain_text
                ),
            ),
        ])
        .with_description("Query execution plan analysis"))
    }
}
