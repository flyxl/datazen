//! JSON / CSV / human output and atomic-enough file delivery.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{CliError, E_OUTPUT};

#[derive(Debug, Clone, Default)]
pub struct OutputOpts {
    pub json: bool,
    pub csv: bool,
    pub file: Option<PathBuf>,
    pub quiet: bool,
}

impl OutputOpts {
    pub fn validate(&self) -> Result<(), CliError> {
        if self.json && self.csv {
            return Err(CliError::new(
                E_OUTPUT,
                "--json and --csv are mutually exclusive",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            headers: headers.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    pub fn push(&mut self, row: impl IntoIterator<Item = impl Into<String>>) {
        self.rows.push(row.into_iter().map(Into::into).collect());
    }
}

/// Serialize one JSON value to stdout or `--file`.
pub fn emit_json<T: Serialize>(value: &T, opts: &OutputOpts) -> Result<(), CliError> {
    let body = serde_json::to_string_pretty(value)
        .map_err(|error| CliError::new(E_OUTPUT, format!("cannot encode JSON: {error}")))?;
    write_body(&format!("{body}\n"), opts)
}

/// Render a table as CSV. Quoting follows RFC 4180; row values stay strings so
/// a query result cannot smuggle a newline or comma into another column.
pub fn emit_csv(table: &Table, opts: &OutputOpts) -> Result<(), CliError> {
    let mut out = String::new();
    push_csv_row(&mut out, &table.headers);
    for row in &table.rows {
        push_csv_row(&mut out, row);
    }
    write_body(&out, opts)
}

/// Human fallback. Deliberately plain: agents should use `--json` or `--csv`.
pub fn emit_table(table: &Table, opts: &OutputOpts) -> Result<(), CliError> {
    let mut out = String::new();
    out.push_str(&table.headers.join("\t"));
    out.push('\n');
    for row in &table.rows {
        out.push_str(&row.join("\t"));
        out.push('\n');
    }
    write_body(&out, opts)
}

pub fn emit<T: Serialize>(value: &T, table: &Table, opts: &OutputOpts) -> Result<(), CliError> {
    opts.validate()?;
    if opts.json {
        emit_json(value, opts)
    } else if opts.csv {
        emit_csv(table, opts)
    } else {
        emit_table(table, opts)
    }
}

pub fn emit_error<E: Serialize + std::fmt::Display>(error: &E, json: bool) {
    if json {
        if let Ok(body) = serde_json::to_string(error) {
            eprintln!("{body}");
            return;
        }
    }
    eprintln!("{error}");
}

fn push_csv_row(out: &mut String, row: &[String]) {
    for (index, cell) in row.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let needs_quotes = cell.chars().any(|ch| matches!(ch, ',' | '"' | '\n' | '\r'));
        if needs_quotes {
            out.push('"');
            out.push_str(&cell.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(cell);
        }
    }
    out.push('\n');
}

fn write_body(body: &str, opts: &OutputOpts) -> Result<(), CliError> {
    match opts.file.as_deref() {
        Some(path) if path != Path::new("-") => fs::write(path, body)
            .map_err(|error| CliError::new(E_OUTPUT, format!("cannot write output: {error}"))),
        _ => {
            if !opts.quiet {
                print!("{body}");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_commas_quotes_and_newlines() {
        let mut table = Table::new(["name", "value"]);
        table.push(["a,b", "say \"hi\"\nnext"]);
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("out.csv");
        let opts = OutputOpts {
            csv: true,
            file: Some(file.clone()),
            ..OutputOpts::default()
        };
        emit_csv(&table, &opts).unwrap();
        let rendered = fs::read_to_string(file).unwrap();
        assert_eq!(rendered, "name,value\n\"a,b\",\"say \"\"hi\"\"\nnext\"\n");
    }

    #[test]
    fn json_and_csv_are_mutually_exclusive() {
        let opts = OutputOpts {
            json: true,
            csv: true,
            ..OutputOpts::default()
        };
        assert_eq!(opts.validate().unwrap_err().code, E_OUTPUT);
    }
}
