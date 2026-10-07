//! Line-count guard for the files the driver split is responsible for.
//!
//! A source file that grows past the point where one screenful of reading can
//! hold it stops being reviewed, so this crate's own size is pinned rather than
//! left to drift back. The check also covers `driver-api`, because that trait is
//! the other half of the same split: a change that pushes the shared contract
//! past its ceiling is as much a regression as one that inflates this crate.
//!
//! It lives under `src/sqlserver/` rather than in `tests/` because it is a unit
//! test of the module layout, and because a file only runs as a test when some
//! module in the crate root declares it — `sqlserver.rs` already owns that
//! declaration block.

use std::path::{Path, PathBuf};

/// The ceiling a source file is expected to stay under.
const LINE_LIMIT: usize = 800;

/// `driver-api/src/traits.rs` is above the ceiling and is held to this one
/// instead. What is left in it is the `DatabaseDriver` contract itself: the
/// trait declaration, the documentation driver authors read, and the one-line
/// delegations into the sibling modules. The contract cannot be split across
/// files without making every signature unresolvable, so the file is pinned at
/// its measured size and may shrink but not grow. Lowering it requires moving
/// documentation or whole methods out of the declaration, which is a contract
/// change rather than a layout change.
const DATABASE_DRIVER_TRAITS_CEILING: usize = 1020;

/// Path shown in a failure message: `driver-api` is reached by climbing out of
/// this crate, and a name full of `..` segments is harder to act on than the
/// `packages/...` form the reader already knows.
fn display(path: &Path) -> String {
    let components: Vec<_> = path.components().collect();
    let start = components
        .iter()
        .position(|component| component.as_os_str() == "packages")
        .unwrap_or(0);
    components[start..]
        .iter()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn line_count(path: &Path) -> usize {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()));
    contents.lines().count()
}

fn assert_not_above(path: &Path, limit: usize) {
    let count = line_count(path);
    assert!(
        count <= limit,
        "{} has {count} lines, over the {limit}-line limit; split it by responsibility",
        display(path)
    );
}

fn sqlserver_crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn driver_api_src() -> PathBuf {
    sqlserver_crate_root()
        .join("..")
        .join("..")
        .join("driver-api")
        .join("src")
}

/// Every `.rs` file directly inside `dir`, sorted, so a failure names the file
/// that grew rather than whichever the filesystem happened to list first.
fn rust_files_in(dir: &Path) -> Vec<PathBuf> {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|err| panic!("cannot list {}: {err}", dir.display()));
    let mut files: Vec<PathBuf> = entries
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    files.sort();
    files
}

#[test]
fn split_files_stay_within_the_line_limit() {
    let sqlserver_src = sqlserver_crate_root().join("src");
    let mut guarded: Vec<PathBuf> = vec![sqlserver_src.join("sqlserver.rs")];
    guarded.extend(rust_files_in(&sqlserver_src.join("sqlserver")));

    let driver_api = driver_api_src();
    guarded.extend(rust_files_in(&driver_api.join("traits")));

    assert!(
        !guarded.is_empty(),
        "the line guard found no files to check, so it is not guarding anything"
    );
    for path in &guarded {
        assert_not_above(path, LINE_LIMIT);
    }
}

#[test]
fn database_driver_trait_declaration_does_not_grow() {
    assert_not_above(
        &driver_api_src().join("traits.rs"),
        DATABASE_DRIVER_TRAITS_CEILING,
    );
}
