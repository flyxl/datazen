//! 备份/恢复选项解析与过滤校验的行为钉子（2026-10-31 自桌面
//! `commands/backup_tests.rs` 归位：被测函数一直在 `datazen::backup`，
//! 测试却住在宿主命令层——单元测试跟着逻辑走）。

use super::*;

#[test]
fn parse_backup_options_recognizes_aliases() {
    let opts = parse_backup_options(&[
        "schema-only".into(),
        "clean".into(),
        "create".into(),
        "no-owner".into(),
        "single-transaction".into(),
        "routines".into(),
        "triggers".into(),
    ]);
    let opts = opts.unwrap();
    assert!(opts.schema_only);
    assert!(opts.clean);
    assert!(opts.create_database);
    assert!(opts.no_owner);
    assert!(opts.single_transaction);
    assert!(opts.routines);
    assert!(opts.triggers);
    assert!(!opts.data_only);
}

#[test]
fn parse_backup_options_rejects_format_custom() {
    let err = parse_backup_options(&["format-custom".into()]).unwrap_err();
    assert!(err.to_string().contains("format-custom"));
}

#[test]
fn validate_backup_filter_extension_allows_known() {
    for ext in ["sql", ".sql", "gz", "dump"] {
        assert!(validate_backup_filter_extension(ext).is_ok());
    }
    assert!(validate_backup_filter_extension("exe").is_err());
}

#[test]
fn validate_backup_filter_extension_accepts_sql_gz_dump() {
    assert_eq!(validate_backup_filter_extension("sql").unwrap(), "sql");
    assert_eq!(validate_backup_filter_extension(".GZ").unwrap(), "gz");
    assert!(validate_backup_filter_extension("exe").is_err());
}

#[test]
fn parse_restore_options_recognizes_single_transaction() {
    let opts = parse_restore_options(&["single-transaction".into()]);
    assert!(opts.single_transaction);
    assert!(!parse_restore_options(&[]).single_transaction);
    assert!(!opts.overwrite);
    assert!(parse_restore_options(&["overwrite".into()]).overwrite);
}
