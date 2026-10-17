use super::{AppDb, AppDbError};
use datazen_feature_core::dashboard::schedule_grant::MonitorGrant;
use rusqlite::{params, OptionalExtension};
impl AppDb {
    pub(crate) fn monitor_grant(&self, id: &str) -> Result<Option<MonitorGrant>, AppDbError> {
        self.with_conn(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT metadata_json FROM monitor_schedule_grants WHERE id=?1",
                    [id],
                    |row| row.get(0),
                )
                .optional()?;
            json.map(|json| {
                serde_json::from_str(&json)
                    .map_err(|_| AppDbError::Other("invalid monitor grant metadata".into()))
            })
            .transpose()
        })
    }
    pub(crate) fn widget_monitor_grant(
        &self,
        dashboard: &str,
        widget: &str,
    ) -> Result<Option<MonitorGrant>, AppDbError> {
        self.with_conn(|conn| {
            let json: Option<String> = conn.query_row("SELECT metadata_json FROM monitor_schedule_grants WHERE dashboard_id=?1 AND widget_id=?2 ORDER BY rowid DESC LIMIT 1", params![dashboard,widget], |row| row.get(0)).optional()?;
            json.map(|json| serde_json::from_str(&json).map_err(|_| AppDbError::Other("invalid monitor grant metadata".into()))).transpose()
        })
    }
    pub(crate) fn save_monitor_grant(&self, grant: &MonitorGrant) -> Result<(), AppDbError> {
        let json = serde_json::to_string(grant)
            .map_err(|_| AppDbError::Other("invalid monitor grant metadata".into()))?;
        self.with_conn(|conn| { conn.execute("INSERT INTO monitor_schedule_grants(id,dashboard_id,widget_id,metadata_json) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET metadata_json=excluded.metadata_json", params![grant.id,grant.dashboard_id,grant.widget_id,json])?; Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v2_upgrade_preserves_workflows_and_creates_metadata_only_grant_storage() {
        let temp = tempfile::tempdir().unwrap();
        let db = AppDb::open(temp.path()).unwrap();
        db.with_conn(|conn| {
            conn.execute_batch("DROP TABLE monitor_schedule_grants; DELETE FROM schema_migrations WHERE version=3; INSERT INTO workflows(id,name,definition_yaml,updated_at,created_at) VALUES('saved','Saved','id: saved','','');")?;
            Ok(())
        }).unwrap();
        drop(db);
        let db = AppDb::open(temp.path()).unwrap();
        assert_eq!(db.get_workflow("saved").unwrap().name, "Saved");
        assert!(db.widget_monitor_grant("d", "w").unwrap().is_none());
        db.with_conn(|conn| {
            let version: i32 =
                conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(version, 3);
            Ok(())
        })
        .unwrap();
    }
}
