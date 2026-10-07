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

use std::path::{Component, Path, PathBuf};

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
///
/// The segments are folded lexically so the `..` used to reach the file never
/// reaches the reader. This rewrites a name that has already been built — it
/// reads nothing from disk, and it cannot change what the guard measured.
fn display(path: &Path) -> String {
    let components: Vec<_> = path.components().collect();
    let start = components
        .iter()
        .position(|component| component.as_os_str() == "packages")
        .unwrap_or(0);
    let mut folded: Vec<String> = Vec::new();
    for component in &components[start..] {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let poppable = folded
                    .last()
                    .is_some_and(|last| !last.is_empty() && !last.starts_with('/') && last != "..");
                if poppable {
                    folded.pop();
                } else {
                    folded.push("..".to_string());
                }
            }
            other => folded.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    folded.join("/")
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

/// Every `.rs` file under `dir`, at any depth, sorted, so a failure names the
/// file that grew rather than whichever the filesystem happened to list first.
///
/// The walk recurses because a guarded file can legitimately sit in a submodule
/// directory — `driver-api`'s structural tests do — and a file that the guard
/// cannot see is a file it can no longer report.
fn rust_files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|err| panic!("cannot list {}: {err}", dir.display()));
        for entry in entries {
            let path = entry.expect("readable directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
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

#[test]
fn displayed_paths_are_canonical() {
    // The real guarded name is built by climbing out of this crate, so it is the
    // one that has to survive the fold; the synthetic ones pin the fold itself.
    assert_eq!(
        display(Path::new("/w/packages/drivers/sql/../../api/src/t.rs")),
        "packages/api/src/t.rs"
    );
    assert_eq!(display(Path::new("a/b/../c/./d.rs")), "a/c/d.rs");

    let shown = display(&driver_api_src().join("traits.rs"));
    assert!(
        !shown.split('/').any(|segment| segment == ".."),
        "the guarded path is still reported with parent segments: {shown}"
    );
}
