//! Targeted known-gap regression through the same isolated child entry point.
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64"))]
fn main() -> std::process::ExitCode {
    use datazen_regex_worker::{
        Edit, Error, HighlightRequest, Operation, Outcome, Request, WORKER_ARG, highlight, search,
        worker_main,
    };
    use std::time::Duration;

    if std::env::args_os().nth(1).is_some_and(|arg| arg == WORKER_ARG) {
        return worker_main();
    }

    let deadline = Duration::from_millis(500);
    let document = "xbby";
    let pattern = r"(?:(?<k>a)|(?<\u006b>b))\k<k>";
    for operation in [
        Operation::Next,
        Operation::Previous,
        Operation::SelectAll,
        Operation::ReplaceNext,
        Operation::ReplaceAll,
    ] {
        assert_eq!(
            search(Request {
                operation, case_sensitive: true, whole_word: false, selection: 0..0,
                document, pattern, replacement: if matches!(operation,Operation::ReplaceAll | Operation::ReplaceNext) { "[$&]" } else { "" },
            }, deadline),
            Err(Error::UnsupportedPattern),
        );
    }
    assert_eq!(
        highlight(HighlightRequest {
            viewport: 0..document.len(), document, pattern, case_sensitive: true, whole_word: false,
        }, deadline),
        Err(Error::UnsupportedPattern),
    );

    let check = |pattern, range: std::ops::Range<usize>| {
        assert_eq!(
            search(Request {
                operation: Operation::ReplaceAll, case_sensitive: true, whole_word: false, selection: 0..0,
                document: "xabby", pattern, replacement: "[$&]",
            }, deadline),
            Ok(Outcome::Edits { edits: vec![Edit {
                range: range.clone(), insert: format!("[{}]", &"xabby"[range]),
            }], following: None }),
        );
    };
    check(r"(?<k>b)\k<k>", 2..4);
    check(r"(b)\1", 2..4);
    check(r"(?:(?<k>a)|(?<k>b))b+", 1..4);
    check(r"(?:(?<k>a)|(?<k>b))(?<unrelated>b)\k<unrelated>?", 1..4);
    println!("10 spawned compatibility witnesses passed");
    std::process::ExitCode::SUCCESS
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64")))]
fn main() {
    panic!("The independently confined worker is not enabled on this target");
}
