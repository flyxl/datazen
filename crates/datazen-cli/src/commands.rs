//! Command orchestration. Transport parsing stays in `cli`; this module owns
//! policy decisions and output shapes.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use datazen_driver_api::{create_driver, iter_driver_factories, ConnectionConfig, TableInfo};
use serde_json::json;

use crate::app_data::AppDataStore;
use crate::cli::{
    AgentAction, Cli, Command, ContextArgs, OpenArgs, OutputArgs, QueryArgs, SchemaArgs,
};
use crate::error::{CliError, E_CONFIG, E_DRIVER, E_NOT_FOUND, E_TIMEOUT, E_USAGE};
use crate::output::{emit, Table};
use crate::query::{
    ensure_supported_host_wiring, guard_sql, is_production, run_with_executor, DriverExecutor,
    Safety,
};
use crate::skill;

pub struct Deps {
    pub data_dir: PathBuf,
    pub home: PathBuf,
}

impl Deps {
    pub fn at(data_dir: PathBuf, home: PathBuf) -> Self {
        Self { data_dir, home }
    }
}

pub async fn run(cli: Cli) -> Result<(), CliError> {
    let data_dir = match cli.data_dir {
        Some(dir) => dir,
        None => crate::app_data::default_data_dir()?,
    };
    let home =
        dirs::home_dir().ok_or_else(|| CliError::new(E_CONFIG, "cannot determine home dir"))?;
    run_with(cli.command, &Deps::at(data_dir, home)).await
}

pub async fn run_with(command: Command, deps: &Deps) -> Result<(), CliError> {
    match command {
        Command::Doctor(output) => doctor(deps, output),
        Command::Capabilities(output) => capabilities(output),
        Command::Connections(output) => connections(deps, output).await,
        Command::Schema(args) => schema(deps, args).await,
        Command::Query(args) => query(deps, args).await,
        Command::Context(args) => context(deps, args).await,
        Command::Open(args) => open(deps, args).await,
        Command::Agent(args) => agent(deps, args).await,
    }
}

fn opts(output: &OutputArgs) -> crate::output::OutputOpts {
    crate::output::OutputOpts {
        json: output.json,
        csv: output.csv,
        file: output.file.clone(),
        quiet: output.quiet,
    }
}

async fn connections(deps: &Deps, output: OutputArgs) -> Result<(), CliError> {
    let store = AppDataStore::new(deps.data_dir.clone());
    let configs = store.list_redacted().await?;
    let mut table = Table::new([
        "id",
        "name",
        "databaseType",
        "host",
        "port",
        "database",
        "group",
        "readOnly",
        "production",
    ]);
    let values: Vec<serde_json::Value> = configs.iter().map(|config| summary(config)).collect();
    for config in &configs {
        table.push([
            config.id.clone(),
            config.name.clone(),
            config.database_type.clone(),
            config.host.clone().unwrap_or_default(),
            config.port.map(|port| port.to_string()).unwrap_or_default(),
            config.database.clone().unwrap_or_default(),
            config.group.clone().unwrap_or_default(),
            config.read_only.to_string(),
            is_production(config, false).to_string(),
        ]);
    }
    emit(&values, &table, &opts(&output))
}

fn summary(config: &ConnectionConfig) -> serde_json::Value {
    json!({
        "id": config.id,
        "name": config.name,
        "databaseType": config.database_type,
        "host": config.host,
        "port": config.port,
        "database": config.database,
        "schema": config.schema,
        "group": config.group,
        "readOnly": config.read_only,
        "production": is_production(config, false),
        "lastConnectedAt": config.last_connected_at,
    })
}

fn doctor(deps: &Deps, output: OutputArgs) -> Result<(), CliError> {
    let store = AppDataStore::new(deps.data_dir.clone());
    let drivers: Vec<serde_json::Value> = iter_driver_factories()
        .into_iter()
        .map(|factory| {
            json!({
                "id": factory.driver_id(),
                "protocolVersion": factory.protocol_version(),
            })
        })
        .collect();
    let mut issues = Vec::new();
    let mut connection_count = 0_usize;
    match std::fs::metadata(store.connections_path()) {
        Ok(metadata) if metadata.is_file() => match serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(store.connections_path()).unwrap_or_default(),
        ) {
            Ok(serde_json::Value::Array(records)) => connection_count = records.len(),
            Ok(_) => issues.push("connections.json is not an array".to_string()),
            Err(error) => issues.push(format!("connections.json is not valid JSON: {error}")),
        },
        Ok(_) => issues.push("connections.json is not a file".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            connection_count = 0;
        }
        Err(error) => issues.push(format!("cannot inspect connections.json: {error}")),
    }
    if drivers.is_empty() {
        issues.push("no database drivers are linked into this build".into());
    }
    let report = json!({
        "ok": issues.is_empty(),
        "version": env!("CARGO_PKG_VERSION"),
        "dataDir": deps.data_dir,
        "connectionsFile": store.connections_path(),
        "connectionCount": connection_count,
        "drivers": drivers,
        "keyBackend": store.key_backend(),
        "issues": issues,
    });
    let mut table = Table::new(["check", "value"]);
    for key in [
        "ok",
        "version",
        "dataDir",
        "connectionCount",
        "keyBackend",
        "issues",
    ] {
        table.push([key.to_string(), report[key].to_string()]);
    }
    emit(&report, &table, &opts(&output))
}

fn capabilities(output: OutputArgs) -> Result<(), CliError> {
    let drivers: Vec<serde_json::Value> = iter_driver_factories()
        .into_iter()
        .map(|factory| {
            json!({
                "id": factory.driver_id(),
                "protocolVersion": factory.protocol_version(),
                "cancellable": factory.supports_cancel_query(),
                "explain": factory.supports_explain(),
            })
        })
        .collect();
    let report = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "commands": ["doctor", "capabilities", "connections", "schema", "query", "context", "open", "agent setup"],
        "outputs": ["json", "csv"],
        "drivers": drivers,
        "safety": {
            "defaultReadOnly": true,
            "allowWritesFlag": "--allow-writes",
            "allowDangerousSqlFlag": "--allow-dangerous-sql",
            "productionWrites": "always refused",
        },
    });
    let mut table = Table::new(["driver", "protocolVersion", "cancellable", "explain"]);
    for driver in &report["drivers"].as_array().cloned().unwrap_or_default() {
        table.push([
            driver["id"].to_string(),
            driver["protocolVersion"].to_string(),
            driver["cancellable"].to_string(),
            driver["explain"].to_string(),
        ]);
    }
    emit(&report, &table, &opts(&output))
}

async fn resolve_connection(
    deps: &Deps,
    selector: &str,
) -> Result<(AppDataStore, ConnectionConfig), CliError> {
    let store = AppDataStore::new(deps.data_dir.clone());
    match store.get_plain(selector).await {
        Ok(config) => return Ok((store, config)),
        Err(error) if error.code == crate::error::E_NOT_FOUND => {}
        Err(error) => return Err(error),
    }
    let all = store.list_redacted().await?;
    let matches: Vec<_> = all
        .into_iter()
        .filter(|config| config.name == selector)
        .collect();
    match matches.len() {
        0 => Err(CliError::new(
            E_NOT_FOUND,
            format!("connection '{selector}' not found by id or name"),
        )),
        1 => {
            let id = matches[0].id.clone();
            let config = store.get_plain(&id).await?;
            Ok((store, config))
        }
        _ => Err(CliError::new(
            E_USAGE,
            format!("connection name '{selector}' is ambiguous; use an id"),
        )),
    }
}

async fn schema(deps: &Deps, args: SchemaArgs) -> Result<(), CliError> {
    let (_store, config) = resolve_connection(deps, &args.connection).await?;
    let database = database_of(&config, args.database.as_deref())?;
    let schema = args.schema.clone().or_else(|| config.schema.clone());
    let tables = fetch_tables(
        &config,
        &database,
        schema.as_deref(),
        args.timeout,
        args.limit,
    )
    .await?;
    let value = json!({
        "connection": summary(&config),
        "database": database,
        "schema": schema,
        "tables": tables.iter().map(table_json).collect::<Vec<_>>(),
    });
    emit(&value, &tables_table(&tables), &opts(&args.output))
}

async fn context(deps: &Deps, args: ContextArgs) -> Result<(), CliError> {
    let (_store, config) = resolve_connection(deps, &args.connection).await?;
    let database = database_of(&config, args.database.as_deref())?;
    let schema = args.schema.clone().or_else(|| config.schema.clone());
    let tables = fetch_tables(
        &config,
        &database,
        schema.as_deref(),
        args.timeout,
        args.limit,
    )
    .await?;
    let value = json!({
        "connection": summary(&config),
        "database": database,
        "schema": schema,
        "tables": tables.iter().map(table_json).collect::<Vec<_>>(),
    });
    emit(&value, &tables_table(&tables), &opts(&args.output))
}

async fn query(deps: &Deps, args: QueryArgs) -> Result<(), CliError> {
    let (_store, config) = resolve_connection(deps, &args.connection).await?;
    let safety = Safety {
        allow_writes: args.allow_writes,
        allow_dangerous_sql: args.allow_dangerous_sql,
        production: args.production,
    };
    // Guard before building an executor: an unsafe statement must not even
    // attempt to open a connection.
    guard_sql(&args.sql, &config, safety)?;
    let executor = Arc::new(DriverExecutor {
        config: config.clone(),
    });
    let output = run_with_executor(
        executor,
        crate::query::QueryRequest {
            connection_id: config.id.clone(),
            sql: args.sql,
            limit: args.limit,
        },
        safety,
        &config,
        args.timeout,
    )
    .await?;
    emit(&output, &output.table(), &opts(&args.output))
}

async fn open(deps: &Deps, args: OpenArgs) -> Result<(), CliError> {
    let (_store, config) = resolve_connection(deps, &args.connection).await?;
    let data_dir = std::path::absolute(&deps.data_dir)
        .map_err(|error| CliError::new(E_CONFIG, format!("cannot resolve data dir: {error}")))?;
    crate::desktop_launch::launch(&config.id, args.query.as_deref(), Some(&data_dir))?;
    let report = json!({
        "connection": summary(&config),
        "launched": true,
        "queryProvided": args.query.is_some(),
    });
    let mut table = Table::new(["connection", "launched", "query"]);
    table.push([
        config.name.clone(),
        "true".to_string(),
        if args.query.is_some() { "yes" } else { "no" }.to_string(),
    ]);
    emit(&report, &table, &opts(&args.output))
}

async fn agent(deps: &Deps, args: crate::cli::AgentArgs) -> Result<(), CliError> {
    match args.action {
        AgentAction::Setup(output) => {
            let outcome = skill::install(&deps.home)?;
            let mut table = Table::new(["status", "path"]);
            table.push([outcome.status.to_string(), outcome.path.clone()]);
            emit(&outcome, &table, &opts(&output))
        }
    }
}

fn database_of(config: &ConnectionConfig, requested: Option<&str>) -> Result<String, CliError> {
    requested
        .map(str::to_string)
        .or_else(|| config.database.clone())
        .ok_or_else(|| {
            CliError::new(
                E_USAGE,
                "this connection has no default database; pass --database",
            )
        })
}

async fn fetch_tables(
    config: &ConnectionConfig,
    database: &str,
    schema: Option<&str>,
    timeout: Duration,
    limit: usize,
) -> Result<Vec<TableInfo>, CliError> {
    ensure_supported_host_wiring(config)?;
    let driver = create_driver(&config.database_type).ok_or_else(|| {
        CliError::new(
            E_DRIVER,
            format!(
                "driver '{}' is not compiled into this CLI build",
                config.database_type
            ),
        )
    })?;
    let run = async {
        let handle = driver
            .connect(config)
            .await
            .map_err(|error| CliError::new(E_DRIVER, format!("connection failed: {error}")))?;
        let tables = driver
            .get_tables(&handle, database, schema)
            .await
            .map_err(|error| CliError::new(E_DRIVER, format!("schema failed: {error}")));
        let _ = driver.disconnect(handle).await;
        tables
    };
    let mut tables = tokio::time::timeout(timeout, run)
        .await
        .map_err(|_| CliError::new(E_TIMEOUT, "schema request timed out"))??;
    tables.truncate(limit);
    Ok(tables)
}

fn table_json(table: &TableInfo) -> serde_json::Value {
    json!({
        "name": table.name,
        "schema": table.schema,
        "tableType": format!("{:?}", table.table_type),
        "rowCount": table.row_count,
    })
}

fn tables_table(tables: &[TableInfo]) -> Table {
    let mut out = Table::new(["name", "schema", "tableType", "rowCount"]);
    for table in tables {
        out.push([
            table.name.clone(),
            table.schema.clone().unwrap_or_default(),
            format!("{:?}", table.table_type),
            table
                .row_count
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ConnectionConfig;

    fn config(name: &str) -> ConnectionConfig {
        serde_json::from_value(json!({
            "id": "c1",
            "name": name,
            "databaseType": "sqlite",
            "host": null,
            "port": null,
            "database": "file:test",
            "schema": null,
            "username": null,
            "password": null,
            "sshTunnel": null,
            "httpProxyTunnel": null,
            "websocketTunnel": null
        }))
        .unwrap()
    }

    #[test]
    fn connection_summary_has_no_secret_fields() {
        let mut config = config("local");
        config.password = Some("super-secret".into());
        let rendered = serde_json::to_string(&summary(&config)).unwrap();
        assert!(!rendered.contains("super-secret"));
        assert!(!rendered.contains("password"));
    }

    #[test]
    fn database_defaults_to_connection_and_requires_one() {
        let mut config = config("local");
        assert_eq!(database_of(&config, None).unwrap(), "file:test");
        config.database = None;
        assert_eq!(database_of(&config, None).unwrap_err().code, E_USAGE);
    }
}
