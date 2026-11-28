// Expected marks independently captured from the stock
// @codemirror/search 6.6.0 RegExpQuery.highlight and @codemirror/state 6.7.4.
// Runtime and release use no Node or JavaScript.

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64"))]
fn main() -> std::process::ExitCode {
    use datazen_regex_worker::{Error, HighlightRequest, WORKER_ARG, highlight, worker_main};
    use std::{ops::Range, process::ExitCode, time::Duration};

    if std::env::args_os().nth(1).is_some_and(|arg| arg == WORKER_ARG) {
        return worker_main();
    }

    fn check(
        label: &str,
        document: &str,
        viewport: Range<usize>,
        pattern: &str,
        want: Result<Vec<Range<usize>>, Error>,
    ) {
        let request = HighlightRequest {
            viewport,
            document,
            pattern,
            case_sensitive: true,
            whole_word: false,
        };
        assert_eq!(highlight(request, Duration::from_secs(1)), want, "{label}");
    }

    check("lookbehind retains full line prefix", &format!("{}acat{}", "x".repeat(49), " ".repeat(600)),
        300..300, "(?<=a)cat", Ok(vec![50..53]));
    check("start anchor cannot see the margin as a line", &format!("{}cat{}", "x".repeat(50), " ".repeat(600)),
        300..300, "^cat", Ok(vec![]));
    check("end anchor sees a clipped right margin", &format!("{}Ztail{}", "x".repeat(549), "x".repeat(100)),
        300..300, "Z$", Ok(vec![549..550]));
    check("marks just inside and outside either margin", &format!("X{}XX{}X{}XX{}", "x".repeat(48), "x".repeat(249), "x".repeat(248), "x".repeat(60)),
        300..300, "X", Ok(vec![50..51, 300..301, 549..550]));
    check("multiline crosses LF", &format!("{}\nfoo\n bar\nx", "x".repeat(100)),
        103..105, r"foo\s+bar", Ok(vec![101..109]));
    check("multiline anchor sees the clipped tail", &format!("{} Ztail{}", "x".repeat(548), "x".repeat(100)),
        300..300, r"\sZ$", Ok(vec![548..550]));
    check("wide multiline speculative chunk includes line end", &format!("{} Z", "x".repeat(5200)),
        0..4900, r"\sZ", Ok(vec![5200..5202]));

    check("wide multiline retries clipped anchor after speculation", &format!("{} Ztail", "x".repeat(5148)),
        0..4900, r"\sZ$", Ok(vec![5148..5150]));

    let within = format!("BEGIN\n{}x\nEND\n", "😀".repeat(80));
    let end = within.find("END").unwrap();
    check("80 astral scalars leave multiline start inside the margin", &within,
        end..end + 3, r"BEGIN\n[^\n]*\nEND", Ok(vec![0..end + 3]));
    let outside = format!("BEGIN\n{}x\nEND\n", "😀".repeat(130));
    let end = outside.find("END").unwrap();
    check("130 astral scalars move multiline start outside the margin", &outside,
        end..end + 3, r"BEGIN\n[^\n]*\nEND", Ok(vec![]));
    let half = format!("BEGIN\n{}\nEND\n", "😀".repeat(130));
    let end = half.find("END").unwrap();
    check("multiline margin cannot start at a half-surrogate", &half,
        end..end + 3, r"BEGIN\n[^\n]*\nEND", Err(Error::UnsupportedRange));

    let word = HighlightRequest {
        viewport: 0..1,
        document: "xcat CAT cat cat_ _cat cat2 cat-cat",
        pattern: "cat",
        case_sensitive: true,
        whole_word: true,
    };
    assert_eq!(highlight(word.clone(), Duration::from_secs(1)), Ok(vec![9..12, 28..31, 32..35]));
    assert_eq!(highlight(HighlightRequest { case_sensitive: false, ..word }, Duration::from_secs(1)),
        Ok(vec![5..8, 9..12, 28..31, 32..35]));

    check("right margin uses UTF-16 units", &format!("a{}cat{}tail", "😀".repeat(120), "😀".repeat(5)),
        0..0, "cat", Ok(vec![481..484]));
    check("zero-width progress uses original scalar edges", "😀cat",
        0..0, "(?=.)", Ok(vec![0..0, 4..4, 5..5, 6..6]));
    check("empty document may have a zero-width mark", "",
        0..0, "^$", Ok(vec![0..0]));
    check("exact complete 1000 marks", &"x".repeat(1000),
        0..1000, "x", Ok((0..1000).map(|i| i..i + 1).collect()));

    check("first over-cap mark produces no marks", &"x".repeat(1001),
        0..1001, "x", Err(Error::BudgetExceeded));
    check("invalid pattern from confined child", "x",
        0..0, "(", Err(Error::InvalidPattern));
    check("empty pattern is not a query", "x",
        0..0, "", Err(Error::Empty));
    check("unsplittable viewport midpoint", "😀cat",
        1..4, "cat", Err(Error::UnsupportedRange));
    check("unsplittable right margin", &format!("{}😀{}", "x".repeat(249), "x".repeat(30)),
        0..0, "x", Err(Error::UnsupportedRange));
    check("unsplittable left margin", &format!("😀{}", "x".repeat(251)),
        253..253, "x", Err(Error::UnsupportedRange));
    check("end beyond document is not clamped", "cat",
        0..4, "cat", Err(Error::UnsupportedRange));
    check("worker remains LF-only", "x\r\na",
        0..1, "a", Err(Error::UnsupportedRange));
    check("overlarge source cannot enter the isolated child", &"x".repeat(128 * 1024 + 1),
        0..1, "a", Err(Error::BudgetExceeded));

    println!("16 stock visible-mark cases and 10 fail-closed policy cases passed");
    ExitCode::SUCCESS
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64", target_pointer_width = "64")))]
fn main() {
    panic!("The passive regex worker oracle only supports Linux x86_64");
}
