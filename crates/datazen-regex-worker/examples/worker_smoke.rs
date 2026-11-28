#[cfg(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64"))]
fn main() -> std::process::ExitCode {
    use datazen_regex_worker::{Edit, Error, Operation, Outcome, Request, WORKER_ARG, search, worker_main};
    use std::time::Duration;

    if std::env::args_os().nth(1).is_some_and(|arg| arg == WORKER_ARG) {
        return worker_main();
    }

    let request = Request {
        operation: Operation::ReplaceAll, case_sensitive: true, whole_word: false,
        selection: 0..0, document: "猫 one One one", pattern: "(one)", replacement: "g$1",
    };
    let expected = vec![Edit { range: 4..7, insert: "gone".to_owned() }, Edit { range: 12..15, insert: "gone".to_owned() }];
    assert_eq!(search(request.clone(), Duration::from_secs(1)), Ok(Outcome::Edits { edits: expected, following: None }));
    assert_eq!(search(Request { pattern: "(", ..request.clone() }, Duration::from_secs(1)), Err(Error::InvalidPattern));
    assert_eq!(search(Request { document: "𖺠猫A", pattern: r"\p{L}+", operation: Operation::SelectAll, replacement: "", ..request.clone() },
        Duration::from_secs(1)), Ok(Outcome::Select(vec![0..8])));
    assert_eq!(search(Request { operation: Operation::ReplaceNext, selection: 4..7, ..request.clone() }, Duration::from_secs(1)),
        Ok(Outcome::Edits { edits: vec![Edit { range: 4..7, insert: "gone".to_owned() }], following: Some(12..15) }));
    assert_eq!(search(Request { operation: Operation::ReplaceNext, selection: 0..0, ..request }, Duration::from_secs(1)),
        Ok(Outcome::Select(vec![4..7])));
    println!("5 spawned confined worker commands passed");
    std::process::ExitCode::SUCCESS
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64")))]
fn main() {
    panic!("The regex worker smoke only supports Linux x86_64");
}
