//! Independent probe of the IPC path redaction, round 3 (Wave 5 · track A).
//!
//! The rule changed: instead of asking "is the character in front of the
//! path a colon?" (which protected `scheme://` and, as a side effect, let
//! `cannot open:/Users/…` through), the matcher now decides by the
//! **scheme**:
//!
//! 1. a `file://` URL — its path **is** a path on disk ⇒ masked;
//! 2. any other URL — its path is on the server ⇒ left alone;
//! 3. a bare absolute path ⇒ masked.
//!
//! The path body may now contain blanks in its inner segments ("My Documents"
//! is a directory); the **last** segment may not, so the tail of a sentence
//! is not mistaken for a file name.
//!
//! This file pins the **exact** output of every input, so a change in any of
//! them is visible in a diff — and it probes the rules the new shape opens
//! up, not only the ones it closes.

use datazen_platform_infra::redaction::{redact_paths_for_ipc, redact_secrets_for_log};

#[test]
fn masks_the_directory_and_keeps_the_file_name() {
    assert_eq!(
        redact_paths_for_ipc("cannot open /Users/alice/code/app.db for reading"),
        "cannot open ***/app.db for reading"
    );
}

#[test]
fn leaves_other_urls_alone() {
    for url in [
        "GET https://api.example.com/v1/users failed with 500",
        "postgres://user@127.0.0.1:5432/app refused",
        "https://host//double/slash",
        "ftp://host/pub/file.txt",
    ] {
        assert_eq!(redact_paths_for_ipc(url), url, "url changed: {url}");
    }
}

#[test]
fn leaves_relative_paths_alone() {
    for path in [
        "config/app.toml not found",
        "./relative/app.db missing",
        "../parent/app.db missing",
        "a/b/c/d.txt missing",
    ] {
        assert_eq!(redact_paths_for_ipc(path), path, "path changed: {path}");
    }
}

#[test]
fn composes_with_the_credential_redaction() {
    let raw = "mysql://root:s3cret@127.0.0.1:3306/app failed on /Users/alice/app.db";
    let secrets = redact_secrets_for_log(raw);
    assert!(!secrets.contains("s3cret"), "{secrets}");
    let both = redact_paths_for_ipc(&secrets);
    assert_eq!(both, "mysql://***@127.0.0.1:3306/app failed on ***/app.db");
}

#[test]
fn masks_windows_and_unc_paths() {
    assert_eq!(
        redact_paths_for_ipc(r"cannot open C:\Users\alice\AppData\app.db"),
        r"cannot open ***/app.db"
    );
    assert_eq!(
        redact_paths_for_ipc(r"cannot open \\server\share\app.db"),
        r"cannot open ***/app.db"
    );
}

/// G1: the path right behind a colon.
#[test]
fn masks_a_path_behind_a_colon() {
    assert_eq!(
        redact_paths_for_ipc("cannot open:/Users/alice/app.db"),
        "cannot open:***/app.db"
    );
}

/// G2: a path with a blank in it — the whole directory part goes.
#[test]
fn masks_a_path_with_a_blank_in_it() {
    assert_eq!(
        redact_paths_for_ipc("cannot open /Users/alice/My Documents/app.db"),
        "cannot open ***/app.db"
    );
}

/// G3: a `file://` URL — its path is a path on disk.
#[test]
fn masks_the_path_of_a_file_url() {
    assert_eq!(
        redact_paths_for_ipc("cannot open file:///Users/alice/app.db"),
        "cannot open file://***/app.db"
    );
}

/// G4: a single-segment path is a directory: there is no file name to keep.
#[test]
fn masks_a_single_segment_path() {
    assert_eq!(
        redact_paths_for_ipc("cannot open /tmp for reading"),
        "cannot open *** for reading"
    );
}

/// A single segment **with** an extension is a file: the name stays.
#[test]
fn keeps_the_name_of_a_single_segment_file() {
    assert_eq!(
        redact_paths_for_ipc("cannot open /app.db for reading"),
        "cannot open ***/app.db for reading"
    );
}

/// The tail of a sentence survives: the last segment may not hold a blank.
#[test]
fn keeps_the_tail_of_a_sentence() {
    assert_eq!(
        redact_paths_for_ipc("failed to read /Users/alice/app.db: no such file"),
        "failed to read ***/app.db: no such file"
    );
}

#[test]
fn masks_a_path_with_a_colon_in_an_inner_segment() {
    assert_eq!(
        redact_paths_for_ipc("cannot open /Users/alice/My:Vault/app.db"),
        "cannot open ***/app.db"
    );
}

#[test]
fn masks_a_non_ascii_path() {
    assert_eq!(
        redact_paths_for_ipc("cannot open /Users/张三/我的文档/app.db"),
        "cannot open ***/app.db"
    );
}

// ---------------------------------------------------------------------------
// The rules the new shape opens up — probed, not assumed.
// ---------------------------------------------------------------------------

/// The trade-off the implementation states: the last segment may not hold a
/// colon, so a file name that **literally** contains one is cut at the colon.
/// The directory part is still masked; only the tail of the *file name* — the
/// part the rule wants to keep anyway — survives.
#[test]
fn a_colon_in_the_last_segment_is_cut_but_the_directory_is_masked() {
    let out = redact_paths_for_ipc("cannot open /Users/alice/weird:name.db");
    println!("colon in the last segment: {out}");
    assert!(!out.contains("/Users/alice"), "{out}");
    assert!(!out.contains("alice"), "{out}");
}

/// A colon in the last segment of a **directory** (no file behind it): the
/// directory chain is masked, the tail of the last segment survives.
#[test]
fn a_colon_in_the_last_directory_segment_is_probed() {
    let out = redact_paths_for_ipc("cannot open /Users/alice/My:Vault");
    println!("colon in the last directory segment: {out}");
    assert!(!out.contains("/Users/alice"), "{out}");
}

/// A `file://` URL with an authority.
#[test]
fn a_file_url_with_an_authority_is_probed() {
    let out = redact_paths_for_ipc("cannot open file://host/Users/alice/app.db");
    println!("file url with an authority: {out}");
    assert!(!out.contains("/Users/alice"), "{out}");
}

/// An upper-case scheme.
#[test]
fn an_upper_case_file_url_is_masked() {
    let out = redact_paths_for_ipc("cannot open FILE:///Users/alice/app.db");
    println!("upper case file url: {out}");
    assert!(!out.contains("/Users/alice"), "{out}");
}

/// A `file:` URL with a **single** slash is not a URL the first branch knows;
/// it must still be masked as a bare path.
#[test]
fn a_single_slash_file_url_is_masked() {
    let out = redact_paths_for_ipc("cannot open file:/Users/alice/app.db");
    println!("single slash file url: {out}");
    assert!(!out.contains("/Users/alice"), "{out}");
}

/// A path that follows a URL's authority, separated by a space.
#[test]
fn a_path_after_a_url_is_masked() {
    let out = redact_paths_for_ipc("error in https://host: /Users/alice/app.db");
    println!("path after a url: {out}");
    assert!(!out.contains("/Users/alice"), "{out}");
    assert!(out.contains("https://host"), "{out}");
}

/// A protocol-relative URL (`//host/path`) is not a path on disk.
#[test]
fn a_protocol_relative_url_is_probed() {
    let out = redact_paths_for_ipc("see //host/share/file.txt");
    println!("protocol relative: {out}");
    assert!(out.contains("/host/share/file.txt"), "{out}");
}

/// A path in parentheses.
#[test]
fn a_path_in_parentheses_is_masked() {
    let out = redact_paths_for_ipc("cannot open (/Users/alice/app.db)");
    println!("in parentheses: {out}");
    assert!(!out.contains("/Users/alice"), "{out}");
    assert!(out.contains("app.db"), "{out}");
}

/// A home-relative path (`~/…`) is not absolute; masking it is harmless but
/// it is worth knowing what the rule does with it.
#[test]
fn a_home_relative_path_is_probed() {
    let out = redact_paths_for_ipc("cannot open ~/code/app.db");
    println!("home relative: {out}");
}
