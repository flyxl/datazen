//! T046 + T095: X-class determination for the two generator products
//! `packages/frontend/src/extensions/generated.ts` (T095) and
//! `generated-pro.ts` (T046).
//!
//! Neither file is authored source. `generated.ts` is the whole output of
//! `scripts/resolve-drivers.mjs` (a driver-selection codegen pass driven by
//! `drivers-registry.json`), and `generated-pro.ts` is the whole output of
//! `scripts/resolve-pro.mjs`. Both paths are declared gitignored in
//! `.gitignore`, and AGENTS.md says "生成文件不得提交". React only *consumes*
//! them — `lib/databaseTypes.ts` is literally `DB_REGISTRY = { ...DRIVER_DB_ENTRIES }`
//! — so there is no React behaviour to translate into Rune. The equivalent
//! surface is provided by the tracked generator plus the driver metas it reads.
//!
//! This module is the receipt for that ruling. It pins three observable
//! channels:
//!   1. the tracked generators still emit every export the React files expose,
//!      and the basic driver set maps to a non-empty, consistent
//!      driver → dbType → meta table;
//!   2. the Rune consumer slice (`db_registry_meta`) agrees with the source
//!      driver metas — no invented metadata, no drift;
//!   3. the generated paths are gitignored, and the on-disk artifacts (when a
//!      build has produced them) are self-consistent with that contract.
//!
//! The artifacts are gitignored and may be absent in a fresh checkout, so the
//! artifact-level probes run only when a file is present; the generator and
//! gitignore probes always run and are the durable part of the ruling.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::views::db_registry_meta;

/// Workspace root, independent of the test binary's CWD (Cargo sets it to the
/// package dir, but the queue/worktree layout must not matter).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read_ws(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read {} ({err})", path.display()))
}

/// A generated artifact is allowed to be missing (gitignored, not committed).
/// Any other I/O error is a real failure, never a silent pass.
fn maybe_read_ws(rel: &str) -> Option<String> {
    let path = workspace_root().join(rel);
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => panic!("cannot read {} ({err})", path.display()),
    }
}

fn assert_contains(hay: &str, needle: &str) {
    assert!(hay.contains(needle), "missing source fact: {needle:?}");
}

/// Text between the first `start` marker and the first `end` after it.
fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let i = match text.find(start) {
        Some(i) => i,
        None => panic!("missing start marker {start:?}"),
    };
    let tail = &text[i + start.len()..];
    let j = match tail.find(end) {
        Some(j) => j,
        None => panic!("missing end marker {end:?} after {start:?}"),
    };
    &tail[..j]
}

/// Body of a `{ ... }` object whose opening header is `header` (must end in
/// `{`). Brace counting keeps nested `dbTypes: [{ ... }]` blocks intact.
fn object_block<'a>(text: &'a str, header: &str) -> &'a str {
    assert!(header.ends_with('{'), "object header must end in '{{': {header:?}");
    let i = match text.find(header) {
        Some(i) => i,
        None => panic!("missing object block {header:?}"),
    };
    let open = i + header.len() - 1;
    let mut depth = 0_i32;
    for (k, byte) in text.as_bytes().iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open + 1..k];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced object block {header:?}");
}

/// Single-quoted tokens in order (the generators use `'` for their strings).
fn quoted(text: &str) -> Vec<String> {
    text.split('\'')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn unique_count(items: &[String]) -> usize {
    items.iter().collect::<HashSet<_>>().len()
}

/// The exact export surface `generated.ts` exposes to React. Every one of
/// these is emitted by the tracked generator template, which is why a Rune
/// port of the artifact would be a duplicate, not a migration.
const FRONTEND_EXPORTS: &[&str] = &[
    "export const DRIVER_PROTOCOL_VERSION",
    "export const DATAZEN_VARIANT",
    "export const DATAZEN_UPDATER_CHANNEL",
    "export type DatabaseType",
    "export const DRIVER_DB_ENTRIES",
    "export const DRIVER_ICON_ENTRIES",
    "export const DRIVER_ICON_PARENTS",
    "export const DRIVER_SQL_DIALECTS",
    "export function getDriverConnectionForm",
    "export function getDriverConnectionAdvanced",
    "export function getDriverValidator",
    "export function getDriverClipboardParsers",
    "export function getDriverSchemaTree",
    "export function getDriverConnectionView",
    "export function getDriverKvSlot",
    "export const DRIVER_SETTINGS_ENTRIES",
    "export const DRIVER_COMMANDS",
    "export function hasDriverCommand",
];

#[test]
fn generated_products_are_gitignored_and_never_force_included() {
    let ignore = read_ws(".gitignore");
    for rel in [
        "packages/frontend/src/extensions/generated.ts",
        "packages/frontend/src/extensions/generated-pro.ts",
        "crates/datazen-desktop/src/driver_init.rs",
        "crates/datazen-desktop/capabilities/default.json",
    ] {
        assert!(
            ignore.lines().any(|line| line == rel),
            "no gitignore rule protects {rel}"
        );
        let negated = format!("!{rel}");
        assert!(
            !ignore.lines().any(|line| line == negated),
            "{rel} is force-included despite being generated"
        );
    }
}

#[test]
fn resolve_drivers_emits_the_whole_react_registry_surface() {
    let gen = read_ws("scripts/resolve-drivers.mjs");
    assert_contains(&gen, "export function renderFrontendRegistry(drivers, variant)");
    assert_contains(
        &gen,
        "workPath('packages/frontend/src/extensions/generated.ts')",
    );
    assert_contains(
        &gen,
        "AUTO-GENERATED by resolve-drivers.mjs — DO NOT EDIT MANUALLY",
    );
    // The registry, not hand-written source, decides which entries exist.
    assert_contains(
        &gen,
        "readFileSync(resolve(ROOT, 'drivers-registry.json'), 'utf-8')",
    );
    assert_contains(&gen, "const FRONTEND_DRIVER_CONFIG = {");
    for token in FRONTEND_EXPORTS {
        assert_contains(&gen, token);
    }
}

#[test]
fn basic_driver_set_maps_to_a_nonempty_consistent_db_meta_table() {
    let gen = read_ws("scripts/resolve-drivers.mjs");

    let basic = quoted(between(&gen, "const BASIC_DRIVERS = [", "];"));
    assert_eq!(
        basic,
        vec!["postgres", "mysql", "sqlite", "redis"],
        "the basic preset is the four core path drivers"
    );

    let frontend = object_block(&gen, "const BASIC_PATH_FRONTEND = {");
    let expected: &[(&str, &[(&str, &str)])] = &[
        (
            "postgres",
            &[
                ("postgresql", "postgresqlMeta"),
                ("questdb", "questdbMeta"),
                ("cloudberry", "cloudberryMeta"),
            ],
        ),
        (
            "mysql",
            &[
                ("mysql", "mysqlMeta"),
                ("mariadb", "mariadbMeta"),
                ("doris", "dorisMeta"),
                ("starrocks", "starrocksMeta"),
                ("manticore", "manticoreMeta"),
                ("ob_oracle", "obOracleMeta"),
            ],
        ),
        ("sqlite", &[("sqlite", "sqliteMeta")]),
        ("redis", &[("redis", "redisMeta")]),
    ];

    let mut db_ids: Vec<String> = Vec::new();
    let mut meta_bindings: Vec<String> = Vec::new();
    for (driver, want) in expected {
        let header = format!("\n  {driver}: {{");
        let block = object_block(frontend, &header);
        assert_contains(block, "dbTypes: [");
        assert_contains(block, "metaPath: '");

        let tokens = quoted(between(block, "dbTypes: [", "]"));
        assert_eq!(
            tokens.len(),
            want.len() * 2,
            "each dbType needs both an id and a metaExport ({driver})"
        );
        let got: Vec<(String, String)> = tokens
            .chunks_exact(2)
            .map(|pair| (pair[0].clone(), pair[1].clone()))
            .collect();
        let want_owned: Vec<(String, String)> = want
            .iter()
            .map(|(id, meta)| ((*id).to_string(), (*meta).to_string()))
            .collect();
        assert_eq!(got, want_owned, "driver → dbType → meta binding ({driver})");

        for (id, meta) in got {
            db_ids.push(id);
            meta_bindings.push(meta);
        }
    }

    assert_eq!(db_ids.len(), 11, "basic drivers contribute 11 db types");
    assert_eq!(
        unique_count(&db_ids),
        db_ids.len(),
        "db type ids must be unique"
    );
    assert_eq!(
        unique_count(&meta_bindings),
        meta_bindings.len(),
        "meta bindings must be unique"
    );
    for core in ["postgresql", "mysql", "sqlite", "redis"] {
        assert!(
            db_ids.iter().any(|id| id == core),
            "basic DB_REGISTRY must contain {core}"
        );
    }

    // Protocol-reuse db types have no own badge SVG: the generator records
    // their parent dbType so the host can compose a badge instead.
    let parents = between(&gen, "const DRIVER_ICON_PARENT = {", "\n};");
    for (child, parent) in [
        ("questdb", "postgresql"),
        ("cloudberry", "postgresql"),
        ("doris", "mysql"),
        ("starrocks", "mysql"),
        ("manticore", "mysql"),
        ("ob_oracle", "mysql"),
    ] {
        assert!(
            parents.contains(&format!("{child}: '{parent}'")),
            "missing icon parent {child} → {parent}"
        );
    }
}

#[test]
fn generated_ts_artifact_when_built_is_self_consistent() {
    let Some(artifact) = maybe_read_ws("packages/frontend/src/extensions/generated.ts") else {
        // Gitignored artifact: a fresh checkout has none. The generator probes
        // above are the durable receipt; this probe tightens a built tree.
        return;
    };
    assert_contains(
        &artifact,
        "AUTO-GENERATED by resolve-drivers.mjs — DO NOT EDIT MANUALLY",
    );

    let ids = quoted(between(&artifact, "export type DatabaseType = ", ";\n"));
    assert!(!ids.is_empty(), "DatabaseType union is empty");
    assert_eq!(
        unique_count(&ids),
        ids.len(),
        "DatabaseType union has duplicate members"
    );
    for core in ["postgresql", "mysql", "sqlite", "redis"] {
        assert!(
            ids.iter().any(|id| id == core),
            "DatabaseType omits core type {core}"
        );
    }

    let entries = between(
        &artifact,
        "export const DRIVER_DB_ENTRIES: Record<string, DatabaseTypeMeta> = {",
        "\n};",
    );
    let mut entry_ids: Vec<String> = Vec::new();
    for line in entries.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, binding) = match line.split_once(':') {
            Some(pair) => pair,
            None => panic!("malformed DRIVER_DB_ENTRIES row: {line}"),
        };
        let key = key.trim();
        let binding = binding.trim().trim_end_matches(',');
        assert!(!key.is_empty() && !binding.is_empty(), "empty db entry: {line}");
        assert!(
            artifact
                .lines()
                .any(|l| l.starts_with("import ") && l.contains(binding)),
            "db entry {key} uses unimported binding {binding}"
        );
        entry_ids.push(key.to_string());
    }
    assert_eq!(
        entry_ids, ids,
        "every DatabaseType member has exactly one meta entry, in the same order"
    );

    // Badge resolution is exactly-one-of: the generator emits an own-SVG entry
    // (`db.<id>`) when the driver ships a badge, otherwise a parent row
    // (`<id>: '<parent>'`) for a protocol-reuse type with no SVG of its own.
    // This build ships a badge for every db type, so DRIVER_ICON_PARENTS is
    // legitimately empty — asserting a hard-coded parent row would be inventing
    // metadata. Pin the invariant instead; it holds for either build.
    let icons = between(
        &artifact,
        "export const DRIVER_ICON_ENTRIES: Record<string, string> = {",
        "\n};",
    );
    let parents = between(
        &artifact,
        "export const DRIVER_ICON_PARENTS: Record<string, string> = {",
        "\n};",
    );
    let icon_keys = quoted(icons);
    assert!(
        icon_keys.iter().all(|key| key.starts_with("db.")),
        "icon entries must be keyed by db.<type>"
    );
    let own: HashSet<&str> = icon_keys.iter().map(|key| &key[3..]).collect();

    let mut inherited: Vec<(&str, &str)> = Vec::new();
    for line in parents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (child, parent) = match line.split_once(':') {
            Some(pair) => pair,
            None => panic!("malformed DRIVER_ICON_PARENTS row: {line}"),
        };
        let parent = parent.trim().trim_end_matches(',').trim_matches('\'');
        inherited.push((child.trim(), parent));
    }
    let inherited_children: HashSet<&str> = inherited.iter().map(|(child, _)| *child).collect();

    for id in &ids {
        let has_own = own.contains(id.as_str());
        let has_parent = inherited_children.contains(id.as_str());
        assert_ne!(
            has_own, has_parent,
            "{id} must have exactly one badge source (own SVG or parent)"
        );
    }
    for (_, parent) in &inherited {
        assert!(
            ids.iter().any(|id| id == parent),
            "inherited badge parent {parent} is not a DatabaseType in this build"
        );
    }
    for id in &own {
        let binding = format!("driverIcon_{id}");
        assert!(
            artifact
                .lines()
                .any(|line| line.starts_with("import ") && line.contains(&binding)),
            "badge binding {binding} is not imported"
        );
    }
    // Core drivers really do ship their badge SVGs, so their entry is the own
    // one — check disk rather than asserting a parent that this build omits.
    for (core, driver) in [
        ("postgresql", "postgres"),
        ("mysql", "mysql"),
        ("sqlite", "sqlite"),
        ("redis", "redis"),
    ] {
        let svg = workspace_root().join(format!("crates/drivers/{driver}/ui/icons/{core}.svg"));
        if svg.exists() {
            assert!(
                own.contains(core),
                "{core} ships a badge SVG but has no DRIVER_ICON_ENTRIES row"
            );
        }
    }
}

#[test]
fn generated_pro_is_a_no_op_community_shim_from_resolve_pro() {
    let gen = read_ws("scripts/resolve-pro.mjs");
    assert_contains(&gen, "export function writeCommunityCodegen(dest = GENERATED_PRO_TS)");
    assert_contains(
        &gen,
        "export const GENERATED_PRO_TS = resolve(ROOT, 'packages/frontend/src/extensions/generated-pro.ts');",
    );
    assert_contains(&gen, "export const DATAZEN_EDITION = 'community' as const;");
    assert_contains(&gen, "export function initProExtensions(): void {");
    assert_contains(&gen, "// Community edition: no pro extensions registered.");

    let Some(artifact) = maybe_read_ws("packages/frontend/src/extensions/generated-pro.ts") else {
        return;
    };
    assert_contains(
        &artifact,
        "AUTO-GENERATED by resolve-pro.mjs — DO NOT EDIT MANUALLY.",
    );
    assert_contains(&artifact, "export const DATAZEN_EDITION = 'community' as const;");
    assert_contains(&artifact, "export function initProExtensions(): void {");
    assert_contains(&artifact, "// Community edition: no pro extensions registered.");
    assert!(
        !artifact.contains("import "),
        "the community shim must not import any extension"
    );
    assert!(
        !artifact.contains("hostExtensionLoader"),
        "the community shim must register no pro extension"
    );
}

#[test]
fn rune_multi_db_slice_agrees_with_the_driver_meta_sources() {
    // The Rune side consumes only this slice of the generated registry (the
    // `WorkflowForm` multi-db rule). It must agree with the driver metas the
    // generator imports — no invented metadata on either side.
    for db_type in [
        "postgresql",
        "questdb",
        "cloudberry",
        "mysql",
        "mariadb",
        "doris",
        "starrocks",
        "manticore",
        "ob_oracle",
        "mongodb",
        "sqlserver",
        "clickhouse",
    ] {
        assert_eq!(
            db_registry_meta(db_type),
            Some((true, false)),
            "driver meta for {db_type} declares hasMultiDatabase, not a domain field"
        );
    }
    assert_eq!(db_registry_meta("kiwi"), Some((true, true)));
    for db_type in [
        "sqlite",
        "redis",
        "duckdb",
        "elasticsearch",
        "rqlite",
        "turso",
        "influxdb",
        "victoriametrics",
        "hbase",
        "vector",
    ] {
        assert_eq!(
            db_registry_meta(db_type),
            None,
            "{db_type} is not a multi-db driver"
        );
    }

    // Source-of-truth cross-check: the metas really declare (or omit) the flag
    // Rune hard-codes in its consumed slice.
    for meta_rel in [
        "crates/drivers/postgres/ui/meta.ts",
        "crates/drivers/mysql/ui/meta.ts",
        "crates/drivers/mongodb/ui/meta.ts",
        "crates/drivers/sqlserver/ui/meta.ts",
        "crates/drivers/clickhouse/ui/meta.ts",
    ] {
        let meta = read_ws(meta_rel);
        assert_contains(&meta, "hasMultiDatabase: true");
    }
    for meta_rel in [
        "crates/drivers/sqlite/ui/meta.ts",
        "crates/drivers/redis/ui/shared/meta.ts",
    ] {
        let meta = read_ws(meta_rel);
        assert!(
            !meta.contains("hasMultiDatabase: true"),
            "{meta_rel} unexpectedly declares hasMultiDatabase"
        );
    }
}
