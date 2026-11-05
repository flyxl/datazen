use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use crate::error::{CliError, E_USAGE};

#[derive(Debug, Parser)]
#[command(
    name = "datazen",
    version,
    about = "Read-only DataZen database access for agents and scripts"
)]
pub struct Cli {
    /// Override the DataZen app data directory (defaults to the desktop app's).
    #[arg(long, global = true, env = "DATAZEN_DATA_DIR")]
    pub data_dir: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check the app data dir, key backend, drivers, and profile catalog.
    Doctor(OutputArgs),
    /// List compiled drivers and CLI capabilities.
    Capabilities(OutputArgs),
    /// List saved connections with secrets redacted.
    Connections(OutputArgs),
    /// List tables for one connection.
    Schema(SchemaArgs),
    /// Run SQL under the CLI read-only policy.
    Query(QueryArgs),
    /// Show a connection plus its table context.
    Context(ContextArgs),
    /// Print the descriptor an external opener can use for a connection.
    Open(OpenArgs),
    /// Install the official DataZen Agent Skill.
    Agent(AgentArgs),
}

#[derive(Debug, Args, Clone, Default)]
pub struct OutputArgs {
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub csv: bool,
    #[arg(long)]
    pub file: Option<PathBuf>,
    #[arg(long, hide = true)]
    pub quiet: bool,
}

#[derive(Debug, Args)]
pub struct SchemaArgs {
    /// Connection id or name.
    pub connection: String,
    #[arg(long)]
    pub database: Option<String>,
    #[arg(long = "schema")]
    pub schema: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long, default_value = "30", value_parser = parse_timeout)]
    pub timeout: Duration,
    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Debug, Args)]
pub struct QueryArgs {
    /// Connection id or name.
    pub connection: String,
    /// SQL statement.
    pub sql: String,
    /// Permit DML writes (INSERT/UPDATE/DELETE/MERGE/REPLACE).
    #[arg(long)]
    pub allow_writes: bool,
    /// Permit dangerous statements (DDL/admin). Does not imply --allow-writes.
    #[arg(long)]
    pub allow_dangerous_sql: bool,
    /// Assert that the target is production. Writes are always refused.
    #[arg(long)]
    pub production: bool,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long, default_value = "30", value_parser = parse_timeout)]
    pub timeout: Duration,
    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Debug, Args)]
pub struct ContextArgs {
    /// Connection id or name.
    pub connection: String,
    #[arg(long)]
    pub database: Option<String>,
    #[arg(long = "schema")]
    pub schema: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long, default_value = "30", value_parser = parse_timeout)]
    pub timeout: Duration,
    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Debug, Args)]
pub struct OpenArgs {
    /// Connection id or name.
    pub connection: String,
    #[arg(long)]
    pub query: Option<String>,
    #[command(flatten)]
    pub output: OutputArgs,
}

#[derive(Debug, Args)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub action: AgentAction,
}

#[derive(Debug, Subcommand)]
pub enum AgentAction {
    /// Install the bundled DataZen skill into ~/.agents/skills/datazen.
    Setup(OutputArgs),
}

pub fn parse_timeout(value: &str) -> Result<Duration, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("timeout cannot be empty".into());
    }
    let (number, unit) = if let Some(number) = value.strip_suffix("ms") {
        (number, "ms")
    } else if let Some(number) = value.strip_suffix('s') {
        (number, "s")
    } else {
        (value, "s")
    };
    let amount: u64 = number
        .parse()
        .map_err(|_| format!("invalid timeout '{value}'"))?;
    if amount == 0 {
        return Err("timeout must be greater than zero".into());
    }
    Ok(if unit == "ms" {
        Duration::from_millis(amount)
    } else {
        Duration::from_secs(amount)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_query_flags_separately() {
        let cli = Cli::try_parse_from([
            "datazen",
            "query",
            "local",
            "update t set x = 1",
            "--allow-writes",
            "--limit",
            "10",
            "--timeout",
            "250ms",
            "--json",
        ])
        .unwrap();
        let Command::Query(args) = cli.command else {
            panic!("expected query command");
        };
        assert!(args.allow_writes);
        assert!(!args.allow_dangerous_sql);
        assert_eq!(args.limit, 10);
        assert_eq!(args.timeout, Duration::from_millis(250));
        assert!(args.output.json);
    }

    #[test]
    fn timeout_parser_accepts_seconds_and_millis() {
        assert_eq!(parse_timeout("2").unwrap(), Duration::from_secs(2));
        assert_eq!(parse_timeout("2s").unwrap(), Duration::from_secs(2));
        assert_eq!(parse_timeout("15ms").unwrap(), Duration::from_millis(15));
        assert!(parse_timeout("0").is_err());
        assert!(parse_timeout("soon").is_err());
    }
}
