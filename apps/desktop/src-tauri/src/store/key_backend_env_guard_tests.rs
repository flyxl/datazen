//! Guard: **only `#[ignore]`d tests may touch the keyring process env**.
//!
//! `DATAZEN_KEYRING` / `DATAZEN_TEST_KEYRING` are process-global. A test that
//! writes them races with every other test in the binary: the reader can see
//! the writer's value, and `DATAZEN_KEYRING=keyring` + `DATAZEN_TEST_KEYRING=1`
//! sends the master-key lookup into **Keychain Services** — on macOS a modal
//! authorization dialog that hangs the whole `cargo test` process.
//!
//! The backend is now resolved **once at startup** and threaded down
//! explicitly (`store::key_backend::resolve_key_backend` → `KeyBackend`), so
//! the *only* place a process env write can still be legitimate is a test
//! that deliberately exercises the OS keychain. Those are `#[ignore]`d and
//! run on their own (`cargo test -- --ignored`).
//!
//! This guard makes that rule enforceable instead of conventional:
//!
//! * a direct `set_var` / `remove_var` of a keyring env name must sit inside
//!   an `#[ignore]`d test function (or in the shared `#[cfg(test)]` helper that
//!   only such tests use);
//! * a call to the `ForceKeyringGuard` helper — an *indirect* env write — must
//!   likewise sit inside an `#[ignore]`d test function.
//!
//! Both checks are source scans: a runtime assertion cannot see a writer that
//! has not run yet, and the whole point is to catch the writer that a future
//! refactor adds.

use std::path::{Path, PathBuf};

/// The env names that decide (or unlock) the master-key backend.
const ENV_NAMES: &[&str] = &["DATAZEN_KEYRING", "DATAZEN_TEST_KEYRING"];

/// Files allowed to contain a keyring env write outside an `#[ignore]`d test.
///
/// `src-tauri/src/testing/mod.rs` holds the shared `#[cfg(test)]` helper
/// (`ForceKeyringGuard`); it is not a test itself, and the guard test below
/// pins that only `#[ignore]`d tests call it.
const ALLOWED_HELPERS: &[&str] = &["apps/desktop/src-tauri/src/testing/mod.rs"];

/// This file: it names the patterns it looks for, so scanning itself would
/// report its own source (same convention as `job_filter_scope_tests.rs`).
const SELF: &str = "apps/desktop/src-tauri/src/store/key_backend_env_guard_tests.rs";

/// Directory names without executable contracts.
const SKIPPED_DIRECTORIES: &[&str] = &["target", "node_modules", ".git"];

/// Repository root: three levels above `apps/desktop/src-tauri`.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("src-tauri 位于 apps/desktop/ 下，上溯三级即仓库根")
        .to_path_buf()
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.join("apps/desktop/src-tauri/src")];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if SKIPPED_DIRECTORIES.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Whether the line writes (or removes) one of the keyring env names.
fn writes_keyring_env(line: &str) -> bool {
    let mutates = line.contains("set_var(") || line.contains("remove_var(");
    mutates && ENV_NAMES.iter().any(|name| line.contains(name))
}

/// Whether the line calls the shared guard — an indirect env write.
fn calls_force_keyring_guard(line: &str) -> bool {
    line.contains("ForceKeyringGuard::new()")
}

/// Index of the line that starts the function the given line belongs to.
fn enclosing_fn(lines: &[&str], hit: usize) -> Option<usize> {
    let mut index = hit;
    while index > 0 {
        index -= 1;
        let trimmed = lines[index].trim_start();
        let starts_fn = trimmed.starts_with("fn ")
            || trimmed.starts_with("async fn ")
            || trimmed.starts_with("pub fn ")
            || trimmed.starts_with("pub(crate) fn ")
            || trimmed.starts_with("pub(super) fn ")
            || trimmed.contains(" fn ");
        if starts_fn {
            return Some(index);
        }
    }
    None
}

/// Whether the function declared at `fn_line` carries an `#[ignore]` attribute.
fn is_ignored(lines: &[&str], fn_line: usize) -> bool {
    let mut index = fn_line;
    while index > 0 {
        index -= 1;
        let trimmed = lines[index].trim();
        if trimmed.is_empty() || !trimmed.starts_with("#[") {
            return false;
        }
        if trimmed.starts_with("#[ignore") {
            return true;
        }
    }
    false
}

/// Every keyring-env write / guard call outside an `#[ignore]`d test.
fn violations(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for path in rust_sources(root) {
        let relative_path = relative(root, &path);
        if relative_path == SELF {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = content.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let indirect = calls_force_keyring_guard(line);
            if !indirect && !writes_keyring_env(line) {
                continue;
            }
            if ALLOWED_HELPERS.contains(&relative_path.as_str()) {
                continue;
            }
            let allowed = enclosing_fn(&lines, index)
                .map(|fn_line| is_ignored(&lines, fn_line))
                .unwrap_or(false);
            if allowed {
                continue;
            }
            out.push(format!("{relative_path}:{}: {}", index + 1, line.trim()));
        }
    }
    out
}

#[test]
fn only_ignored_tests_may_touch_the_keyring_env() {
    let root = repository_root();
    let violations = violations(&root);
    assert!(
        violations.is_empty(),
        "the master-key backend is resolved once at startup, so a non-`#[ignore]`d test \
         must not write the keyring process env (a process-global write races with every \
         other test and can send the master-key lookup into Keychain Services):\n{}",
        violations.join("\n")
    );
}
