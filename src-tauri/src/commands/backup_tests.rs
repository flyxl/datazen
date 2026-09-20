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

const SOURCE: &str = include_str!("backup.rs");
const BOOTSTRAP_RS: &str = include_str!("../bootstrap/run.rs");

#[test]
fn bootstrap_rs_registers_merged_commands_only() {
    assert!(
        BOOTSTRAP_RS.contains("commands::backup_database,"),
        "merged `backup_database` must stay registered"
    );
    assert!(
        BOOTSTRAP_RS.contains("commands::restore_sql_file,"),
        "`restore_sql_file` must be registered"
    );
    for gone in [
        "commands::backup_database_with_dialog,",
        "commands::restore_database,",
        "commands::restore_database_with_dialog,",
        "commands::execute_sql_file,",
        "commands::execute_sql_file_with_dialog,",
    ] {
        assert!(
            !BOOTSTRAP_RS.contains(gone),
            "`{gone}` must no longer be registered in bootstrap.rs"
        );
    }
}
