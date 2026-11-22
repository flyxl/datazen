//! T572: `navigator/treeRowAria.ts` + `navigator/types.ts` port — the
//! equivalence gate, ported from React's `navigatorTreeRowAriaEquivalence.test.ts`.
//!
//! The shared virtual-tree shell knows nothing about connections, databases
//! or object categories. Everything a screen reader hears about a navigator
//! row is decided by two host-owned pieces — the builder (which row exists,
//! how deep it is painted) and `navigator_row_aria` (what it announces) — and
//! the shell only converts `level_depth` into a 1-based `aria-level`. That
//! leaves the tree's accessibility resting on two facts that are easy to
//! state and easy to break:
//!
//!  1. **Every variant announces something.** A new row type that forgets to
//!     return ARIA renders silently and is heard as a leaf.
//!  2. **Search changes the announced level by exactly one, and only for the
//!     rows that lost a parent.** A global object search drops the group
//!     header between a connection and its databases, so every row below
//!     announces one rung higher while still painting at the same indent.
//!
//! The second is the subtle one, and the reason indent and level are asserted
//! separately throughout: the two numbers are *supposed* to disagree under a
//! search, and "harmonising" them is the regression this file exists to catch.

use std::collections::{HashMap, HashSet};

use crate::views::navigator::build_flat_rows::deps::locator::{
    PINNED_GROUP_KEY, RECENT_GROUP_KEY,
};
use crate::views::navigator::build_flat_rows::inputs::{
    ActiveEntrySnapshot, ConnectionSnapshot, GroupedSection, SchemaStateSnapshot, TableInfo,
    TableType,
};
use crate::views::navigator::build_flat_rows::{build_navigator_flat_rows, BuildFlatRowsParams};
use crate::views::navigator::tree_row_aria::{
    aria_level_of, connection_expanded_state, is_navigator_branch, navigator_row_aria,
    VirtualTreeItemAria, TREE_TOP_LEVEL,
};
use crate::views::navigator::types::NavigatorRow;

/// React `indentOf(row.depth, INDENT_BASE_REM = 0, INDENT_STEP_REM = 0.75)`,
/// in rem — the number the paint side consumes.
fn indent_rem(row: &NavigatorRow) -> f32 {
    row.depth() as f32 * 0.75
}

/// The test's `Dump`: type, label, level (`None` for a decoration), expanded
/// (`None` when the attribute is absent — not the same as `false`), indent.
struct Dump {
    kind: &'static str,
    label: String,
    level: Option<usize>,
    expanded: Option<bool>,
    indent_rem: f32,
}

/// A readable stand-in for the label the row paints (React `rowKeyLabel`).
fn row_key_label(row: &NavigatorRow) -> String {
    match row {
        NavigatorRow::Section { section, .. } => (*section).to_owned(),
        NavigatorRow::Group { group_name, .. } => group_name.clone(),
        NavigatorRow::Connection { id, .. } => id.clone(),
        NavigatorRow::Db { connection_id, db_name, .. }
        | NavigatorRow::KvDb { connection_id, db_name, .. } => {
            format!("{connection_id}/{db_name}")
        }
        NavigatorRow::Schema { schema_name, .. } => schema_name.clone(),
        NavigatorRow::Category { key, .. } => key.clone(),
        NavigatorRow::Table { item, .. } => item.name.clone(),
        NavigatorRow::Object { obj, cat_id, .. } => format!("{}#{cat_id}", obj.name),
        NavigatorRow::DbLoading { owner_key, .. } => owner_key.clone(),
        NavigatorRow::DbError { message, .. } => message.clone(),
        NavigatorRow::NamespaceNode { key, .. } => key.clone(),
        NavigatorRow::EmptyGroup { group_name, .. } => format!("empty:{group_name}"),
        NavigatorRow::NoConnections { .. } => "no-connections".to_owned(),
    }
}

fn dump(rows: &[NavigatorRow]) -> Vec<Dump> {
    rows.iter()
        .map(|row| {
            let aria = navigator_row_aria(row);
            Dump {
                kind: row.kind(),
                label: row_key_label(row),
                level: aria.map(|a: VirtualTreeItemAria| a.level),
                expanded: aria.and_then(|a| a.expanded),
                indent_rem: indent_rem(row),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The variant table — every variant, written out rather than produced by the
// builder.
//
// The builder only emits `kv-db`, the key-value `db-loading` and
// `namespace-node` for drivers whose meta says "key-value" or "path
// hierarchy", which depends on the generated driver registry. Asserting the
// ARIA contract against a registry-gated fixture would make it silently
// vacuous on a basic build, so the variant table is literal: the union is the
// spec, and this is the spec written down. The builder gets its own section
// below.
// ---------------------------------------------------------------------------

const DEPTH: usize = 2;

fn variants() -> Vec<NavigatorRow> {
    let dp = |depth: usize| (depth, depth);
    vec![
        NavigatorRow::Section {
            depth_pair: dp(0),
            section: "pinned",
            display_name: "Pinned".to_owned(),
            count: 1,
            expanded: true,
        },
        NavigatorRow::Group {
            depth_pair: dp(0),
            group_name: "team-a".to_owned(),
            display_name: "Team A".to_owned(),
            count: 1,
            expanded: true,
        },
        NavigatorRow::Connection {
            depth_pair: dp(DEPTH),
            id: "c1".to_owned(),
            name: "c1".to_owned(),
            engine: "postgres".to_owned(),
            section_group: "team-a".to_owned(),
            is_selected: true,
            status: "connected".to_owned(),
            expanded: true,
            match_payload: None,
        },
        NavigatorRow::Db {
            depth_pair: dp(DEPTH + 1),
            connection_id: "c1".to_owned(),
            db_session_id: "s1".to_owned(),
            db_name: "db1".to_owned(),
            expanded: true,
            loading: false,
            is_open: true,
        },
        NavigatorRow::Schema {
            depth_pair: dp(DEPTH + 2),
            connection_id: "c1".to_owned(),
            db_name: "db1".to_owned(),
            schema_name: "public".to_owned(),
            expanded: true,
        },
        NavigatorRow::Category {
            depth_pair: dp(DEPTH + 3),
            key: "c1::db1::public::tables".to_owned(),
            db_name: "db1".to_owned(),
            cat_id: "tables",
            cat_label_key: "schemaTree.tables",
            count: 1,
            expanded: true,
        },
        NavigatorRow::Table {
            depth_pair: dp(DEPTH + 4),
            item: TableInfo {
                name: "users".to_owned(),
                schema: Some("public".to_owned()),
                table_type: TableType::Table,
            },
            cat_id: "tables",
            is_selected: false,
            connection_id: "c1".to_owned(),
            db_session_id: "s1".to_owned(),
            db_name: "db1".to_owned(),
        },
        NavigatorRow::Object {
            depth_pair: dp(DEPTH + 4),
            obj: crate::views::navigator::build_flat_rows::inputs::DatabaseObject {
                kind: "function".to_owned(),
                schema: None,
                name: "fn_calc".to_owned(),
            },
            cat_id: "routines",
            connection_id: "c1".to_owned(),
            db_name: "db1".to_owned(),
            schema_name: Some("public".to_owned()),
        },
        NavigatorRow::KvDb {
            depth_pair: dp(DEPTH + 1),
            connection_id: "c2".to_owned(),
            db_session_id: "s2".to_owned(),
            db_name: "db0".to_owned(),
            is_selected: false,
            db_counts_command: None,
        },
        NavigatorRow::DbLoading {
            depth_pair: dp(DEPTH + 1),
            owner_key: "conn:c2".to_owned(),
        },
        NavigatorRow::NamespaceNode {
            depth_pair: dp(DEPTH + 2),
            name: "tbl".to_owned(),
            expanded: false,
            is_leaf: false,
            leaf_kind: None,
            segments: vec!["tbl".to_owned()],
            key: "ns:db1::tbl".to_owned(),
            connection_id: "c3".to_owned(),
            db_session_id: "s3".to_owned(),
        },
        NavigatorRow::EmptyGroup {
            depth_pair: dp(1),
            group_name: "team-b".to_owned(),
        },
        NavigatorRow::NoConnections { depth_pair: dp(0) },
    ]
}

const ALL_TYPES: [&str; 13] = [
    "section",
    "group",
    "connection",
    "db",
    "schema",
    "category",
    "table",
    "object",
    "kv-db",
    "db-loading",
    "namespace-node",
    "empty-group",
    "no-connections",
];

const DECORATIONS: [&str; 2] = ["empty-group", "no-connections"];
/// Rows that own no child list, so `aria-expanded` must be absent, not `false`.
const LEAVES: [&str; 4] = ["table", "object", "kv-db", "db-loading"];

fn is_decoration(kind: &str) -> bool {
    DECORATIONS.contains(&kind)
}

// ---------------------------------------------------------------------------
// ARIA contract
// ---------------------------------------------------------------------------

#[test]
fn covers_all_13_variants() {
    let rows = variants();
    let mut seen: Vec<&str> = rows.iter().map(|r| r.kind()).collect();
    seen.sort_unstable();
    let mut all = ALL_TYPES;
    all.sort_unstable();
    assert_eq!(seen.len(), 13);
    assert_eq!(seen, all);
}

#[test]
fn announces_a_role_and_a_level_for_every_variant_except_the_two_decorations() {
    for row in variants() {
        let kind = row.kind();
        let aria = navigator_row_aria(&row);
        if is_decoration(kind) {
            assert!(
                aria.is_none(),
                "{kind} must stay out of the tree structure"
            );
        } else {
            let aria = aria.unwrap_or_else(|| panic!("{kind} announced nothing"));
            assert_eq!(aria.role, "treeitem", "{kind} announced no role");
            assert_eq!(
                aria.level,
                row.level_depth() + TREE_TOP_LEVEL,
                "{kind} announced no level"
            );
        }
    }
}

#[test]
fn starts_at_level_1_because_aria_level_is_1_based_while_depth_is_0_based() {
    assert_eq!(TREE_TOP_LEVEL, 1);
    for row in variants() {
        if is_decoration(row.kind()) {
            continue;
        }
        // One conversion, in one place: `aria_level_of`. Asserting the
        // arithmetic here is what stops a renderer from growing a second
        // opinion.
        assert_eq!(
            aria_level_of(&row),
            row.level_depth() + TREE_TOP_LEVEL,
            "{}",
            row.kind()
        );
    }
    // And the two roots really are at the top rung.
    for row in variants() {
        if row.kind() == "section" || row.kind() == "group" {
            assert_eq!(aria_level_of(&row), TREE_TOP_LEVEL);
        }
    }
}

#[test]
fn omits_aria_expanded_on_the_rows_that_own_no_child_list() {
    for row in dump(&variants()) {
        if LEAVES.contains(&row.kind) {
            // Absent, not `false`: a `false` tells assistive tech a leaf is
            // collapsed and therefore activatable, which is a lie about a
            // table.
            assert!(
                row.expanded.is_none(),
                "{} reported aria-expanded",
                row.kind
            );
        }
    }
}

#[test]
fn answers_is_this_a_branch_from_the_row_never_from_its_position() {
    // A `namespace-node` is a branch only while it has children; the same type
    // answers differently depending on the row, which is the whole point of
    // asking the row rather than its index.
    let rows = variants();
    let node = rows
        .iter()
        .find(|r| r.kind() == "namespace-node")
        .expect("namespace node in the table");
    assert!(is_navigator_branch(node));
    let mut leaf = node.clone();
    if let NavigatorRow::NamespaceNode { is_leaf, .. } = &mut leaf {
        *is_leaf = true;
    }
    assert!(!is_navigator_branch(&leaf));
}

// ---------------------------------------------------------------------------
// The two ARIA functions must agree
// ---------------------------------------------------------------------------

/// `navigator_row_aria` and `is_navigator_branch` both answer "does this row
/// own a child list", and for a `namespace-node` they are required to give the
/// same answer. They used to disagree: the announcement wrote `aria-expanded`
/// for every `namespace-node` while the branch probe read `is_leaf`, so a
/// namespace leaf announced itself as a collapsed disclosure of a list it does
/// not have. This is the cross-check that gap was missing — phrased against
/// `is_navigator_branch` rather than a hand-listed set of leaf *types*,
/// because `namespace-node` is polymorphic and any type-keyed list has to lie
/// about one of its two shapes.
#[test]
fn never_announces_a_disclosure_on_a_row_the_shell_calls_a_leaf() {
    let rows = variants();
    let branch = rows
        .iter()
        .find(|r| r.kind() == "namespace-node")
        .expect("namespace node in the table")
        .clone();
    let mut leaf = branch.clone();
    if let NavigatorRow::NamespaceNode { is_leaf, name, .. } = &mut leaf {
        *is_leaf = true;
        *name = "mv1".to_owned();
    }
    let mut all = rows;
    all.push(leaf);
    // A guard against the list silently losing its one leaf of that type.
    assert_eq!(
        all.iter()
            .filter(|r| matches!(r, NavigatorRow::NamespaceNode { is_leaf: true, .. }))
            .count(),
        1
    );

    for row in &all {
        if is_decoration(row.kind()) {
            continue;
        }
        let announced = navigator_row_aria(row).and_then(|a| a.expanded);
        if !is_navigator_branch(row) {
            // The defect: this read `aria-expanded="false"`.
            assert!(
                announced.is_none(),
                "{} is a leaf but announced a disclosure",
                row_key_label(row)
            );
        }
    }
}

#[test]
fn covers_the_polymorphic_variant_in_both_of_its_states_not_just_the_branch_one() {
    let rows = variants();
    let branch = rows
        .iter()
        .find(|r| r.kind() == "namespace-node")
        .expect("namespace node in the table")
        .clone();
    // The leaf state is not in the 13-variant table, because the table is
    // keyed by type and this type has two shapes. It has to be spelled out.
    let mut leaf = branch.clone();
    if let NavigatorRow::NamespaceNode { is_leaf, .. } = &mut leaf {
        *is_leaf = true;
    }

    assert!(is_navigator_branch(&branch));
    assert_eq!(navigator_row_aria(&branch).unwrap().expanded, Some(false));

    assert!(!is_navigator_branch(&leaf));
    // Absent, not `false`. Both halves are asserted, because the bug was one
    // function being right while the other was wrong.
    assert_eq!(
        navigator_row_aria(&leaf),
        Some(VirtualTreeItemAria {
            role: "treeitem",
            level: aria_level_of(&leaf),
            expanded: None,
        })
    );
}

#[test]
fn keeps_the_connection_exception_explicit_rather_than_accidental() {
    // The one variant where the two answers are *meant* to differ: a
    // database's parent is its connection even while the connection is idle,
    // so the shell must keep treating the row as a branch — but there is
    // nothing to disclose yet, so no `aria-expanded`. If this ever starts
    // announcing `false`, the exception has turned into the bug above.
    let rows = variants();
    let base = rows
        .iter()
        .find(|r| r.kind() == "connection")
        .expect("connection in the table")
        .clone();
    let mut idle = base.clone();
    if let NavigatorRow::Connection { status, expanded, .. } = &mut idle {
        *status = "error".to_owned();
        *expanded = false;
    }
    assert!(is_navigator_branch(&idle));
    assert_eq!(navigator_row_aria(&idle).unwrap().expanded, None);
    assert_eq!(
        navigator_row_aria(&idle),
        Some(VirtualTreeItemAria {
            role: "treeitem",
            level: aria_level_of(&idle),
            expanded: None,
        })
    );
    // And the function the chevron reads says the same thing.
    assert_eq!(connection_expanded_state(&idle), None);
}

#[test]
fn a_connecting_connection_announces_its_disclosure_like_a_connected_one() {
    // `connectionExpandedState` covers both statuses that own a child list;
    // the exception test above pins only the silent half (idle/error). This
    // pins the other two, so neither half of the disjunction can regress
    // alone.
    let rows = variants();
    let base = rows
        .iter()
        .find(|r| r.kind() == "connection")
        .expect("connection in the table")
        .clone();
    for status in ["connected", "connecting"] {
        let mut conn = base.clone();
        if let NavigatorRow::Connection { status: s, expanded, .. } = &mut conn {
            *s = status.to_owned();
            *expanded = true;
        }
        assert_eq!(
            connection_expanded_state(&conn),
            Some(true),
            "{status} owns a child list and must announce it"
        );
        assert_eq!(navigator_row_aria(&conn).unwrap().expanded, Some(true));
    }
    for status in ["idle", "error"] {
        let mut conn = base.clone();
        if let NavigatorRow::Connection { status: s, expanded, .. } = &mut conn {
            *s = status.to_owned();
            *expanded = true;
        }
        assert_eq!(
            connection_expanded_state(&conn),
            None,
            "{status} has nothing to expand and must announce nothing"
        );
    }
}

// ---------------------------------------------------------------------------
// Search shifts the level by exactly one
// ---------------------------------------------------------------------------

/// The builder's rule, restated: under a search the group header that sat
/// between a connection and its databases is gone, so rows at or below
/// `CONNECTION_CHILD_DEPTH` announce one rung higher and paint where they
/// were.
fn searched(row: &NavigatorRow) -> NavigatorRow {
    if is_decoration(row.kind()) {
        return row.clone();
    }
    let mut next = row.clone();
    let hidden = if row.depth() >= DEPTH { 1 } else { 0 };
    let (depth, level_depth) = row.depth_pair();
    next.set_level_depth(level_depth.saturating_sub(hidden));
    let _ = depth;
    next
}

#[test]
fn drops_the_announced_level_by_one_and_never_by_two_or_by_none() {
    for row in variants() {
        if is_decoration(row.kind()) {
            continue;
        }
        let before = navigator_row_aria(&row).unwrap().level;
        let after = navigator_row_aria(&searched(&row)).unwrap().level;
        let expected = if row.depth() >= DEPTH { before - 1 } else { before };
        assert_eq!(after, expected, "{} (depth {})", row.kind(), row.depth());
    }
}

#[test]
fn paints_the_same_indent_in_both_states_while_announcing_a_different_level() {
    let rows: Vec<NavigatorRow> = variants()
        .into_iter()
        .filter(|r| r.depth() >= DEPTH && !is_decoration(r.kind()))
        .collect();
    assert!(!rows.is_empty());
    for row in rows {
        let before = &dump(std::slice::from_ref(&row))[0];
        let after = &dump(std::slice::from_ref(&searched(&row)))[0];
        assert_eq!(after.indent_rem, before.indent_rem, "{} indent moved", row.kind());
        assert_eq!(after.level, before.level.map(|l| l - 1), "{} level", row.kind());
    }
}

#[test]
fn leaves_the_roots_alone_a_search_hides_nothing_above_a_connection() {
    for row in variants().into_iter().filter(|r| r.depth() < DEPTH && !is_decoration(r.kind())) {
        assert_eq!(
            navigator_row_aria(&searched(&row)).unwrap().level,
            navigator_row_aria(&row).unwrap().level,
            "{}",
            row.kind()
        );
    }
}

// ---------------------------------------------------------------------------
// The same rule, measured on real builder output rather than on a table of
// hand-written rows. The fixture is a plain SQL connection, so it reaches the
// variants a SQL navigator actually shows; the key-value and path-hierarchy
// variants are gated on the generated driver registry and are covered by the
// literal table above plus the tree-row suite.
// ---------------------------------------------------------------------------

fn pg_conn(id: &str) -> ConnectionSnapshot {
    ConnectionSnapshot {
        id: id.to_owned(),
        name: id.to_owned(),
        database_type: "postgresql".to_owned(),
        host: Some("localhost".to_owned()),
        database: Some("db1".to_owned()),
        ..Default::default()
    }
}

fn aria_schema_state() -> SchemaStateSnapshot {
    SchemaStateSnapshot {
        current_database: None,
        databases: vec!["db1".to_owned()],
        database_type: Some("PostgreSQL".to_owned()),
        tables: vec![TableInfo {
            name: "users".to_owned(),
            schema: Some("public".to_owned()),
            table_type: TableType::Table,
        }],
        schema_names: vec!["public".to_owned()],
        ..Default::default()
    }
}

fn aria_params(
    query: &str,
    grouped: Vec<GroupedSection>,
    expanded_groups: HashSet<String>,
    expanded_connections: HashSet<String>,
) -> BuildFlatRowsParams {
    BuildFlatRowsParams {
        grouped,
        expanded_groups,
        expanded_connections,
        expanded_dbs: HashSet::from(["c1::db1".to_owned()]),
        expanded_schemas: HashSet::from(["c1::db1::public".to_owned()]),
        expanded_cats: HashSet::from([
            "c1::db1::public::tables".to_owned(),
            "c1::db1::public::routines".to_owned(),
        ]),
        active_connections: HashMap::from([(
            "c1".to_owned(),
            ActiveEntrySnapshot {
                db_session_id: Some("s1".to_owned()),
                status: "connected".to_owned(),
            },
        )]),
        active_connection_id: Some("c1".to_owned()),
        schemas: HashMap::from([("s1".to_owned(), aria_schema_state())]),
        query: query.to_owned(),
        ..Default::default()
    }
}

fn grouped_reserved() -> Vec<GroupedSection> {
    vec![
        GroupedSection {
            group: PINNED_GROUP_KEY.to_owned(),
            connections: vec![pg_conn("c1")],
        },
        GroupedSection {
            group: RECENT_GROUP_KEY.to_owned(),
            connections: vec![],
        },
    ]
}

fn grouped_custom() -> Vec<GroupedSection> {
    vec![
        GroupedSection {
            group: "team-b".to_owned(),
            connections: vec![pg_conn("c1")],
        },
        GroupedSection {
            group: "team-z".to_owned(),
            connections: vec![],
        },
    ]
}

fn custom_expand_sets() -> (HashSet<String>, HashSet<String>) {
    (
        HashSet::from(["team-b".to_owned(), "team-z".to_owned()]),
        HashSet::from(["team-b::c1".to_owned()]),
    )
}

fn sorted_kinds(rows: &[NavigatorRow]) -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = rows.iter().map(|r| r.kind()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

#[test]
fn the_plain_tree_produces_the_roots_and_the_hint_row_under_the_reserved_keys() {
    // Measured, not assumed: the plain set is measured under the two
    // *reserved* group keys, where pinned renders as a `section` — a fact
    // about this fixture, pinned by the next test's custom-group run.
    let plain = dump(&build_navigator_flat_rows(&aria_params(
        "",
        grouped_reserved(),
        HashSet::from([PINNED_GROUP_KEY.to_owned(), RECENT_GROUP_KEY.to_owned()]),
        HashSet::from([format!("{PINNED_GROUP_KEY}::c1")]),
    )));
    assert_eq!(
        plain.iter().map(|d| d.kind).collect::<HashSet<_>>(),
        [
            "category",
            "connection",
            "db",
            "empty-group",
            "schema",
            "section",
            "table"
        ]
        .into_iter()
        .collect::<HashSet<_>>()
    );
}

#[test]
fn a_search_drops_both_root_spellings_and_the_hint_row() {
    let searching = dump(&build_navigator_flat_rows(&aria_params(
        "users",
        grouped_reserved(),
        HashSet::from([PINNED_GROUP_KEY.to_owned(), RECENT_GROUP_KEY.to_owned()]),
        HashSet::from([format!("{PINNED_GROUP_KEY}::c1")]),
    )));
    // A search replaces the grouped sections with one unranked list, so it
    // emits no `section` and no `group` row at all, and an empty group is
    // filtered away rather than padded with its hint row.
    assert_eq!(
        searching.iter().map(|d| d.kind).collect::<HashSet<_>>(),
        ["category", "connection", "db", "schema", "table"]
            .into_iter()
            .collect::<HashSet<_>>()
    );

    // No groups at all is the only way to reach `no-connections`.
    let bare = dump(&build_navigator_flat_rows(&aria_params(
        "",
        vec![],
        HashSet::from([PINNED_GROUP_KEY.to_owned(), RECENT_GROUP_KEY.to_owned()]),
        HashSet::from([format!("{PINNED_GROUP_KEY}::c1")]),
    )));
    assert!(bare.iter().any(|d| d.kind == "no-connections"));
}

#[test]
fn a_user_created_group_name_adds_a_group_row_and_a_search_still_does_not() {
    // With a group the user named, the builder emits `group` where it emits
    // `section` for pinned — so the ordinary union is eight, not seven.
    // `group` and `section` are the two roots; exactly one of them is
    // present, decided by which kind of group the user has.
    let (groups, conns) = custom_expand_sets();
    let custom_grouped = dump(&build_navigator_flat_rows(&aria_params(
        "",
        grouped_custom(),
        groups.clone(),
        conns.clone(),
    )));
    assert_eq!(
        custom_grouped.iter().map(|d| d.kind).collect::<HashSet<_>>(),
        [
            "category",
            "connection",
            "db",
            "empty-group",
            "group",
            "schema",
            "table"
        ]
        .into_iter()
        .collect::<HashSet<_>>()
    );
    assert!(!custom_grouped.iter().any(|d| d.kind == "section"));

    let plain = dump(&build_navigator_flat_rows(&aria_params(
        "",
        grouped_reserved(),
        HashSet::from([PINNED_GROUP_KEY.to_owned(), RECENT_GROUP_KEY.to_owned()]),
        HashSet::from([format!("{PINNED_GROUP_KEY}::c1")]),
    )));
    assert!(!plain.iter().any(|d| d.kind == "group"));

    // And the searched set is genuinely root-free either way, so the level
    // shift does not depend on which root spelling was in play.
    let searched_custom = dump(&build_navigator_flat_rows(&aria_params(
        "users",
        grouped_custom(),
        groups,
        conns,
    )));
    assert_eq!(
        sorted_kinds(
            &build_navigator_flat_rows(&aria_params(
                "users",
                grouped_custom(),
                custom_expand_sets().0,
                custom_expand_sets().1,
            ))
        ),
        sorted_kinds(&build_navigator_flat_rows(&aria_params(
            "users",
            grouped_reserved(),
            HashSet::from([PINNED_GROUP_KEY.to_owned(), RECENT_GROUP_KEY.to_owned()]),
            HashSet::from([format!("{PINNED_GROUP_KEY}::c1")]),
        )))
    );
    assert!(!searched_custom.iter().any(|d| d.kind == "group"));
    assert!(!searched_custom.iter().any(|d| d.kind == "section"));
}

fn find<'a>(rows: &'a [Dump], label: &str) -> Option<&'a Dump> {
    rows.iter().find(|d| format!("{}:{}", d.kind, d.label) == label)
}

fn plain_and_searching() -> (Vec<Dump>, Vec<Dump>) {
    (
        dump(&build_navigator_flat_rows(&aria_params(
            "",
            grouped_reserved(),
            HashSet::from([PINNED_GROUP_KEY.to_owned(), RECENT_GROUP_KEY.to_owned()]),
            HashSet::from([format!("{PINNED_GROUP_KEY}::c1")]),
        ))),
        dump(&build_navigator_flat_rows(&aria_params(
            "users",
            grouped_reserved(),
            HashSet::from([PINNED_GROUP_KEY.to_owned(), RECENT_GROUP_KEY.to_owned()]),
            HashSet::from([format!("{PINNED_GROUP_KEY}::c1")]),
        ))),
    )
}

#[test]
fn drops_one_level_for_every_row_below_the_header_the_search_removed() {
    let (plain, searching) = plain_and_searching();
    for label in ["db:c1/db1", "schema:public", "category:c1::db1::public::tables"] {
        let before = find(&plain, label).unwrap_or_else(|| panic!("{label} in the plain tree"));
        let after =
            find(&searching, label).unwrap_or_else(|| panic!("{label} in the searched tree"));
        assert_eq!(after.level, before.level.map(|l| l - 1), "{label}");
    }
}

#[test]
fn paints_the_searched_rows_at_the_indent_they_had_before() {
    let (plain, searching) = plain_and_searching();
    for label in ["db:c1/db1", "schema:public", "category:c1::db1::public::tables"] {
        assert_eq!(
            find(&searching, label).unwrap().indent_rem,
            find(&plain, label).unwrap().indent_rem,
            "{label}"
        );
    }
}

#[test]
fn announces_a_level_on_every_row_it_emits_and_none_on_the_two_decorations() {
    let (plain, searching) = plain_and_searching();
    let bare = dump(&build_navigator_flat_rows(&aria_params(
        "",
        vec![],
        HashSet::from([PINNED_GROUP_KEY.to_owned(), RECENT_GROUP_KEY.to_owned()]),
        HashSet::from([format!("{PINNED_GROUP_KEY}::c1")]),
    )));
    for row in plain.iter().chain(&searching).chain(&bare) {
        if is_decoration(row.kind) {
            assert!(row.level.is_none(), "{}", row.kind);
        } else {
            assert!(row.level.is_some(), "{}", row.kind);
        }
    }
}
