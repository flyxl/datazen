//! Guard: no driver source may read a protected env file.
//!
//! The fixture security rules (`docs/architecture/platform/fake-runtime-fixtures.md`
//! §13, and `AGENTS.md` "本地环境变量文件保护") say that *no* program — not just an
//! agent — may open, read, parse or print a repo-local env file; credentials are
//! injected by CI secrets or by an authorised process environment. This test
//! turns that rule into an executable assertion for the whole `packages/drivers/`
//! tree.
//!
//! How it avoids being a tautology:
//!
//! * **Directory scan, no file list.** It walks `packages/drivers/*/{src,tests}`
//!   from disk. A driver crate added tomorrow is covered without touching this
//!   file, and a hard-coded allow-list could not be extended by accident.
//! * **Self-hosting.** The forbidden names are assembled with `concat!` from
//!   fragments, so this file stays legal even if it is ever moved under a
//!   scanned `tests/` directory. Nothing is excluded from the scan by name.
//! * **String literals, not prose.** A name only counts when it is immediately
//!   followed by its closing quote, so a doc comment may still *talk about* env
//!   files (as `postgres_cross_database.rs` does) while a `join` of the name
//!   inside a string literal fails.
//! * **It scans a synthetic tree too.** `the_scan_catches_a_brand_new_offender`
//!   builds a throwaway `packages/drivers/<never-seen>/tests/` tree, plants a
//!   violation, and asserts the very same `findings()` reports it. If the scan
//!   ever degraded into "check the files I know about", that test would fail.
//!
//! Known limit, stated rather than hidden: a path assembled at runtime out of
//! unrelated fragments (`concat!(".en", "v")`, a name built from `String::push`)
//! is not detected. That is deliberate — the point of the opt-in loader is that
//! a reviewer sees it — and the two rules above already reject the crate-level
//! escape hatch by banning the dotenv crates outright.

use std::path::{Path, PathBuf};

/// The protected basename, assembled from fragments so this file cannot match
/// its own rules.
const DOTENV: &str = concat!(".en", "v");
const ENV_FILE_BASENAMES: &[&str] = &[DOTENV, concat!(".en", "v.test"), concat!(".en", "v.local")];
/// Loader call tokens, assembled from fragments so this file cannot match them.
const DOTENV_LOADERS: &[&str] = &[
    concat!("load", "_dotenv"),
    concat!("dotenv", "::"),
    concat!("dotenv", "()"),
    concat!("dotenv", "y"),
];
/// Substrings that would betray a dotenv dependency in a driver's `Cargo.toml`.
const DOTENV_CRATE_TOKENS: &[&str] = &[concat!("dotenv", "y"), concat!("dot", "env")];
/// Source roots inside every driver crate.
const SCANNED_SUBDIRS: &[&str] = &[concat!("sr", "c"), concat!("te", "sts")];

#[derive(Debug)]
struct Finding {
    file: String,
    line_no: usize,
    detail: String,
}

impl Finding {
    fn render(&self) -> String {
        format!("{}:{}: {}", self.file, self.line_no, self.detail)
    }
}

/// Walk up from this crate to the checkout that owns `packages/drivers`.
///
/// Resolved from `CARGO_MANIFEST_DIR`, not the process working directory, so the
/// result is the same whichever target directory or package the test is run
/// from — and in a worktree it resolves to the worktree, not the main checkout.
fn repo_root() -> PathBuf {
    let mut dir: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("packages").join("drivers").is_dir() {
            return dir.to_path_buf();
        }
        dir = dir.parent().unwrap_or_else(|| {
            panic!("no ancestor of CARGO_MANIFEST_DIR contains packages/drivers")
        });
    }
}

fn is_protected_env_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    ENV_FILE_BASENAMES.iter().any(|env| {
        name == *env || name.starts_with(env) && name.as_bytes().get(env.len()) == Some(&b'.')
    })
}

/// True when `line` contains `token` immediately followed by its closing quote.
///
/// Matching against the *closing* quote is what separates a path from prose:
/// a name wrapped in backticks in a doc comment has a backtick after it and is
/// left alone, while the same name inside a string literal does not survive.
fn line_contains_quoted_token(line: &str, token: &str) -> bool {
    let mut from = 0usize;
    while let Some(rel) = line[from..].find(token) {
        let after = from + rel + token.len();
        if line[after..].starts_with('"') || line[after..].starts_with('\'') {
            return true;
        }
        from = from + rel + 1;
    }
    false
}

fn line_contains_token(line: &str, token: &str) -> bool {
    line.contains(token)
}

/// The protected file this line names as a string literal, if any.
fn named_env_file(line: &str) -> Option<&'static str> {
    ENV_FILE_BASENAMES
        .iter()
        .copied()
        .find(|name| line_contains_quoted_token(line, name))
}

fn collect_rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
            && !is_protected_env_file(&path)
        {
            out.push(path);
        }
    }
}

/// Every `.rs` file under `packages/drivers/*/{src,tests}` that exists right now.
fn scanned_files(root: &Path) -> Vec<PathBuf> {
    let drivers = root.join("packages").join("drivers");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&drivers) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        for sub in SCANNED_SUBDIRS {
            collect_rust_sources(&entry.path().join(sub), &mut out);
        }
    }
    out.sort();
    out
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// The one rule, applied uniformly to every scanned file.
fn findings(files: &[PathBuf], root: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for path in files {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = rel(root, path);
        for (idx, line) in content.lines().enumerate() {
            let line_no = idx + 1;
            if let Some(basename) = named_env_file(line) {
                out.push(Finding {
                    file: name.clone(),
                    line_no,
                    detail: format!(
                        "names the protected file {basename:?} as a string literal: \
                         test credentials come from the process environment only"
                    ),
                });
            }
            if let Some(loader) = DOTENV_LOADERS
                .iter()
                .copied()
                .find(|loader| line_contains_token(line, loader))
            {
                out.push(Finding {
                    file: name.clone(),
                    line_no,
                    detail: format!(
                        "uses the dotenv loader {loader:?}: loading an env file from disk is \
                         forbidden, read the process environment instead"
                    ),
                });
            }
        }
    }
    out
}

/// A `dotenv` dependency is the crate-level escape hatch around the rules above.
fn dependency_findings(root: &Path) -> Vec<Finding> {
    let drivers = root.join("packages").join("drivers");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&drivers) else {
        return out;
    };
    for entry in entries.flatten() {
        let manifest = entry.path().join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        for (idx, line) in content.lines().enumerate() {
            if let Some(token) = DOTENV_CRATE_TOKENS
                .iter()
                .copied()
                .find(|token| line_contains_token(line, token))
            {
                out.push(Finding {
                    file: rel(root, &manifest),
                    line_no: idx + 1,
                    detail: format!(
                        "driver crate depends on {token:?}: a driver must not be able to load an \
                         env file from disk at all"
                    ),
                });
            }
        }
    }
    out
}

fn report(findings: &[Finding], scanned: usize) -> String {
    let mut s = format!(
        "{} protected-env-file read path(s) under packages/drivers \
         ({} source file(s) scanned).\n\
         Fixture security rules (§13 of docs/architecture/platform/fake-runtime-fixtures.md) \
         forbid any program from opening, reading or printing a repo-local env file. \
         Take credentials from the process environment:\n",
        findings.len(),
        scanned
    );
    for finding in findings {
        s.push_str("  ");
        s.push_str(&finding.render());
        s.push('\n');
    }
    s.push_str(
        "Remove the file read rather than reordering it — a priority fallback still reads the \
         protected file. Gate the live test on its `TEST_*` process-env keys so it skips cleanly \
         when they are absent.",
    );
    s
}

#[test]
fn no_driver_source_reads_a_protected_env_file() {
    let root = repo_root();
    let files = scanned_files(&root);
    // A silently empty scan would make this test vacuously green.
    assert!(
        !files.is_empty(),
        "scan found no driver sources under {} — the guard is not looking at anything",
        root.join("packages").join("drivers").display()
    );
    let mut found = findings(&files, &root);
    found.extend(dependency_findings(&root));
    assert!(found.is_empty(), "{}", report(&found, files.len()));
}

#[test]
fn the_guard_is_clean_under_its_own_rule() {
    // This file normally lives outside the scanned tree, so nothing would stop a
    // future edit from teaching the guard to read env files by example. Hold it
    // to its own standard so that stays impossible.
    let root = repo_root();
    let me = root.join(file!());
    assert!(
        me.is_file(),
        "cannot locate the guard source at {}",
        me.display()
    );
    let found = findings(std::slice::from_ref(&me), &root);
    assert!(found.is_empty(), "{}", report(&found, 1));
}

#[test]
fn the_scan_covers_a_brand_new_offender() {
    // A synthetic checkout, so the scan is exercised on a crate layout it has
    // never seen and cannot have been told about.
    let scratch = std::env::temp_dir().join(format!(
        "dz-env-guard-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let clean = scratch
        .join("packages")
        .join("drivers")
        .join("brand_new_db")
        .join("src");
    let dirty = scratch
        .join("packages")
        .join("drivers")
        .join("brand_new_db")
        .join("tests");
    std::fs::create_dir_all(&clean).unwrap();
    std::fs::create_dir_all(&dirty).unwrap();
    std::fs::write(clean.join("lib.rs"), "pub fn connect() {}\n").unwrap();
    let planted = dirty.join("live_smoke.rs");
    std::fs::write(
        &planted,
        format!(
            "// two lines of header\nfn creds() -> std::collections::HashMap<String, String> {{\n    \
             let p = dir.join(\"{DOTENV}\");\n    let out = std::fs::read_to_string(&p).unwrap();\n    \
             out\n}}\n"
        ),
    )
    .unwrap();

    let files = scanned_files(&scratch);
    assert_eq!(files.len(), 2, "expected the planted src and tests file");

    let found = findings(&files, &scratch);
    assert_eq!(
        found.len(),
        1,
        "the planted join was not reported: {:?}",
        found.iter().map(Finding::render).collect::<Vec<_>>()
    );
    assert_eq!(
        found[0].line_no, 3,
        "the report must point at the offending line"
    );
    assert!(
        found[0].file.ends_with("brand_new_db/tests/live_smoke.rs"),
        "unexpected file: {}",
        found[0].file
    );

    // The crate-level escape hatch: a driver that merely *can* load an env file
    // is caught even when its own source is innocent.
    let crate_root = scratch
        .join("packages")
        .join("drivers")
        .join("brand_new_db");
    assert!(dependency_findings(&scratch).is_empty());
    std::fs::write(
        crate_root.join("Cargo.toml"),
        "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n",
    )
    .unwrap();
    assert!(dependency_findings(&scratch).is_empty());
    std::fs::write(
        crate_root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n{} = \"0.15\"\n",
            concat!("dot", "env")
        ),
    )
    .unwrap();
    let deps = dependency_findings(&scratch);
    assert_eq!(
        deps.len(),
        1,
        "a dotenv dependency was not reported: {:?}",
        deps.iter().map(Finding::render).collect::<Vec<_>>()
    );
    assert!(
        deps[0].file.ends_with("brand_new_db/Cargo.toml"),
        "unexpected manifest: {}",
        deps[0].file
    );

    std::fs::remove_dir_all(&scratch).unwrap();
}

#[test]
fn the_matcher_rejects_a_path_and_accepts_prose() {
    // Inputs are interpolated, never written out, so that this file contains no
    // offending literal of its own and stays green even under a scanned tree.
    let env = DOTENV;
    let env_test = concat!(".en", "v.test");
    // A path: the name is a string literal, so this must be caught.
    assert_eq!(
        named_env_file(&format!(r#"dir.join("{env}")"#)),
        Some(DOTENV)
    );
    assert_eq!(
        named_env_file(&format!(r#"let t = "{env_test}";"#)),
        Some(env_test)
    );
    assert_eq!(
        named_env_file(&format!(r#"p.join(r"{env}")"#)),
        Some(DOTENV)
    );
    assert_eq!(named_env_file(&format!("p.join('{env}')")), Some(DOTENV));
    // Prose in a doc comment: backticks, not a closing quote. Must stay legal —
    // documenting *why* a file is not read is the point of the rule.
    assert_eq!(
        named_env_file(&format!(
            "//! test must never open a protected `{env}` file"
        )),
        None
    );
    assert_eq!(
        named_env_file(&format!(
            "/// reading a repo-local `{env}` would load a protected file"
        )),
        None
    );
    // Same prefix, different word: nothing to do with an env file.
    assert_eq!(named_env_file(r#"dir.join("environment")"#), None);
    assert_eq!(named_env_file(r#"dir.join("env")"#), None);
    assert_eq!(named_env_file(r#"dir.join(".environment")"#), None);
}

#[test]
fn the_protected_file_check_never_names_a_file_it_must_not_read() {
    assert!(is_protected_env_file(Path::new(&format!("/repo/{DOTENV}"))));
    assert!(is_protected_env_file(Path::new(&format!(
        "/repo/{}",
        concat!(".en", "v.test")
    ))));
    assert!(!is_protected_env_file(Path::new("/repo/live_smoke.rs")));
    assert!(!is_protected_env_file(Path::new("/repo/environment.rs")));
}
