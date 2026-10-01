//! Guards the local-env-file protection for the SQL Server live suite.
//!
//! A live suite that reads a gitignored credential file by default is a suite
//! that silently depends on a developer's untracked file: it opens a protected
//! file no one asked it to open, and it makes the local run the only place the
//! live tier can pass. The rule enforced here is narrow and mechanical:
//!
//! 1. no source in this crate names an env file as a **string literal**, and no
//!    dotenv-style loader is called — the same shapes the shared contract
//!    template refuses, checked for this crate;
//! 2. the file is opened **only** through an explicit opt-in that a developer
//!    sets, and the decision is a pure function so it can be proven without
//!    mutating the process environment;
//! 3. the default path opens nothing at all, and an opted-in-but-unreadable file
//!    is an error the caller reports — never a silently empty map that would let
//!    the run pretend the file said nothing.
//!
//! `AGENTS.md` forbids opening these files' contents, so this suite never reads
//! one either: the probes below build and read a throwaway file under the
//! system temp directory with a name that is not an env file.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use common::{decode_opt_in, load_settings, EnvFileOptIn, EnvFileProblem, ENV_FILE_OPT_IN};

/// The env-file names this crate must never name literally. Assembled from
/// fragments so this guard cannot match its own source.
fn env_file_names() -> [&'static str; 2] {
    [concat!(".en", "v"), concat!(".en", "v.test")]
}

/// Every spelling that names an env file inside a string literal, plus the
/// dotenv-style loaders that read one implicitly.
///
/// Each name is matched **against its closing quote**, not as a bare substring,
/// so backticked prose about such a file stays legal and the guard cannot be
/// satisfied by deleting words from the docs — while a directory-qualified read
/// such as `../<name>` is still caught.
fn env_guard_tokens() -> Vec<String> {
    let mut tokens: Vec<String> = vec![
        concat!("load_", "dotenv").to_string(),
        concat!("dot", "env()").to_string(),
        concat!("dot", "env::").to_string(),
        concat!("dot", "envy").to_string(),
    ];
    for name in env_file_names() {
        for quote in ['"', '\''] {
            tokens.push(format!("{name}{quote}"));
        }
    }
    tokens
}

fn env_guard_violation(content: &str) -> Option<String> {
    env_guard_tokens()
        .into_iter()
        .find(|token| content.contains(token.as_str()))
}

/// The first forbidden token in this source, if any.
fn first_violation() -> Option<(PathBuf, String)> {
    let mut found = None;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for dir in [root.join("tests"), root.join("src")] {
        let mut sources = Vec::new();
        collect_rs(&dir, &mut sources);
        assert!(!sources.is_empty(), "no Rust source found under {}", dir.display());
        for path in sources {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            if let Some(token) = env_guard_violation(&content) {
                found = Some((path, token));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    found
}

/// Collects every `.rs` file under `dir`. An unreadable directory is a loud
/// failure rather than an empty result: "found no file" and "could not look"
/// must never look alike to a guard that is deciding whether anything was
/// scanned.
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("cannot read an entry of {}: {e}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_source_in_this_crate_names_an_env_file_or_loads_one_implicitly() {
    assert!(
        file!().ends_with("sqlserver/tests/live_env_file_opt_in.rs"),
        "this file is not the sqlserver env-file guard (file!() = {})",
        file!()
    );
    if let Some((path, token)) = first_violation() {
        panic!(
            "{} must not contain {token:?} — a local env file is opened only through \
             {ENV_FILE_OPT_IN}",
            path.display()
        );
    }
}

/// The matcher itself is proven against the shapes it must catch and the shapes
/// that must stay legal, so a token list cannot quietly stop matching.
#[test]
fn the_env_guard_matches_every_shape_of_env_file_literal() {
    let name = env_file_names()[0];
    let test_variant = env_file_names()[1];

    // (source, must_be_flagged)
    let cases: Vec<(String, bool)> = vec![
        // --- flagged: a quoted name, a prefixed name, a directory-qualified read
        (format!("read_to_string({q}{name}{q})", q = '"'), true),
        (format!("read_to_string({q}{name}{q})", q = '\''), true),
        (format!("join({q}../{name}{q})", q = '"'), true),
        (format!("join({q}fixtures/{test_variant}{q})", q = '"'), true),
        (format!("{q}{name}{q} and {q}../{name}{q}", q = '"'), true),
        // --- legal: prose in a comment, and the opt-in variable itself
        (format!("see the crate-local {name} file for the key list"), false),
        (format!("{ENV_FILE_OPT_IN}=/path/to/file"), false),
        (format!("a variable named {}", ENV_FILE_OPT_IN), false),
        (format!("the file has no name of its own, see {name}"), false),
        // --- legal: a different file that merely starts the same way. The guard
        // must stay narrow, or it becomes noise nobody keeps.
        (format!("{q}{name}rc{q}", q = '"'), false),
        (format!("{q}{name}.example{q}", q = '"'), false),
    ];

    for (source, must_be_flagged) in cases {
        let flagged = env_guard_violation(&source).is_some();
        assert_eq!(
            flagged, must_be_flagged,
            "matcher disagreement on {source:?}: flagged={flagged}, expected={must_be_flagged}"
        );
    }
}

/// The opt-in is what decides whether a file is opened, and nothing else: no
/// value, no blank value, and no conventional default path.
#[test]
fn the_default_path_reads_the_process_environment_and_opens_nothing() {
    // Absent and blank both mean "not set" — never "fall back to a known file".
    assert_eq!(decode_opt_in(None), EnvFileOptIn::ProcessEnvironmentOnly);
    assert_eq!(decode_opt_in(Some("")), EnvFileOptIn::ProcessEnvironmentOnly);
    assert_eq!(decode_opt_in(Some("   \t ")), EnvFileOptIn::ProcessEnvironmentOnly);

    // A named path is honoured verbatim, trimmed — the developer is in charge.
    assert_eq!(
        decode_opt_in(Some("  /tmp/dz/probe  ")),
        EnvFileOptIn::Named(PathBuf::from("/tmp/dz/probe"))
    );

    // The default case reaches the file system zero times: it returns an empty
    // map even when a readable file with settings does exist at a path this
    // crate would happily have defaulted to.
    let probe = probe_file("dz-sqlserver-opt-in-default");
    let named = EnvFileOptIn::Named(probe.path.clone());
    assert_eq!(
        load_settings(&named).expect("the named probe is readable").len(),
        4,
        "the probe fixture must actually carry settings, or this proves nothing"
    );
    let settings = load_settings(&EnvFileOptIn::ProcessEnvironmentOnly)
        .expect("the default path never fails");
    assert!(
        settings.is_empty(),
        "the default path must contribute nothing without an opt-in, got {} keys",
        settings.len()
    );
    probe.remove();
}

/// A named file that cannot be read is an error, not a silently empty map — an
/// empty map is indistinguishable from "the file said nothing" and would let a
/// mis-typed path quietly downgrade a live run.
#[test]
fn an_unreadable_opt_in_file_is_reported_instead_of_ignored() {
    let missing = std::env::temp_dir().join(format!("dz-sqlserver-absent-probe-{}", probe_suffix()));
    assert!(!missing.exists(), "the probe path must not exist for this case");

    match load_settings(&EnvFileOptIn::Named(missing.clone())) {
        Err(EnvFileProblem::Unreadable { file_name, kind }) => {
            assert_eq!(
                file_name,
                missing.file_name().expect("a named path has a file name").to_string_lossy(),
                "the report must name the file the developer typed"
            );
            assert_eq!(
                kind, "NotFound",
                "only the OS error kind may be surfaced — never a line or a value"
            );
            // Nothing from the file system can leak into a report: it carries
            // the file name and the kind, nothing else.
            assert!(!kind.contains('/'), "{kind:?} must not carry a path");
        }
        Ok(settings) => panic!(
            "an unreadable opt-in file must be an error, got {} settings",
            settings.len()
        ),
    }
}

/// A named, readable file contributes exactly what it says, and quoting plus
/// comments behave — this is the behaviour local developers depend on, so it is
/// asserted rather than assumed.
#[test]
fn a_named_readable_file_contributes_its_settings() {
    let probe = probe_file("dz-sqlserver-opt-in-readable");
    let settings = load_settings(&EnvFileOptIn::Named(probe.path.clone()))
        .expect("the named probe is readable");

    assert_eq!(
        settings.get("TEST_SQLSERVER_HOST").map(String::as_str),
        Some("example.invalid"),
        "an unquoted value must survive"
    );
    assert_eq!(
        settings.get("TEST_SQLSERVER_PASSWORD").map(String::as_str),
        Some("single-quoted"),
        "single quotes must be stripped"
    );
    assert_eq!(
        settings.get("TEST_SQLSERVER_DATABASE").map(String::as_str),
        Some("double-quoted"),
        "a matching quote pair must be stripped"
    );
    assert_eq!(
        settings.get("TEST_SQLSERVER_SCHEMA").map(String::as_str),
        Some("\"unmatched"),
        "an unmatched quote is part of the value, not a delimiter"
    );
    assert!(
        !settings.contains_key("# a comment line"),
        "a comment line is not a setting"
    );
    assert!(
        !settings.contains_key("no_equals_sign"),
        "a line without a separator is not a setting"
    );
    probe.remove();
}

/// An unreadable opt-in is handled, never a panic — the live suite must be able
/// to skip on it.
#[test]
fn an_unreadable_opt_in_is_handled_never_a_panic() {
    let missing = PathBuf::from("/definitely/not/here/dz-sqlserver-probe");
    let outcome = std::panic::catch_unwind(|| load_settings(&EnvFileOptIn::Named(missing)))
        .expect("an unreadable opt-in must be handled, never a panic");
    assert!(
        outcome.is_err(),
        "an unreadable opt-in must surface as an error the caller can skip on"
    );
}

/// A throwaway settings file under the system temp directory. The name is not an
/// env-file name, so writing and reading it can never touch a protected file.
fn probe_file(stem: &str) -> Probe {
    let path = std::env::temp_dir().join(format!("{stem}-{}", probe_suffix()));
    let content = "\
# a comment line
TEST_SQLSERVER_HOST=example.invalid
TEST_SQLSERVER_PASSWORD='single-quoted'
TEST_SQLSERVER_DATABASE=\"double-quoted\"
TEST_SQLSERVER_SCHEMA=\"unmatched
no_equals_sign
";
    std::fs::write(&path, content)
        .unwrap_or_else(|e| panic!("cannot write the probe file {}: {e}", path.display()));
    Probe { path }
}

struct Probe {
    path: PathBuf,
}

impl Probe {
    fn remove(&self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A per-probe suffix, so two probes in the same process never collide on a path.
fn probe_suffix() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::SeqCst))
}