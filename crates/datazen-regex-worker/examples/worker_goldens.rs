// Expected results captured from @codemirror/search 6.6.0 and
// @codemirror/state 6.7.4 using only a synthetic editor. Offsets have been
// converted from the LF snapshot's UTF-16 positions to UTF-8 byte boundaries.
// This runner exercises the actual newly exec'd restricted worker.
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64"))]
fn main() -> std::process::ExitCode {
    use datazen_regex_worker::{Edit, Error, Operation, Outcome, Request, WORKER_ARG, search, worker_main};
    use std::time::Duration;

    if std::env::args_os().nth(1).is_some_and(|arg| arg == WORKER_ARG) {
        return worker_main();
    }

    let mut count = 0;
    let mut receipt = |r: Request<'_>, want: Result<Outcome, Error>, case: &str| {
        let op = r.operation;
        assert_eq!(search(r, Duration::from_secs(1)), want, "{case} {op:?}");
        count += 1;
    };
    {
        let case = "per-line cursor preserves preceding lookbehind context";
        let d = "x cat x cat";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 2..2,
            document: d,
            pattern: "(?<=x )cat",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![2..5])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![8..11])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![2..5, 8..11])), case);
    }
    {
        let case = "clipped wrapped tail anchor differs from global match";
        let d = "catZ";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 3..3,
            document: d,
            pattern: "cat$",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..3])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..3])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![])), case);
    }
    {
        let case = "wrapped native orientation must use end then start";
        let d = "one two one three one";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 19..21,
            document: d,
            pattern: "one",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..3])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![8..11])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..3, 8..11, 18..21])), case);
    }
    {
        let case = "real CM caret and range coordinates normalize CRLF/astral";
        let d = "😀\ncat\n猫 cat";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 5..8,
            document: d,
            pattern: "(cat)",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![13..16])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![13..16])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![5..8, 13..16])), case);
        r.operation = Operation::ReplaceNext;
        r.replacement = "$1🐈";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 5..8, insert: "cat🐈".to_owned() },
            ],
            following: Some(13..16)
        }), case);
        r.operation = Operation::ReplaceAll;
        r.replacement = "$1🐈";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 5..8, insert: "cat🐈".to_owned() },
                Edit { range: 13..16, insert: "cat🐈".to_owned() },
            ],
            following: None
        }), case);
    }
    {
        let case = "zero-width Next may have same coordinate; exact insert";
        let d = "😀\ncat cat\n";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 5..5,
            document: d,
            pattern: "(?=cat)",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![5..5])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![9..9])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![5..5, 9..9])), case);
        r.operation = Operation::ReplaceNext;
        r.replacement = "Z\\t";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 5..5, insert: "Z\t".to_owned() },
            ],
            following: Some(5..5)
        }), case);
        r.operation = Operation::ReplaceAll;
        r.replacement = "Z\\t";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 5..5, insert: "Z\t".to_owned() },
                Edit { range: 9..9, insert: "Z\t".to_owned() },
            ],
            following: None
        }), case);
    }
    {
        let case = "Replace Next navigates if selection not a full match";
        let d = "cat cat";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 0..2,
            document: d,
            pattern: "(cat)",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![4..7])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![4..7])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..3, 4..7])), case);
        r.operation = Operation::ReplaceNext;
        r.replacement = "$&Z";
        receipt(r.clone(), Ok(Outcome::Select(vec![0..3])), case);
        r.operation = Operation::ReplaceAll;
        r.replacement = "$&Z";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 0..3, insert: "catZ".to_owned() },
                Edit { range: 4..7, insert: "catZ".to_owned() },
            ],
            following: None
        }), case);
    }
    {
        let case = "Replace Next next match is from original text, excludes fresh match";
        let d = "猫 one one";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 4..7,
            document: d,
            pattern: "(one)",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![8..11])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![8..11])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![4..7, 8..11])), case);
        r.operation = Operation::ReplaceNext;
        r.replacement = "g$1";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 4..7, insert: "gone".to_owned() },
            ],
            following: Some(8..11)
        }), case);
        r.operation = Operation::ReplaceAll;
        r.replacement = "g$1";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 4..7, insert: "gone".to_owned() },
                Edit { range: 8..11, insert: "gone".to_owned() },
            ],
            following: None
        }), case);
    }
    {
        let case = "CM expansion is not JS String.replace: unmatched, numbered fallback, named literal";
        let d = "alpha beta";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 0..5,
            document: d,
            pattern: "(?<a>alpha)|(?<b>beta)",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![6..10])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![6..10])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..5, 6..10])), case);
        r.operation = Operation::ReplaceNext;
        r.replacement = "$1|$2|$12|$&|$$|$<a>|$03|\\n";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 0..5, insert: "alpha|undefined|alpha2|alpha|$|$<a>|$03|\n".to_owned() },
            ],
            following: Some(6..10)
        }), case);
        r.operation = Operation::ReplaceAll;
        r.replacement = "$1|$2|$12|$&|$$|$<a>|$03|\\n";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 0..5, insert: "alpha|undefined|alpha2|alpha|$|$<a>|$03|\n".to_owned() },
                Edit { range: 6..10, insert: "undefined|beta|undefined2|beta|$|$<a>|$03|\n".to_owned() },
            ],
            following: None
        }), case);
    }
    {
        let case = "CM Match Case + Whole Word uses the actual regexp search cursor";
        let d = "CAT 猫cat CAT cat_cat (cat) cat猫 cat";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: true,
            whole_word: true,
            selection: 0..0,
            document: d,
            pattern: "cat",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![24..27])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![36..39])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![24..27, 36..39])), case);
    }
    {
        let case = "expanded CR and CRLF are normalized by CM edit construction";
        let d = "cat cat";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 0..3,
            document: d,
            pattern: "(cat)",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![4..7])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![4..7])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..3, 4..7])), case);
        r.operation = Operation::ReplaceNext;
        r.replacement = "\\r\\n$1\\r";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 0..3, insert: "\ncat\n".to_owned() },
            ],
            following: Some(4..7)
        }), case);
        r.operation = Operation::ReplaceAll;
        r.replacement = "\\r\\n$1\\r";
        receipt(r.clone(), Ok(Outcome::Edits {
            edits: vec![
                Edit { range: 0..3, insert: "\ncat\n".to_owned() },
                Edit { range: 4..7, insert: "\ncat\n".to_owned() },
            ],
            following: None
        }), case);
    }
    {
        let case = "invalid syntax leaves everything untouched";
        let d = "cat cat";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 0..3,
            document: d,
            pattern: "(",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Err(Error::InvalidPattern), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Err(Error::InvalidPattern), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Err(Error::InvalidPattern), case);
        r.operation = Operation::ReplaceNext;
        r.replacement = "never";
        receipt(r.clone(), Err(Error::InvalidPattern), case);
        r.operation = Operation::ReplaceAll;
        r.replacement = "never";
        receipt(r.clone(), Err(Error::InvalidPattern), case);
    }
    {
        let case = "empty search leaves everything untouched";
        let d = "cat";
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 0..0,
            document: d,
            pattern: "",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Err(Error::Empty), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Err(Error::Empty), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Err(Error::Empty), case);
        r.operation = Operation::ReplaceNext;
        r.replacement = "never";
        receipt(r.clone(), Err(Error::Empty), case);
        r.operation = Operation::ReplaceAll;
        r.replacement = "never";
        receipt(r.clone(), Err(Error::Empty), case);
    }
    {
        let case = "select-all exact accepted edge";
        let d = "x".repeat(1000); let d = d.as_str();
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 0..0,
            document: d,
            pattern: "x",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..1])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![999..1000])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Ok(Outcome::Select((0..1000).map(|n| n..n+1).collect())), case);
    }
    {
        let case = "select-all rejects next item, no truncated success";
        let d = "x".repeat(1001); let d = d.as_str();
        let mut r = Request {
            operation: Operation::Next,
            case_sensitive: false,
            whole_word: false,
            selection: 0..0,
            document: d,
            pattern: "x",
            replacement: "",
        };
        r.operation = Operation::Next;
        receipt(r.clone(), Ok(Outcome::Select(vec![0..1])), case);
        r.operation = Operation::Previous;
        receipt(r.clone(), Ok(Outcome::Select(vec![1000..1001])), case);
        r.operation = Operation::SelectAll;
        receipt(r.clone(), Err(Error::BudgetExceeded), case);
    }
    assert_eq!(count, 58);
    println!("{count} pinned CodeMirror command outcomes passed in spawned worker");
    std::process::ExitCode::SUCCESS
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64")))]
fn main() {
    panic!("The regex worker golden runner only supports Linux x86_64");
}
