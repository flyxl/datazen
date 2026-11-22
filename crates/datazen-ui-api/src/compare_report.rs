#[cfg(test)]
mod tests;

use serde::Serialize;

use crate::{
    display_table_name, row_diff_counts, table_has_row_diffs, TableListRow, TableSyncStatus,
};

#[derive(Clone, Debug, PartialEq)]
pub struct CompareReportRow {
    pub table: TableListRow,
    pub incompatible_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CompareSummaryStats {
    pub inserts: usize,
    pub updates: usize,
    pub deletes: usize,
    #[serde(rename = "unchangedTables")]
    pub unchanged_tables: usize,
    pub incompatible: usize,
}

pub fn summarize_compare(rows: &[CompareReportRow]) -> CompareSummaryStats {
    let mut stats = CompareSummaryStats::default();
    for row in rows {
        if row.table.status == TableSyncStatus::Incompatible {
            stats.incompatible += 1;
            continue;
        }
        if row.table.status != TableSyncStatus::Matched {
            continue;
        }
        let counts = row_diff_counts(&row.table);
        stats.inserts += counts.inserts;
        stats.updates += counts.updates;
        stats.deletes += counts.deletes;
        if !table_has_row_diffs(&row.table) {
            stats.unchanged_tables += 1;
        }
    }
    stats
}

pub fn build_compare_report_text(
    rows: &[CompareReportRow],
    stats: Option<&CompareSummaryStats>,
) -> String {
    let summary = stats.cloned().unwrap_or_else(|| summarize_compare(rows));
    let mut lines = vec![
        "Data Sync Compare Report".to_owned(),
        format!("Inserts: {}", summary.inserts),
        format!("Updates: {}", summary.updates),
        format!("Deletes: {}", summary.deletes),
        format!("Unchanged tables: {}", summary.unchanged_tables),
        format!("Incompatible tables: {}", summary.incompatible),
        String::new(),
        "Tables:".to_owned(),
    ];
    for row in rows {
        let name = display_table_name(&row.table);
        if row.table.status == TableSyncStatus::Incompatible {
            let reason = row
                .incompatible_reason
                .as_deref()
                .filter(|reason| !reason.is_empty())
                .map(|reason| format!(" — {reason}"))
                .unwrap_or_default();
            lines.push(format!("- {name}: INCOMPATIBLE{reason}"));
            continue;
        }
        if row.table.status != TableSyncStatus::Matched {
            lines.push(format!("- {name}: {}", row.table.status.as_str()));
            continue;
        }
        let counts = row_diff_counts(&row.table);
        lines.push(format!(
            "- {name}: +{} / ~{} / −{} ({} unchanged rows)",
            counts.inserts, counts.updates, counts.deletes, counts.unchanged
        ));
    }
    lines.join("\n")
}

#[derive(Serialize)]
#[serde(untagged)]
enum ReportTable {
    Incompatible {
        name: String,
        status: &'static str,
        #[serde(rename = "incompatibleReason")]
        incompatible_reason: Option<String>,
    },
    Plain {
        name: String,
        status: &'static str,
    },
    Matched {
        name: String,
        status: &'static str,
        inserts: usize,
        updates: usize,
        deletes: usize,
        #[serde(rename = "unchangedRows")]
        unchanged_rows: usize,
    },
}

#[derive(Serialize)]
struct ReportPayload<'a> {
    summary: &'a CompareSummaryStats,
    tables: Vec<ReportTable>,
}

pub fn build_compare_report_json(
    rows: &[CompareReportRow],
    stats: Option<&CompareSummaryStats>,
) -> String {
    let default_stats = summarize_compare(rows);
    let summary = stats.unwrap_or(&default_stats);
    let tables = rows
        .iter()
        .map(|row| {
            let name = display_table_name(&row.table);
            let status = row.table.status.as_str();
            if row.table.status == TableSyncStatus::Incompatible {
                ReportTable::Incompatible {
                    name,
                    status,
                    incompatible_reason: row.incompatible_reason.clone(),
                }
            } else if row.table.status != TableSyncStatus::Matched {
                ReportTable::Plain { name, status }
            } else {
                let counts = row_diff_counts(&row.table);
                ReportTable::Matched {
                    name,
                    status,
                    inserts: counts.inserts,
                    updates: counts.updates,
                    deletes: counts.deletes,
                    unchanged_rows: counts.unchanged,
                }
            }
        })
        .collect();
    serde_json::to_string_pretty(&ReportPayload { summary, tables }).unwrap_or_default()
}
