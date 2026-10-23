//! Independent probe of the IPC path redaction (Wave 5 · track A, FKU-D5).
//!
//! `redact_paths_for_ipc` masks the **directory** part of an absolute path and
//! keeps the file name. The claim it makes is precise, so this file checks it
//! against inputs the implementation's own unit tests do not cover:
//!
//! - a URL's path is not a path on disk and must survive;
//! - a relative path is not absolute and must survive;
//! - the credential redaction composes with the path redaction;
//! - and the **context rule** — "the path must not be part of a longer word" —
//!   must not be fooled by a `/` that belongs to something else.
//!
//! The cases below are the ones a real error message can contain: a `file:`
//! URL, a path with a space in it, a path with a colon in it, a path inside a
//! URL, a Windows path and a UNC share.
//!
//! The four cases that the first round of the audit found leaking are pinned
//! here as **fixed** behaviour (FKU2-D2/D3/D4): the old rule — "the previous
//! character is a colon, so it is part of a URL" plus a path body that stopped
//! at the first blank — let them through.

use datazen_platform_infra::redaction::{redact_paths_for_ipc, redact_secrets_for_log};

#[test]
fn masks_the_directory_and_keeps_the_file_name() {
    let out = redact_paths_for_ipc("cannot open /Users/alice/code/app.db for reading");
    assert_eq!(out, "cannot open ***/app.db for reading");
}

#[test]
fn leaves_a_url_path_intact() {
    for url in [
        "GET https://api.example.com/v1/users failed with 500",
        "postgres://user@127.0.0.1:5432/app refused",
        "https://host//double/slash",
    ] {
        assert_eq!(redact_paths_for_ipc(url), url, "url changed: {url}");
    }
}

#[test]
fn leaves_a_relative_path_intact() {
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
    assert!(!both.contains("/Users/alice"), "{both}");
    assert!(both.contains("mysql://***@127.0.0.1:3306/app"), "{both}");
    assert!(both.contains("app.db"), "{both}");
}

#[test]
fn masks_a_windows_path_and_a_unc_share() {
    assert_eq!(
        redact_paths_for_ipc(r"cannot open C:\Users\alice\AppData\app.db"),
        r"cannot open ***/app.db"
    );
    assert_eq!(
        redact_paths_for_ipc(r"cannot open \\server\share\app.db"),
        r"cannot open ***/app.db"
    );
}

/// The rule the implementation states: "the path must not be part of a longer
/// word".
///
/// A path that directly follows a colon is still a path: `:` is what a
/// message uses before a path ("cannot open:/Users/alice/app.db"), and the
/// decision is made by the **scheme** now, not by the character in front.
#[test]
fn a_path_after_a_colon_is_masked() {
    let out = redact_paths_for_ipc("cannot open:/Users/alice/app.db");
    assert!(!out.contains("/Users/alice"), "{out}");
    assert!(out.contains("app.db"), "{out}");
}

/// A path with a **space** in it: "My Documents" is a directory, and a
/// directory with a blank in it is the most common shape on macOS and
/// Windows. The whole directory part goes, the file name may stay.
#[test]
fn a_path_with_a_space_is_masked() {
    let out = redact_paths_for_ipc("cannot open /Users/alice/My Documents/app.db");
    assert!(!out.contains("/Users/alice"), "{out}");
    assert!(!out.contains("My Documents"), "{out}");
    assert!(out.contains("app.db"), "{out}");
}

/// A `file:` URL carries an absolute path inside it — and that path **is** a
/// path on disk, so it is masked like one.
#[test]
fn a_file_url_is_masked() {
    let out = redact_paths_for_ipc("cannot open file:///Users/alice/app.db");
    assert!(!out.contains("/Users/alice"), "{out}");
    assert!(out.contains("app.db"), "{out}");
}

/// A path whose segment contains a colon.
#[test]
fn a_path_with_a_colon_in_it_is_masked() {
    let out = redact_paths_for_ipc("cannot open /Users/alice/My:Vault/app.db");
    assert!(!out.contains("/Users/alice"), "{out}");
    assert!(!out.contains("Vault"), "{out}");
    assert!(out.contains("app.db"), "{out}");
}

/// A non-ASCII path.
#[test]
fn a_non_ascii_path_is_masked() {
    let out = redact_paths_for_ipc("cannot open /Users/张三/我的文档/app.db");
    assert!(!out.contains("/Users/张三"), "{out}");
    assert!(out.contains("app.db"), "{out}");
}

/// A single-segment absolute path (`/tmp`) is a directory and has no file name
/// to keep: the absolute form is what must not travel.
#[test]
fn a_single_segment_absolute_path_is_masked() {
    let out = redact_paths_for_ipc("cannot open /tmp for reading");
    assert!(!out.contains("/tmp"), "{out}");
}
