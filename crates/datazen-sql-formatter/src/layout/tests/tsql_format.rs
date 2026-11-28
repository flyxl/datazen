use crate::{format, Dialect, Error, Options};

fn format_tsql(sql: &str) -> Result<String, Error> {
    format(sql, &Options { dialect: Dialect::TransactSql, ..Options::default() })
}

#[test]
fn tsql_unit_formats_batches_and_recognizes_only_standalone_go() {
    assert_eq!(format_tsql("select 1;\ngo\nselect 2;").as_deref(), Ok("SELECT\n  1;\n\nGO\nSELECT\n  2;"));
    assert_eq!(format_tsql("select 1\ngo\nselect 2;").as_deref(), Ok("SELECT\n  1\nGO\nSELECT\n  2;"));
    for malformed in ["GO", "select 1 GO\nselect 2;", "select 1;\ngo 2", "select 1;\ngo;"] {
        assert_eq!(format_tsql(malformed), Err(Error::Unsupported), "input {malformed:?}");
    }
}

#[test]
fn tsql_unit_handles_the_qualified_names_and_quotes_without_rewriting_literals() {
    let sql = r#"select N'猫', "Report", @[q]]q], {fn foo()}, x::y from dbo..[Ledger] where x !< 5;"#;
    let expected = "SELECT\n  N'猫',\n  \"Report\",\n  @[q]]q],\n  {fn foo()},\n  x::y\nFROM\n  dbo..[Ledger]\nWHERE\n  x !< 5;";
    assert_eq!(format_tsql(sql).as_deref(), Ok(expected));
    assert_eq!(format_tsql("select @[missing;"), Err(Error::Lexical));
    // While an @ parameter name is being typed, a following separator
    // must not be swallowed by the quoted-placeholder path.
    assert_eq!(format_tsql("select @;").as_deref(), Ok("SELECT\n  @;"));
    assert_eq!(format_tsql("select #staged, @global from [#temp];").as_deref(),
               Ok("SELECT\n  #staged,\n  @global\nFROM\n  [#temp];"));
}

#[test]
fn tsql_unit_distinguishes_transactions_from_procedural_begin() {
    assert_eq!(format_tsql("begin transaction t;save transaction savepoint_1;rollback transaction savepoint_1;commit;").as_deref(),
        Ok("BEGIN TRANSACTION t;\n\nSAVE TRANSACTION savepoint_1;\n\nROLLBACK TRANSACTION savepoint_1;\n\nCOMMIT;"));
    assert_eq!(format_tsql("begin;"), Err(Error::Unsupported));
    assert_eq!(format_tsql("rollback;").as_deref(), Ok("ROLLBACK;"));
}
