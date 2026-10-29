//! Saved workflows: the records themselves and the CRUD over them.

use super::dashboards::DashboardWorkflowRef;
use super::{AppDb, AppDbError};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkflowVisibility {
    User,
    DashboardHidden,
}

impl WorkflowVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::DashboardHidden => "dashboardHidden",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(Self::User),
            "dashboardHidden" => Some(Self::DashboardHidden),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub visibility: WorkflowVisibility,
    pub definition_yaml: String,
    pub created_at: String,
    pub updated_at: String,
}

impl AppDb {
    pub fn upsert_workflow(&self, record: &WorkflowRecord) -> Result<(), AppDbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO workflows
                    (id, name, description, visibility, definition_yaml, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    description = excluded.description,
                    visibility = excluded.visibility,
                    definition_yaml = excluded.definition_yaml,
                    updated_at = excluded.updated_at",
                params![
                    record.id,
                    record.name,
                    record.description,
                    record.visibility.as_str(),
                    record.definition_yaml,
                    record.created_at,
                    record.updated_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_workflow(&self, id: &str) -> Result<WorkflowRecord, AppDbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, description, visibility, definition_yaml, created_at, updated_at
                 FROM workflows WHERE id = ?1",
                params![id],
                map_workflow_row,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => AppDbError::NotFound(id.into()),
                other => AppDbError::from(other),
            })
        })
    }

    pub fn list_workflows(
        &self,
        visibility: Option<WorkflowVisibility>,
    ) -> Result<Vec<WorkflowRecord>, AppDbError> {
        self.with_conn(|conn| {
            let mut rows = Vec::new();
            if let Some(v) = visibility {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, visibility, definition_yaml, created_at, updated_at
                     FROM workflows WHERE visibility = ?1 ORDER BY name COLLATE NOCASE",
                )?;
                let iter = stmt.query_map(params![v.as_str()], map_workflow_row)?;
                for row in iter {
                    rows.push(row?);
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, visibility, definition_yaml, created_at, updated_at
                     FROM workflows ORDER BY name COLLATE NOCASE",
                )?;
                let iter = stmt.query_map([], map_workflow_row)?;
                for row in iter {
                    rows.push(row?);
                }
            }
            Ok(rows)
        })
    }

    pub fn delete_workflow(&self, id: &str) -> Result<(), AppDbError> {
        let refs = self.find_workflow_refs(id)?;
        if !refs.is_empty() {
            let summary = refs
                .iter()
                .map(|r| format!("{} / {}", r.dashboard_name, r.widget_title))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(AppDbError::WorkflowInUse(summary));
        }
        self.with_conn(|conn| {
            let n = conn.execute("DELETE FROM workflows WHERE id = ?1", params![id])?;
            if n == 0 {
                return Err(AppDbError::NotFound(id.into()));
            }
            Ok(())
        })
    }

    pub fn find_workflow_refs(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<DashboardWorkflowRef>, AppDbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT w.workflow_id, w.dashboard_id, w.id, d.name, w.title
                 FROM widgets w
                 JOIN dashboards d ON d.id = w.dashboard_id
                 WHERE w.workflow_id = ?1
                 ORDER BY d.name, w.title",
            )?;
            let iter = stmt.query_map(params![workflow_id], |row| {
                Ok(DashboardWorkflowRef {
                    workflow_id: row.get(0)?,
                    dashboard_id: row.get(1)?,
                    widget_id: row.get(2)?,
                    dashboard_name: row.get(3)?,
                    widget_title: row.get(4)?,
                })
            })?;
            let mut refs = Vec::new();
            for row in iter {
                refs.push(row?);
            }
            Ok(refs)
        })
    }

    // ── Dashboards ────────────────────────────────────────────────────────
}

fn map_workflow_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRecord> {
    let visibility_raw: String = row.get(3)?;
    let visibility = WorkflowVisibility::parse(&visibility_raw).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("bad visibility: {visibility_raw}"),
            )),
        )
    })?;
    Ok(WorkflowRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        visibility,
        definition_yaml: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
