use std::collections::BTreeMap;

use crate::{build_migration_window_url_params, MigrationWindowKind, MigrationWindowPrefill};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackupMode {
    Backup,
    Restore,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackupPrefill {
    pub connection_id: Option<String>,
    pub database: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SingletonWindowPlan {
    pub label: &'static str,
    pub params: BTreeMap<String, String>,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub title_key: &'static str,
}

fn migration_plan(
    label: &'static str,
    kind: MigrationWindowKind,
    prefill: Option<&MigrationWindowPrefill>,
    width: u32,
    height: u32,
    min_width: u32,
    min_height: u32,
    title_key: &'static str,
) -> SingletonWindowPlan {
    SingletonWindowPlan {
        label,
        params: build_migration_window_url_params(kind, prefill),
        width,
        height,
        min_width,
        min_height,
        title_key,
    }
}

pub fn data_sync_window_options(prefill: Option<&MigrationWindowPrefill>) -> SingletonWindowPlan {
    migration_plan(
        "data-sync-singleton",
        MigrationWindowKind::DataSync,
        prefill,
        1000,
        700,
        600,
        480,
        "common.dataSyncTitle",
    )
}

pub fn data_transfer_window_options(
    prefill: Option<&MigrationWindowPrefill>,
) -> SingletonWindowPlan {
    migration_plan(
        "data-transfer-singleton",
        MigrationWindowKind::DataTransfer,
        prefill,
        1000,
        720,
        640,
        480,
        "win.dataTransfer",
    )
}

pub fn schema_diff_window_options(prefill: Option<&MigrationWindowPrefill>) -> SingletonWindowPlan {
    migration_plan(
        "schema-diff-singleton",
        MigrationWindowKind::SchemaDiff,
        prefill,
        900,
        640,
        560,
        420,
        "win.schemaDiff",
    )
}

pub fn backup_window_options(
    mode: BackupMode,
    prefill: Option<&BackupPrefill>,
) -> SingletonWindowPlan {
    let restore = mode == BackupMode::Restore;
    let mut params = BTreeMap::new();
    params.insert("window".to_owned(), "backup".to_owned());
    if restore {
        params.insert("mode".to_owned(), "restore".to_owned());
    }
    if let Some(prefill) = prefill {
        if let Some(connection_id) = prefill.connection_id.as_deref().filter(|id| !id.is_empty()) {
            params.insert("connectionId".to_owned(), connection_id.to_owned());
        }
        if let Some(database) = prefill.database.as_deref().filter(|db| !db.is_empty()) {
            params.insert("database".to_owned(), database.to_owned());
        }
    }
    SingletonWindowPlan {
        label: if restore {
            "backup-restore-singleton"
        } else {
            "backup-singleton"
        },
        params,
        width: 750,
        height: 520,
        min_width: 600,
        min_height: 400,
        title_key: if restore { "win.restore" } else { "win.backup" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_windows_have_distinct_labels_sizes_and_titles() {
        let sync = data_sync_window_options(None);
        assert_eq!(sync.label, "data-sync-singleton");
        assert_eq!(
            (sync.width, sync.height, sync.min_width, sync.min_height),
            (1000, 700, 600, 480)
        );
        assert_eq!(sync.title_key, "common.dataSyncTitle");
        assert_eq!(
            sync.params.get("window").map(String::as_str),
            Some("data-sync")
        );

        let transfer = data_transfer_window_options(None);
        assert_eq!(transfer.label, "data-transfer-singleton");
        assert_eq!((transfer.width, transfer.height), (1000, 720));
        assert_eq!(transfer.title_key, "win.dataTransfer");
        assert_eq!(
            transfer.params.get("window").map(String::as_str),
            Some("data-transfer")
        );

        let diff = schema_diff_window_options(None);
        assert_eq!(diff.label, "schema-diff-singleton");
        assert_eq!(
            (diff.width, diff.height, diff.min_width, diff.min_height),
            (900, 640, 560, 420)
        );
        assert_eq!(diff.title_key, "win.schemaDiff");
        assert_eq!(
            diff.params.get("window").map(String::as_str),
            Some("schema-diff")
        );
    }

    #[test]
    fn migration_prefill_is_forwarded_into_url_params() {
        let prefill = MigrationWindowPrefill {
            source_id: Some("src".to_owned()),
            ..Default::default()
        };
        let plan = data_sync_window_options(Some(&prefill));
        assert_eq!(plan.params.get("sourceId").map(String::as_str), Some("src"));
    }

    #[test]
    fn backup_mode_selects_label_title_and_restore_param() {
        let backup = backup_window_options(BackupMode::Backup, None);
        assert_eq!(backup.label, "backup-singleton");
        assert_eq!(backup.title_key, "win.backup");
        assert_eq!(
            backup.params.get("window").map(String::as_str),
            Some("backup")
        );
        assert!(!backup.params.contains_key("mode"));
        assert_eq!(
            (
                backup.width,
                backup.height,
                backup.min_width,
                backup.min_height
            ),
            (750, 520, 600, 400)
        );

        let restore = backup_window_options(BackupMode::Restore, None);
        assert_eq!(restore.label, "backup-restore-singleton");
        assert_eq!(restore.title_key, "win.restore");
        assert_eq!(
            restore.params.get("mode").map(String::as_str),
            Some("restore")
        );
    }

    #[test]
    fn backup_prefill_includes_only_nonblank_fields() {
        let prefill = BackupPrefill {
            connection_id: Some("conn-1".to_owned()),
            database: Some("  ".to_owned()),
        };
        let plan = backup_window_options(BackupMode::Restore, Some(&prefill));
        assert_eq!(
            plan.params.get("connectionId").map(String::as_str),
            Some("conn-1")
        );
        assert_eq!(plan.params.get("database").map(String::as_str), Some("  "));
    }
}
