use std::io::{self, Read, Write};
use std::process::ExitCode;
use datazen_sql_formatter::{format, Dialect, Indent, KeywordCase, LogicalOperatorNewline, Options};

const HELP: &str = "Usage: format-sql [--dialect sql|sqlite|postgresql|mysql|mariadb|transactsql] [--keywords preserve|upper|lower] [--indent tabs|N] [--boolean-break before|after] [--blank-lines N] < draft.sql";

fn parse(args: Vec<String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut pairs = args.chunks_exact(2);
    for pair in pairs.by_ref() {
        let value = pair[1].as_str();
        match pair[0].as_str() {
            "--dialect" => {
                options.dialect = match value {
                    "sql" => Dialect::Sql, "sqlite" => Dialect::Sqlite,
                    "postgresql" => Dialect::PostgreSql, "mysql" => Dialect::MySql,
                    "mariadb" => Dialect::MariaDb, "transactsql" => Dialect::TransactSql,
                    _ => return Err("unknown dialect".into()),
                };
            }
            "--keywords" => {
                options.keyword_case = match value {
                    "preserve" => KeywordCase::Preserve, "upper" => KeywordCase::Upper,
                    "lower" => KeywordCase::Lower,
                    _ => return Err("unknown keyword casing".into()),
                };
            }
            "--indent" => {
                options.indent = if value == "tabs" { Indent::Tabs }
                    else { Indent::Spaces(value.parse().map_err(|_| "expected an indent width")?) };
            }
            "--boolean-break" => {
                options.logical_operator_newline = match value {
                    "before" => LogicalOperatorNewline::Before, "after" => LogicalOperatorNewline::After,
                    _ => return Err("expected before or after".into()),
                };
            }
            "--blank-lines" => options.lines_between_queries =
                value.parse().map_err(|_| "expected a blank line count")?,
            _ => return Err("unknown option".into()),
        }
    }
    if !pairs.remainder().is_empty() { return Err("missing option value".into()); }
    Ok(options)
}

fn run() -> Result<(), (u8, String)> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!("{HELP}");
        return Ok(());
    }
    let options = parse(args).map_err(|err| (2, format!("{err}\n{HELP}")))?;
    // This public consumer bounds process memory without blocking the crate
    // from independently reporting its API's input limit.
    let mut input = String::new();
    io::stdin().take((datazen_sql_formatter::MAX_INPUT_BYTES + 4) as u64)
        .read_to_string(&mut input).map_err(|err| (3, format!("read SQL: {err}")))?;
    let output = format(&input, &options).map_err(|err| (1, err.to_string()))?;
    io::stdout().write_all(output.as_bytes()).map_err(|err| (3, format!("write SQL: {err}")))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => { eprintln!("{message}"); ExitCode::from(code) }
    }
}
