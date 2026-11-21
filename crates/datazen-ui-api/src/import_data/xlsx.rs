use std::collections::HashMap;

use serde_json::{Map, Value};

use super::csv::dedupe_columns;
use super::types::{
    ImportParseError, ImportParseErrorKind, ImportParseResult, ParsedData, XlsxParseOptions,
    XlsxParseResult,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct XlsxWorkbook {
    pub sheet_names: Vec<String>,
    pub sheets: HashMap<String, Vec<Map<String, Value>>>,
}

pub type XlsxBookReader = dyn Fn(&str) -> Result<XlsxWorkbook, ImportParseError>;

pub fn parse_xlsx_import(
    base64: &str,
    options: &XlsxParseOptions,
    reader: &XlsxBookReader,
) -> Result<XlsxParseResult, ImportParseError> {
    let workbook = reader(base64)?;
    if workbook.sheet_names.is_empty() {
        return Err(ImportParseError::new(
            ImportParseErrorKind::UnsupportedSheet,
            "The workbook has no worksheets",
        ));
    }
    let first = workbook.sheet_names.first().cloned().unwrap_or_default();
    let chosen = match &options.sheet_name {
        Some(name) if workbook.sheet_names.iter().any(|sheet| sheet == name) => name.clone(),
        Some(name) => {
            return Err(ImportParseError::new(
                ImportParseErrorKind::UnsupportedSheet,
                format!("No worksheet named \"{name}\""),
            ));
        }
        None => first,
    };
    let raw_rows = workbook.sheets.get(&chosen).cloned().unwrap_or_default();
    if raw_rows.is_empty() {
        return Ok(XlsxParseResult {
            sheet_names: workbook.sheet_names,
            chosen_sheet: Some(chosen),
            ..Default::default()
        });
    }

    let mut column_set = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in &raw_rows {
        for key in row.keys() {
            if seen.insert(key.clone()) {
                column_set.push(key.clone());
            }
        }
    }
    let mut diagnostics = Vec::new();
    let columns = dedupe_columns(column_set, &mut diagnostics);
    let rows: Vec<Map<String, Value>> = raw_rows
        .iter()
        .map(|item| {
            let mut record = Map::new();
            for column in &columns {
                record.insert(
                    column.clone(),
                    item.get(column).cloned().unwrap_or(Value::Null),
                );
            }
            record
        })
        .collect();
    let parsed = ImportParseResult {
        columns,
        rows,
        row_origins: vec![0; raw_rows.len()],
        diagnostics,
    };
    Ok(XlsxParseResult {
        columns: parsed.columns,
        rows: parsed.rows,
        row_origins: parsed.row_origins,
        diagnostics: parsed.diagnostics,
        sheet_names: workbook.sheet_names,
        chosen_sheet: Some(chosen),
    })
}

pub fn parse_xlsx(base64: &str, reader: &XlsxBookReader) -> Result<ParsedData, ImportParseError> {
    let result = parse_xlsx_import(base64, &XlsxParseOptions::default(), reader)?;
    Ok(ParsedData {
        columns: result.columns,
        rows: result.rows,
    })
}
