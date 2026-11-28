use datazen_sql_formatter::{format, Dialect, Error, Indent, KeywordCase, LogicalOperatorNewline, Options};
use serde::Deserialize;

#[derive(Deserialize)]
struct Corpus { formatter: String, rows: Vec<Row> }

#[derive(Deserialize)]
struct Row { id: String, input: String, trimmed: String, options: ReferenceOptions, expect: Expect }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceOptions {
    language: String,
    keyword_case: String,
    indent_style: String,
    tab_width: u8,
    use_tabs: bool,
    logical_operator_newline: String,
    lines_between_queries: u8,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Expect {
    Formatted { output: String },
    Rejected,
    BypassBlank,
}

impl From<ReferenceOptions> for Options {
    fn from(r: ReferenceOptions) -> Options {
        assert_eq!(r.indent_style, "standard");
        Options {
            dialect: match r.language.as_str() {
                "sql" => Dialect::Sql,
                "sqlite" => Dialect::Sqlite,
                "postgresql" => Dialect::PostgreSql,
                "mysql" => Dialect::MySql,
                "mariadb" => Dialect::MariaDb,
                "transactsql" => Dialect::TransactSql,
                other => panic!("unexpected reference dialect {other}"),
            },
            keyword_case: match r.keyword_case.as_str() {
                "preserve" => KeywordCase::Preserve,
                "upper" => KeywordCase::Upper,
                "lower" => KeywordCase::Lower,
                other => panic!("unexpected reference case policy {other}"),
            },
            indent: if r.use_tabs {
                assert_eq!(r.tab_width, 1);
                Indent::Tabs
            } else {
                Indent::Spaces(r.tab_width)
            },
            logical_operator_newline: match r.logical_operator_newline.as_str() {
                "before" => LogicalOperatorNewline::Before,
                "after" => LogicalOperatorNewline::After,
                other => panic!("unexpected reference logical-operator layout {other}"),
            },
            lines_between_queries: r.lines_between_queries,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome { Exact, Unsupported, Different, UnexpectedError, IntentionalUnsupported }

#[test]
fn complete_frozen_original_formatter_matrix() {
    let mut n = [0; 5];
    for (raw, expected_count) in [
        (include_str!("reference/golden.json"), 43),
        (include_str!("reference/legacy-mappings.json"), 12),
        (include_str!("reference/golden-ddl.json"), 25),
    ] {
        let corpus: Corpus = serde_json::from_str(raw).unwrap();
        assert!(corpus.formatter.contains("15.8.2"));
        assert_eq!(corpus.rows.len(), expected_count);
        for row in corpus.rows {
            let options = row.options.into();
            let actual = format(&row.input, &options);
            let result = match (actual, &row.expect) {
                // The legacy command emits "/ /" for a client delimiter "//".
                // The native command must preserve the draft, not copy that rewrite.
                (Err(Error::Unsupported), Expect::Formatted {..}) if row.id == "mysql-delimiter-syntax" => {
                    Outcome::IntentionalUnsupported
                }
                (Ok(actual), Expect::Formatted { output }) if actual == *output => Outcome::Exact,
                (Ok(actual), Expect::BypassBlank) if row.trimmed.is_empty() && actual == row.input => Outcome::Exact,
                (Err(Error::Lexical | Error::Syntax), Expect::Rejected) => Outcome::Exact,
                (Err(Error::Unsupported), _) => Outcome::Unsupported,
                (Ok(_), _) => Outcome::Different,
                (Err(_), _) => Outcome::UnexpectedError,
            };
            println!("{} {:?} {:?}", row.id, options.dialect, result);
            n[match result {
                Outcome::Exact => 0,
                Outcome::Unsupported => 1,
                Outcome::Different => 2,
                Outcome::UnexpectedError => 3,
                Outcome::IntentionalUnsupported => 4,
            }] += 1;
        }
    }
    println!("parity results: exact={} unsupported={} different={} other failure={} deliberately unsupported={}",
        n[0], n[1], n[2], n[3], n[4]);
    assert_eq!(n, [79, 0, 0, 0, 1], "frozen corpus has a regression or lost a required safe refusal");
}
