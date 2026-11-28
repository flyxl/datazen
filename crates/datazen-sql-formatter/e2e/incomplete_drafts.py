#!/usr/bin/env python3
"""External consumer acceptance: malformed or unsupported SQL emits no partial document."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

# Common incomplete edits and deliberately unsupported grammar, grouped by
# the original user request (report, schema edit, import/export, etc.). The
# expected API behavior is atomic rejection, not broad dialect validity.
DRAFTS = {
    "sqlite": {
        "unmatched_closing_group": "select id from contact_list);",
        "empty_script_fragment": ";",
        "incomplete_tcl_namespace": "select $catalog::;",
        "unclosed_tcl_parameter": "select $catalog::entry(key;",
        "unquoted_numeric_identifier": "select 123客户 from contacts;",
        "malformed_hex_value": "select 0xGF;",
        "unfinished_exponent": "select 1e+;",
        "non_numeric_exponent_suffix": "select 1e2unit;",
        "data_type_missing_after_column": "alter table contacts add column label default;",
        "unsupported_top_level_maintenance": "vacuum;",
        "incomplete_case_at_end": "select case when id=1 then 'yes';",
        "extra_end_in_select": "select end from clients;",
        "qualified_pragma_missing_value": "pragma main.cache_size = ;",
        "qualified_pragma_extra_arguments": "pragma main.table_info('clients', 3);",
        "projection_missing_column_name": "select clients.;",
        "projection_missing_second_column": "select a,,b from clients;",
        "projection_ends_in_operator": "select balance+ from clients;",
        "pagination_extra_bound": "select id from clients limit 1, 2, 3;",
        "pagination_repeated": "select id from clients limit 2 limit 3;",
        "join_missing_rhs": "select a.id from a left join;",
        "compound_missing_query": "select a from products union all;",
        "compound_loose_alias": "select a from products union all alias;",
        "compound_no_first_query": "union select 1;",
        "repeated_projection": "select sku select labels from products;",
        "out_of_order_source": "select id where id > 20 from products;",
        "empty_case": "select case end from products;",
        "case_premature_comma": "select case when enabled,flag then 'yes' end;",
        "case_repeated_else": "select case when enabled then 'yes' else 'no' else 'other' end;",
        "range_missing_and": "select * from t where n between 1 or 10;",
        "range_no_high_bound": "select * from t where n between 1 and;",
        "comment_without_terminator": "select a /* waiting for closing delimiter",
        "named_bind_without_identifier": "select @# from contacts;",
        "incomplete_tcl_bind": "select $::name from contacts;",
        "unknown_token": "select label from contacts where label 🙂 'hello';",
        "foreign_scope_alias": "select * from (where ready);",
        "upsert_missing_action": "insert into clients(id) values(42) on conflict(id);",
        "upsert_missing_conflict_clause": "insert into clients(id) values(42) on conflict(id) do;",
        "insert_missing_target": "insert into (id) values (1);",
        "values_missing_group": "insert into contacts(id) values 1;",
        "values_missing_comma": "insert into contacts(id) values(1) (2);",
        "update_unexpected_select": "update products select 1;",
        "update_extra_returning": "update products set n=1 returning n returning id;",
        "unsupported_create_as": "create table local_snapshot as select id from customers;",
        "unsupported_trigger": "create trigger audit after insert on clients begin select 1; end;",
        "alter_extra_action": "alter table clients add column note text, drop column label;",
        "missing_alter_table_name": "alter table clients extra add column n integer;",
        "index_incomplete_predicate": "create index i on clients(id) where;",
        "savepoint_without_target": "savepoint;",
        "incomplete_column_decl": "alter table clients add column 1 integer;",
        "invalid_explain_mode": "explain query details select id from clients;",
        "extra_transaction_clause": "begin exclusive transaction where enabled;",
        "too_many_expression_groups": "select " + "(" * 55 + "price" + ")" * 55 + " from products;",
        "nested_case_depth_guard": "select " + "case when 1 then " * 52 + "0" + " end" * 52 + ";",
    },
    "postgresql": {
        "unfinished_dollar_marker": "select $document;",
        "alter_table_storage_clause_not_supported": "alter table contacts set (fillfactor = 70);",
        "ddl_missing_option_type": "explain (where true) select 1;",
        "explain_with_unparenthesized_analyze": "explain analyze select 1;",
        "schema_incomplete_guard": "create schema if analytics;",
        "schema_extra_objects": "create schema analytics authorization analyst create table metrics(id int);",
        "copy_subquery_not_supported": "copy (select 1) to stdout with (format csv);",
        "copy_missing_table": "copy 42 to stdout with (format csv);",
        "copy_with_invalid_column": "copy events(id+1) to stdout with (format text);",
        "copy_with_opaque_target": "copy /* sql-formatter-disable */ events /* sql-formatter-enable */ to stdout with (format csv);",
        "copy_server_file_not_supported": "copy analytics.events to 'events.csv' with (format csv);",
        "copy_extra_qualification": "copy other.db.schema.events to stdout with (format csv);",
        "copy_numeric_format": "copy analytics.events to stdout with (format 3);",
        "copy_unclosed_options": "copy analytics.events to stdout with (format csv, header true;",
        "explain_duplicate_option": "explain (timing on, timing off) select 1;",
        "explain_missing_comma": "explain (verbose true format json) select 1;",
        "explain_non_boolean_option": "explain (analyze sometimes) select 1;",
        "explain_unimplemented_option": "explain (settings true) select 1;",
        "explain_not_select": "explain (format yaml) update clients set id=2;",
        "alter_column_missing_default": "alter table clients alter column label;",
        "alter_column_empty_default": "alter table clients alter column label set default;",
        "unclosed_dollar_tag": "select $document$unfinished;",
        "unfinished_quoted_unicode": "select U&\"folder;",
        "incomplete_escape": "select E'budget\\\nvalue';",
        "line_comment_separator": "select 1 -- copied comment\u2028more;",
        "unclosed_multilevel_comment": "select 1 /* annotation /* more */",
        "extra_drop_target": "drop table clients,accounts;",
    },
    "mysql": {
        "unfinished_quoted_variable": "select @!;",
        "incomplete_qualified_key": "insert into t(id) values(1) on duplicate key update t.id + 1;",
        "delimiter_directive": "delimiter //\ncreate procedure demo() begin select 1; end //",
        "collation_missing_value": "create table contacts(id int) default charset=;",
        "unsupported_storage_option": "create table contacts(id int) compression='lz4';",
        "insert_missing_assignment": "insert into t(n) values(1) on duplicate key update t.n;",
        "insert_invalid_target": "insert into t(n) values(1) on duplicate key update 2='bad';",
        "delete_unimplemented_multi_target": "delete a,b from a inner join b on a.id=b.id;",
        "unfinished_variable": "select @@ from contacts;",
        "unclosed_user_variable": "select @'incomplete from contacts;",
        "create_index_missing_columns": "create index idx on contacts;",
        "unterminated_comment": "select 1 /* open",
    },
    "mariadb": {
        "explain_on_unsupported_ddl": "explain extended select 1;",
        "rollback_missing_savepoint": "rollback to savepoint;",
        "view_check_then_invalid_clause": "create or replace view v as select id from t with check option order by id;",
        "alter_missing_index_cols": "alter table contacts add index name_idx;",
        "unknown_alter_action": "alter table contacts engine=csv;",
        "bad_charset": "create table contacts(id int) default character set;",
    },
    "sql": {
        "unsupported_drop_object": "drop database scratch;",
        "unsupported_procedural_save": "save snapshot;",
        "unimplemented_create_function": "create function f() returns int;",
        "out_of_scope_update": "select a set x=1;",
        "unsupported_partial_with": "with imported as (a) select id from imported;",
        "incomplete_insert_values": "insert into clients(id) values(1),;",
        "incomplete_index_guard": "create unique index if clients_idx on clients(id);",
        "incomplete_ddl_drop_guard": "drop table if clients;",
        "escaped_unterminated_string": "select N'unfinished",
    },
}

REFUSALS = {
    b"unsupported SQL format syntax or dialect\n",
    b"invalid SQL token\n",
    b"invalid SQL syntax\n",
}


def run(binary, output):
    output.mkdir(mode=0o700)
    rows = []
    for dialect, cases in DRAFTS.items():
        for label, draft in cases.items():
            key = dialect + "-" + label
            raw = output / (key + ".profraw")
            # A complete leading SELECT detects erroneous partial output when
            # the second statement is incomplete.
            text = "select  41;\n" + draft
            env = {**os.environ, "LLVM_PROFILE_FILE": str(raw.resolve())}
            run = subprocess.run([binary, "--dialect", dialect], input=text.encode(), env=env,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=8)
            if run.returncode != 1 or run.stdout or run.stderr not in REFUSALS or not raw.is_file() or not raw.stat().st_size:
                raise AssertionError(f"{key} failed atomic rejection: status={run.returncode}, "
                                     f"out={len(run.stdout)} bytes, error={run.stderr[:80]!r}")
            rows.append({"case": key, "input_sha256": hashlib.sha256(text.encode()).hexdigest(),
                         "status": 1, "error": run.stderr.decode().strip(), "profile": raw.name})
    (output / "passing.json").write_text(json.dumps(rows, indent=2) + "\n")
    return len(rows)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path, required=True, help="new results directory")
    args = parser.parse_args()
    print(f"{run(args.binary.resolve(strict=True), args.output.resolve())} external refusal cases passed")
