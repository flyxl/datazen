//! Actual lexical + layout differential for the closed definition grammar.
//! Expected SQL is from the exact pinned 15.8.2 development reference.
//! The delimiter case intentionally rejects the legacy delimiter rewrite.
use super::*;
use crate::{lexer, Limits};

fn check(id: &str, dialect: Dialect, case: KeywordCase, indent: Indent, gaps: u8,
         input: &str, expected: Result<&str, Error>) {
    let deadline = Instant::now() + Duration::from_secs(3);
    let options = Options {
        dialect, keyword_case: case, indent, lines_between_queries: gaps,
        logical_operator_newline: LogicalOperatorNewline::Before,
    };
    let limits = Limits { deadline };
    let actual = lexer::tokenize(input, dialect, &limits)
        .and_then(|tokens| format_admitted_tokens(&tokens, &options, 1024 * 1024, deadline));
    assert_eq!(actual.as_deref(), expected.as_deref(), "{id}");
}

#[test]
fn exact_closed_definition_reference() {
    check("sqlite-create-table", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"create table if not exists accounts (id integer primary key, name text not null, balance numeric(9,2) default 0, check(balance>=0));"###,
         Ok(r###"CREATE TABLE IF NOT EXISTS accounts (
  id integer PRIMARY KEY,
  name text NOT NULL,
  balance numeric(9, 2) DEFAULT 0,
  CHECK (balance >= 0)
);"###));
    check("sqlite-create-index", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"create unique index if not exists account_name on accounts(name collate nocase) where name is not null;"###,
         Ok(r###"CREATE UNIQUE INDEX IF NOT EXISTS account_name ON accounts (name COLLATE nocase)
WHERE
  name IS NOT NULL;"###));
    check("sqlite-create-view", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"create view active_accounts as select id,name from accounts where balance>0;"###,
         Ok(r###"CREATE VIEW active_accounts AS
SELECT
  id,
  name
FROM
  accounts
WHERE
  balance > 0;"###));
    check("sqlite-alter-add-column", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"alter table accounts add column enabled integer default 1 not null;"###,
         Ok(r###"ALTER TABLE accounts
ADD COLUMN enabled integer DEFAULT 1 NOT NULL;"###));
    check("sqlite-drop-if-exists", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 0,
         r###"drop view if exists active_accounts;drop index if exists account_name;drop table if exists accounts;"###,
         Ok(r###"DROP VIEW IF EXISTS active_accounts;
DROP INDEX IF EXISTS account_name;
DROP TABLE IF EXISTS accounts;"###));
    check("sqlite-explain-query-plan", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"explain query plan select a.id,b.name from accounts a join ledger b on a.id=b.account_id where a.balance>0;"###,
         Ok(r###"EXPLAIN QUERY PLAN
SELECT
  a.id,
  b.name
FROM
  accounts a
  JOIN ledger b ON a.id = b.account_id
WHERE
  a.balance > 0;"###));
    check("sqlite-explain", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"explain select 1+2;"###,
         Ok(r###"EXPLAIN
SELECT
  1 + 2;"###));
    check("sqlite-transaction", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"begin immediate transaction;insert into t values(1);commit;begin;rollback;"###,
         Ok(r###"BEGIN IMMEDIATE TRANSACTION;

INSERT INTO
  t
VALUES
  (1);

COMMIT;

BEGIN;

ROLLBACK;"###));
    check("sqlite-savepoints", Dialect::Sqlite, KeywordCase::Lower, Indent::Tabs, 1,
         r###"savepoint inner_edit;update t set n=2 where id=1;rollback to savepoint inner_edit;release savepoint inner_edit;"###,
         Ok(r###"savepoint inner_edit;

update t
set
	n = 2
where
	id = 1;

rollback to savepoint inner_edit;

release savepoint inner_edit;"###));
    check("sqlite-cjk-schema", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(4), 1,
         r###"create table "账户"("编号" integer primary key,"名称" text default '上海;😀');create index "名称索引" on "账户"("名称");"###,
         Ok(r###"CREATE TABLE "账户" (
    "编号" integer PRIMARY KEY,
    "名称" text DEFAULT '上海;😀'
);

CREATE INDEX "名称索引" ON "账户" ("名称");"###));
    check("sqlite-unterminated-delimiter", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"create table broken(name text default 'never closes);"###,
         Err(Error::Lexical));
    check("sqlite-unbalanced-parenthesis", Dialect::Sqlite, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"create table broken(id integer, check((id>0));"###,
         Err(Error::Syntax));
    check("pg-create-table-references", Dialect::PostgreSql, KeywordCase::Upper, Indent::Spaces(4), 1,
         r###"create table analytics.events(id bigint generated always as identity primary key, payload jsonb default '{}'::jsonb, parent_id bigint references analytics.events(id) on delete set null);"###,
         Ok(r###"CREATE TABLE analytics.events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    payload jsonb DEFAULT '{}'::jsonb,
    parent_id bigint REFERENCES analytics.events (id) ON DELETE SET NULL
);"###));
    check("pg-alter-and-drop", Dialect::PostgreSql, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"alter table analytics.events alter column payload set default '{}'::jsonb;drop table if exists analytics.events cascade;"###,
         Ok(r###"ALTER TABLE analytics.events
ALTER COLUMN payload
SET DEFAULT '{}'::jsonb;

DROP TABLE IF EXISTS analytics.events cascade;"###));
    check("mysql-create-table-engine", Dialect::MySql, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"create table `ledger`(`id` bigint unsigned not null auto_increment,`label` varchar(80) default 'memo',primary key(`id`)) engine=InnoDB default charset=utf8mb4;"###,
         Ok(r###"CREATE TABLE `ledger` (
  `id` bigint UNSIGNED NOT NULL auto_increment,
  `label` varchar(80) default 'memo',
  PRIMARY KEY (`id`)
) engine = InnoDB default charset = utf8mb4;"###));
    check("mysql-alter-index", Dialect::MySql, KeywordCase::Lower, Indent::Tabs, 1,
         r###"alter table `ledger` add index `ix_label`(`label`),rename column `label` to `memo`;"###,
         Ok(r###"alter table `ledger`
add index `ix_label` (`label`),
rename column `label` to `memo`;"###));
    check("mysql-delimiter-syntax", Dialect::MySql, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"delimiter //
create procedure summary() begin select 1; end //
delimiter ;"###,
         Err(Error::Unsupported));
    check("mariadb-create-or-replace-view", Dialect::MariaDb, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"create or replace view `billed` as select id,n as amount from `ledger` where n>0 with check option;"###,
         Ok(r###"CREATE OR REPLACE VIEW `billed` AS
SELECT
  id,
  n AS amount
FROM
  `ledger`
WHERE
  n > 0
WITH
  CHECK OPTION;"###));
    check("mariadb-transactions", Dialect::MariaDb, KeywordCase::Upper, Indent::Spaces(2), 2,
         r###"start transaction; savepoint edit_one; rollback to savepoint edit_one; commit;"###,
         Ok(r###"START TRANSACTION;


SAVEPOINT edit_one;


ROLLBACK TO
SAVEPOINT edit_one;


COMMIT;"###));
    check("tsql-create-alter-go", Dialect::TransactSql, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"create table [dbo].[Ledger]([id] int identity(1,1) not null primary key,[name] nvarchar(60) default N'中文');
go
alter table [dbo].[Ledger] add [active] bit not null default 1;
GO"###,
         Ok(r###"CREATE TABLE [dbo].[Ledger] (
  [id] int identity(1, 1) NOT NULL PRIMARY KEY,
  [name] nvarchar(60) DEFAULT N'中文'
);

GO
ALTER TABLE [dbo].[Ledger]
ADD [active] bit NOT NULL DEFAULT 1;

GO"###));
    check("tsql-transaction-and-drop", Dialect::TransactSql, KeywordCase::Upper, Indent::Spaces(2), 1,
         r###"begin transaction t;save transaction edit_one;rollback transaction edit_one;commit transaction t;drop table if exists [dbo].[Ledger];"###,
         Ok(r###"BEGIN TRANSACTION t;

SAVE TRANSACTION edit_one;

ROLLBACK TRANSACTION edit_one;

COMMIT TRANSACTION t;

DROP TABLE IF EXISTS [dbo].[Ledger];"###));
    check("sql-ddl-transaction-preserve", Dialect::Sql, KeywordCase::Preserve, Indent::Spaces(4), 1,
         r###"CrEaTe TaBlE inventory(id integer not null,unique(id));AlTeR TaBlE inventory DrOp CoLuMn id;RoLlBaCk;"###,
         Ok(r###"CrEaTe TaBlE inventory (id integer not null, unique (id));

AlTeR TaBlE inventory
DrOp CoLuMn id;

RoLlBaCk;"###));
}

#[test]
fn unrecognized_statement_shapes_and_client_directives_stay_unsupported() {
    for (id, dialect, sql) in [
        ("second-statement-client-delimiter", Dialect::MySql, "select 1;\nDELIMITER //"),
        ("mysql-mariadb-client-directive", Dialect::MariaDb, "DELIMITER //\nBEGIN"),
        ("create-table-as-not-admitted", Dialect::Sqlite, "create table tmp as select 1;"),
        ("create-procedure-not-admitted", Dialect::MySql, "create procedure x() select 1;"),
        ("create-trigger-not-admitted", Dialect::Sqlite, "create trigger watch after insert on t begin select 1; end;"),
        ("multi-drop-not-admitted", Dialect::PostgreSql, "drop table a,b;"),
        ("tds-begin-is-not-a-transaction", Dialect::TransactSql, "begin;"),
        ("alter-unknown-action-not-admitted", Dialect::Sqlite, "alter table t nonsense x;"),
        ("view-subquery-must-be-real", Dialect::Sqlite, "create view v as;"),
    ] {
        check(id, dialect, KeywordCase::Upper, Indent::Spaces(2), 1, sql, Err(Error::Unsupported));
    }
    // A real column named delimiter is not a client directive.
    check("column-named-delimiter", Dialect::MySql, KeywordCase::Upper, Indent::Spaces(2), 1,
        "select delimiter from t;", Ok("SELECT\n  delimiter\nFROM\n  t;"));
}

#[test]
fn exact_postgres_schema_explain_and_stdout_copy() {
    check("pg-create-schema", Dialect::PostgreSql, KeywordCase::Upper, Indent::Spaces(2), 1,
        r###"create schema if not exists analytics authorization analyst;"###,
        Ok(r###"CREATE SCHEMA if NOT EXISTS analytics AUTHORIZATION analyst;"###));
    check("pg-copy-options", Dialect::PostgreSql, KeywordCase::Upper, Indent::Spaces(2), 1,
        r###"copy analytics.events(id,payload) to stdout with (format csv, header true, delimiter ',');"###,
        Ok(r###"COPY analytics.events (id, payload) TO stdout
WITH
  (format csv, header TRUE, delimiter ',');"###));
    check("pg-explain-options", Dialect::PostgreSql, KeywordCase::Upper, Indent::Spaces(2), 1,
        r###"explain (analyze false, costs true, format json) select payload->>'name' from analytics.events where payload @> '{"enabled":true}'::jsonb;"###,
        Ok(r###"EXPLAIN (
  ANALYZE FALSE,
  costs TRUE,
  format json
)
SELECT
  payload ->> 'name'
FROM
  analytics.events
WHERE
  payload @> '{"enabled":true}'::jsonb;"###));
    check("pg-explain-option-comment", Dialect::PostgreSql, KeywordCase::Upper, Indent::Spaces(2), 1,
        "explain (analyze /* mode */ false, costs true) select 1;",
        Ok("EXPLAIN (\n  ANALYZE /* mode */ FALSE,\n  costs TRUE\n)\nSELECT\n  1;"));
    check("pg-copy-quoted-target-and-line-comment", Dialect::PostgreSql, KeywordCase::Upper, Indent::Spaces(2), 1,
        "copy \"catalog\".\"事件\"(id) to stdout with (format csv, -- data follows options\nheader true);",
        Ok("COPY \"catalog\".\"事件\" (id) TO stdout\nWITH\n  (\n    format csv, -- data follows options\n    header TRUE\n  );"));
}

#[test]
fn postgres_copy_never_admits_client_data_programs_or_paths() {
    for (id, sql) in [
        ("stdin-with-raw-numeric-data", "copy t from stdin;\n1,2\n"),
        ("stdin-with-options", "copy t from stdin with (format csv);"),
        ("external-path", "copy t to '/tmp/out.csv' with (format csv);"),
        ("external-program", "copy t to program 'cat' with (format csv);"),
        ("query-target", "copy (select 1) to stdout with (format csv);"),
        ("qualified-name-too-deep", "copy x.y.z.t to stdout with (format csv);"),
        ("unknown-setting", "copy t to stdout with (encoding 'UTF8');"),
        ("duplicate-mode", "copy t to stdout with (format csv, format text);"),
        ("delimiter-is-not-single-byte", "copy t to stdout with (delimiter '😀');"),
        ("opaque-option", "copy t to stdout with (/* sql-formatter-disable */format csv/* sql-formatter-enable */);"),
        ("unsupported-explain-subject", "explain (verbose true) update t set n=1;"),
        ("unsupported-explain-option", "explain (memory true) select 1;"),
        ("additional-schema-statement", "create schema s authorization a create table t(id int);"),
    ] {
        check(id, Dialect::PostgreSql, KeywordCase::Upper, Indent::Spaces(2), 1, sql, Err(Error::Unsupported));
    }
}
