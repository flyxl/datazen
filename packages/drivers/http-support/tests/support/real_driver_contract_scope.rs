//! Scope tier: what the guards cover, what they provably cannot, and the honest
//! inventory a green run must never be read as.
//!
//! Three separate questions land here, and they belong together because each one
//! is a limit of the layer below it. The source-level env-file guard sees only
//! one driver crate, so a crate that does not bind the template is scanned by
//! nobody and has to be *named* here instead. The guard reads source text, so it
//! is a scan and not a sandbox. And a required dimension whose live test skipped
//! is unverified, not passed — the report that says so runs unconditionally, on
//! every run, whether or not a server was present.
//!
//! `#[path]`-included, never compiled alone.

#![allow(dead_code)]

use super::*;

/// The guard above only ever sees **one** driver crate: `env!("CARGO_MANIFEST_DIR")`
/// resolves to the crate that included this template, not to the crate the file
/// physically lives in. A crate that does not bind the template is therefore
/// scanned by nobody, silently — which is how `sqlserver` kept reading a local
/// env file while a guard named `test_sources_never_read_env_files` stayed green.
///
/// This test makes that scope explicit instead of accidental. A driver crate that
/// ships a `tests/` directory must either bind the template (and so be scanned
/// whenever its own contract test runs) or be listed here with its reason. Adding
/// a driver crate and forgetting the list therefore fails loudly, and fixing a
/// listed crate and forgetting to remove it also fails loudly.
#[test]
fn every_driver_crate_with_tests_either_binds_this_guard_or_is_declared() {
    let root = drivers_root();
    let template = template_sources();
    let mut declared: Vec<String> = UNGUARDED_DRIVER_CRATES
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    declared.sort();

    let mut scanned_by_nobody: Vec<String> = Vec::new();
    let entries =
        std::fs::read_dir(&root).unwrap_or_else(|e| panic!("cannot list {}: {e}", root.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.join("tests").is_dir() {
            continue;
        }
        // A crate is covered when it runs the template, or when it *is* the
        // template's home and its sources are already in the scanned set.
        let bound = path.join("tests/real_driver_contract.rs").is_file();
        let is_template_home =
            !template.is_empty() && template.iter().all(|src| src.starts_with(&path));
        if bound || is_template_home {
            continue;
        }
        scanned_by_nobody.push(entry.file_name().to_string_lossy().into_owned());
    }
    scanned_by_nobody.sort();

    assert_eq!(
        scanned_by_nobody, declared,
        "driver crates that ship tests but are scanned by no run of this guard: \
         {scanned_by_nobody:?} vs declared {declared:?}"
    );
    for (name, reason) in UNGUARDED_DRIVER_CRATES {
        assert!(
            drivers_root().join(name).join("tests").is_dir(),
            "UNGUARDED_DRIVER_CRATES lists `{name}` but that crate no longer ships tests: \
             remove it from the list"
        );
        // A gap with no stated reason is an undocumented gap: the next reader
        // cannot tell an accepted trade-off from an oversight.
        assert!(
            !reason.trim().is_empty(),
            "UNGUARDED_DRIVER_CRATES lists `{name}` without saying why it is not covered"
        );
    }
}

/// Driver crates that ship a `tests/` directory but never run this guard, with
/// the reason each one is not covered yet. Every entry is a known, visible gap
/// in the §10.2 rule 5 enforcement, not a silent one.
const UNGUARDED_DRIVER_CRATES: &[(&str, &str)] = &[
    (
        "clickhouse",
        "no contract binding yet; no env-file read in its tests today",
    ),
    (
        "duckdb",
        "no contract binding yet; no env-file read in its tests today",
    ),
    (
        "mongodb",
        "no contract binding yet; no env-file read in its tests today",
    ),
    (
        "redis",
        "no contract binding yet; no env-file read in its tests today",
    ),
    (
        "sqlite",
        "no contract binding yet; no env-file read in its tests today",
    ),
    (
        "sqlserver",
        "no contract binding yet (its capability matrix is unverified against a real \
         server, so binding the full template here would be a claim, not a check); \
         the env-file rule it used to break is now enforced inside its own crate by \
         tests/live_env_file_opt_in.rs, which scans this crate's sources and runs on \
         every `cargo test -p datazen-driver-sqlserver`",
    ),
];

/// The guard's own matcher, proven against the spellings that must be caught and
/// the ones that must stay legal.
///
/// Without this, a token list can quietly stop matching the shape it was written
/// for and a green guard keeps asserting nothing. That is exactly how a
/// directory-qualified `read_to_string` of a prefixed name and a reader of the
/// `test` variant both passed a guard written to forbid them.
#[test]
fn the_env_guard_matches_every_shape_of_env_file_literal() {
    let name = env_file_names()[0];
    let test_variant = env_file_names()[1];

    // (source, must_be_flagged)
    let cases: Vec<(String, bool)> = vec![
        // --- must be flagged: a quoted path that names an env file
        (format!("read_to_string({q}{name}{q})", q = '"'), true),
        (format!("read_to_string({q}{name}{q})", q = '\''), true),
        (format!("read_to_string({q}../{name}{q})", q = '"'), true),
        (format!("dir.join({q}fixtures/{name}{q})", q = '"'), true),
        (format!("read_to_string({q}x{name}{q})", q = '"'), true),
        (
            format!("read_to_string({q}{test_variant}{q})", q = '"'),
            true,
        ),
        (format!("dir.join({q}{test_variant}{q})", q = '"'), true),
        (
            format!("read_to_string({q}{test_variant}{q})", q = '\''),
            true,
        ),
        (
            format!("read_to_string({q}../{test_variant}{q})", q = '"'),
            true,
        ),
        // --- must be flagged: an implicit loader
        (concat!("load_", "dotenv").to_string(), true),
        (concat!("dot", "env::from_filename").to_string(), true),
        (concat!("dot", "envy::from_filename").to_string(), true),
        // --- must stay legal: prose in backticks, and substrings, not paths
        (
            format!("//! why no {q}{name}{q} file is read", q = '`'),
            false,
        ),
        (
            format!(
                "/// must never parse {q}packages/drivers/{name}{q}",
                q = '`'
            ),
            false,
        ),
        (
            format!(
                "/// configured in {q}drivers/sqlserver/{test_variant}{q}",
                q = '`'
            ),
            false,
        ),
        ("contract.env_prefix".to_string(), false),
        ("TEST_PG_DATABASE".to_string(), false),
        ("sql_guard".to_string(), false),
    ];

    for (source, must_flag) in &cases {
        assert_eq!(
            env_guard_violation(source).is_some(),
            *must_flag,
            "guard disagreed about {source:?}"
        );
    }
}

/// **The guard's boundary, pinned as a test instead of a caveat.**
///
/// `env_file_names` is a fixed list, and `env_guard_violation` looks for those
/// literals — closing quote included — in source text. It is a **static** scan: it
/// sees a name that is written down, never a name that is assembled while the
/// program runs. A probe that builds the name with `concat!` and formats it into a
/// path is therefore clean text, and the suite stays green.
///
/// That is a real limit of static scanning, not a defect to be papered over, and
/// no token list can close it: the offending string does not exist until after
/// `main` has started, so there is nothing in the source for any static rule to
/// match. What *can* be done is make the limit impossible to forget, which is what
/// this test does — it demonstrates the gap, names it, and keeps the assembled
/// name on the list the guard watches for. The same limit is stated in
/// `env_file_names`'s own doc comment and in `unverified_scope_report`.
///
/// The two halves below are both asserted, because either one alone would be
/// vacuous: a probe that assembles a name the guard does not watch proves nothing
/// about the guard, and a source that fails to assemble proves nothing about the
/// gap.
#[test]
fn the_static_env_guard_cannot_see_a_name_assembled_at_runtime() {
    // Assembled at runtime, so the source that follows holds no env-file literal.
    let assembled: String = concat!(".en", "v").to_string();
    assert!(
        env_file_names().contains(&assembled.as_str()),
        "the probe must assemble a name the guard actually watches for, or it demonstrates \
         nothing: {assembled:?} vs {:?}",
        env_file_names()
    );

    // Half one: once written into a source string, this IS caught — the matcher is
    // fine, and any runtime name that reaches a *string* is visible to it.
    let as_written_text = format!("read_to_string({q}{assembled}{q})", q = '"');
    assert!(
        env_guard_violation(&as_written_text).is_some(),
        "the assembled name written into text must be flagged; if the matcher stopped \
         matching {assembled:?} the boundary below would be a distraction, not a limit"
    );

    // Half two: the source that produces that name is not caught. This is the gap,
    // asserted rather than described.
    let runtime_construction = concat!(
        "let name = concat!(",
        "\".en\",",
        "\"v\");",
        "std::fs::read_to_string(format!(\"{}\", name));"
    );
    assert_eq!(
        env_guard_violation(runtime_construction),
        None,
        "the guard now flags a name assembled at runtime — good, but then this test's stated \
         boundary is out of date and the doc comment on env_file_names must be rewritten to \
         say what the guard can now see"
    );

    // And the honest consequence is not left implicit: the report has to carry it.
    let report = scope_text(Err("probe".to_string()));
    assert!(
        report.contains("运行期") && report.contains("扫不到"),
        "the report must state the static-scan boundary in these words; a limit recorded only \
         in a test name is not a limit anyone reads. Report reads:\n{report}"
    );
}

/// Anti-silent-drop guard: a required dimension may not stay in the matrix while
/// its live test quietly disappears.
#[test]
fn every_required_dimension_owns_a_named_test() {
    let source = template_sources()
        .iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        })
        .collect::<Vec<_>>()
        .join("\n");
    for (id, test, _) in REQUIRED_DIMENSIONS {
        assert!(
            source.contains(&format!("async fn {test}(")),
            "{id} is listed as required but has no live test named {test}"
        );
    }
}

/// A dimension whose live test exists but proves only **part** of what the matrix
/// asks of it.
///
/// This is table data rather than an `if` inside the live test, because the
/// failure it replaces was exactly an `if` that never fired. CM-26's honesty note
/// used to sit behind `if !reset_for_reuse.is_present()`, while every driver under
/// test declares `reset_for_reuse = Supported` — so the condition was constantly
/// false, the note never printed (zero occurrences across two real live runs), and
/// the scope report still listed CM-26 among the dimensions that "really executed
/// and passed". A table cannot go quiet: [`unverified_scope_report`] prints every
/// row on every run, and `partial_obligations_are_never_reported_as_passed` fails
/// if one is missing from the report or leaks into a passed claim.
pub struct PartialObligation {
    /// The contract-matrix id this obligation belongs to.
    pub dimension: &'static str,
    /// The contract capability field that has no testable surface behind it.
    pub capability: &'static str,
    /// The half the live test really asserts, in plain words.
    pub asserted: &'static str,
    /// The half it does not, and therefore may never be claimed.
    pub missing: &'static str,
    /// Why the missing half is untestable on the current trait surface.
    pub reason: &'static str,
}

/// One `Contract` capability field that has no `DatabaseDriver` method behind it.
pub struct CapabilityWithoutTraitSurface {
    /// The `Contract` capability field name.
    pub capability: &'static str,
    /// Why the binding cannot be enforced at runtime, in plain words.
    pub reason: &'static str,
}

/// The `Contract` capability fields that are **declarations with no
/// `DatabaseDriver` method behind them**.
///
/// A capability declared `Supported` here is unverifyable by construction: there
/// is no call to make, so no test can ever exercise it, and reporting it as
/// verified would be a claim about nothing. Every entry owes a
/// [`PartialObligation`], and the two lists are checked against each other from
/// both sides — which is what stops `PARTIAL_OBLIGATIONS` from being emptied to
/// make a claim disappear, the way a guarded `eprintln!` was silenced by a
/// condition that never fired.
///
/// Listing a capability here is not a fix: it is the acknowledgement that the
/// declaration is unenforceable, so it may no longer be counted as verified.
pub const CAPABILITIES_WITHOUT_TRAIT_SURFACE: &[CapabilityWithoutTraitSurface] =
    &[CapabilityWithoutTraitSurface {
        capability: "reset_for_reuse",
        reason: "`DatabaseDriver` exposes no reset/release-for-reuse method. Flipping the \
                  binding to `Unsupported` left every contract outcome byte-identical and \
                  EXIT=0, while a control probe asserting the same field failed under that \
                  mutation (EXIT=101): the value provably reaches runtime, but no branch \
                  consumes it. All 15 driver crates declare `ResetForReuse::Unsupported` in \
                  production, so a `Supported` binding could only ever have been false.",
    }];

/// Every half-asserted dimension, whether or not its live test ever ran.
pub const PARTIAL_OBLIGATIONS: &[PartialObligation] = &[PartialObligation {
    dimension: "CM-26",
    capability: "reset_for_reuse",
    asserted: "a failed statement leaves nothing blindly reused: the open resource set \
               stays coherent, an unopened target is not reported as open, and the session \
               still works without repair",
    missing: "resources are *safely reusable* after a failure — that a reset/release entry \
              point exists, resets what a failure dirtied, and refuses when it cannot",
    reason: "`DatabaseDriver` exposes no reset/release-for-reuse method, so the declared \
             reset_for_reuse capability has no call to assert and cannot be verified; it is \
             reported as unverified rather than counted as passed",
}];

/// The obligation recorded for `dimension`, or `None` if the dimension owes no
/// half-honest note.
pub fn partial_obligation(dimension: &str) -> Option<&'static PartialObligation> {
    PARTIAL_OBLIGATIONS
        .iter()
        .find(|o| o.dimension == dimension)
}

/// Print the note. Unconditional by construction — the caller looks the row up and
/// reports it, rather than deciding whether it is worth saying.
pub fn report_partial_obligation(o: &PartialObligation) {
    eprintln!(
        "⚠  {} 的 {} 只验证了一半：\n     已验证：{}\n     未验证：{}\n     原因：{}\n     \
         该维度不计入『已真实执行并通过』。",
        crate::CONTRACT.label,
        o.dimension,
        o.asserted,
        o.missing,
        o.reason
    );
}

/// The unscanned-crate section's one fixed policy sentence.
///
/// Deliberately **not** interpolated with anything: the count and the crate names
/// are printed on their own lines out of `UNGUARDED_DRIVER_CRATES`, and the reason
/// each crate is unbound stays in code rather than becoming prose here. That is
/// the whole point — see [`unscanned_crate_section`].
const UNGUARDED_CRATES_POLICY: &str = "以下 crate 带有 tests/ 目录，却从不绑定本契约模板，因此它们的测试源不在上面『无 env 文件读取』的结论范围内；本 guard 不对它们作任何断言，列为免检是一项已记录的缺口，不是一项结论。未绑定的原因写在 UNGUARDED_DRIVER_CRATES 的代码注释里，不打印成散文，以免自由文本变成新的声明面。";

/// The report's section about the crates this guard does **not** scan.
///
/// Built from machine data plus one fixed literal — no hand-written crate list, and
/// no free-text slot beside the names.
///
/// That slot is why the previous version needed a word blacklist to defend it. The
/// section was assembled inline in [`scope_text`], so appending a sentence to it
/// was a legal edit, and this one —
///
/// > 以上 crate 均不读取 env 文件，已纳入防护体系，具有同等保护力度。
///
/// — passed all 28 tests while contradicting the line two lines above it. Eight
/// forbidden words did not close that, because the next sentence simply avoids all
/// eight. Generating the section removes the slot; the golden in
/// `the_report_names_every_crate_this_guard_does_not_scan` closes the rest.
fn unscanned_crate_section() -> String {
    let names = UNGUARDED_DRIVER_CRATES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\n--- 免检驱动 crate（共 {} 个）：本 guard 不扫描，其结论不被上面的扫描覆盖 ---\n     \
         {UNGUARDED_CRATES_POLICY}\n     清单：{names}\n",
        UNGUARDED_DRIVER_CRATES.len(),
    )
}

/// The honest inventory of what a run did **not** verify, as text.
///
/// Built by a function so the claims can be asserted on instead of merely
/// printed: a report that only exists as `eprintln!` lines cannot be checked for
/// saying something false.
///
/// `availability` is a **parameter**, not a call. Taking it as an argument is what
/// makes the "live tier is ready" wording testable in a run that has no live tier
/// — which is the only kind of run a reviewer or CI ever has. The first version
/// of this fix branched on the real `availability()` and asserted on the result,
/// and the assertion had no subject: with no `TEST_PG_*` in the environment the
/// `Ok` arm is never taken, the over-claim being guarded against was never even
/// produced, and the guard passed. That is the CM-26 defect reproduced one level
/// up, so the branch became something a test can hold both ways — and
/// `check_report_claims` holds it in *both* directions, because a guard over only
/// the arm this run happens to take would miss the one CI takes.
fn scope_text(availability: Result<(), String>) -> String {
    let contract = &crate::CONTRACT;
    let mut out = String::new();
    out.push_str(&format!(
        "\n=== {} real-driver contract: 未验证范围 ===\n",
        contract.label
    ));
    match availability {
        Ok(()) => out.push_str(
            "live 层前置条件齐备；下面列出的维度，其 live 测试只有在本次真实数据库运行中才成立。\n",
        ),
        Err(reason) => out.push_str(&format!("live 层未启用：{reason}\n")),
    }
    for (id, test, what) in REQUIRED_DIMENSIONS {
        out.push_str(&format!("  - {id} / {test}: {what}\n"));
    }
    out.push_str(
        "free 层（声明自洽、能力区分、拒绝断言、幂等性、无 env 文件读取）已实际执行并通过。\n",
    );
    out.push_str(&format!(
        "     边界：上面的『无 env 文件读取』是**静态文本扫描**的结论——它扫得到源码里写下来的\
         文件名，扫不到运行期才拼出来的文件名。运行期拼装的 env 文件名，本 guard 拦不住，\
         这一点没有任何静态规则能改变（字符串在 main 运行后才存在）。详见 \
         the_static_env_guard_cannot_see_a_name_assembled_at_runtime。\n"
    ));
    out.push_str(&unscanned_crate_section());
    out.push_str("\n--- 只验证了一半的维度（不计入已通过）---\n");
    if PARTIAL_OBLIGATIONS.is_empty() {
        out.push_str("  （无）\n");
    }
    for o in PARTIAL_OBLIGATIONS {
        out.push_str(&format!(
            "  - {} 已验证：{}\n      未验证：{}\n      原因：{}\n",
            o.dimension, o.asserted, o.missing, o.reason
        ));
    }
    out.push_str("============================================\n");
    out
}

/// The honest inventory of what a run did **not** verify. Always runs, so a green
/// suite can never be read as "capabilities verified" when no real database was
/// involved. Use `-- --nocapture` to read the reasons.
#[test]
fn unverified_scope_report() {
    eprint!("{}", scope_text(crate::CONTRACT.availability()));
}

/// The unscanned-crate section, pinned as a **golden**.
///
/// Written out by hand rather than derived from `UNGUARDED_CRATES_POLICY` or
/// `UNGUARDED_DRIVER_CRATES`: if the test built its expectation out of the same
/// constants the report is rendered from, editing a word would rewrite the check
/// along with it, and the assertion would be hollow. Two independent literals are
/// the only version of this guard that says anything.
///
/// What it buys, stated without overselling: the section cannot gain an
/// **unreviewed** sentence, and it cannot drift from the crate list — the two
/// mutations that shipped green straight through this guard. What it does not buy
/// is judgement: whoever edits the section edits this pin in the same commit, and
/// no assertion can tell whether the new sentence is *honest*. Prose elsewhere in
/// the report sits outside this pin by design; that is what the availability and
/// PARTIAL_OBLIGATIONS checks are for.
const UNGUARDED_CRATES_GOLDEN: &str = "\
\n--- 免检驱动 crate（共 6 个）：本 guard 不扫描，其结论不被上面的扫描覆盖 ---
     以下 crate 带有 tests/ 目录，却从不绑定本契约模板，因此它们的测试源不在上面『无 env 文件读取』的结论范围内；本 guard 不对它们作任何断言，列为免检是一项已记录的缺口，不是一项结论。未绑定的原因写在 UNGUARDED_DRIVER_CRATES 的代码注释里，不打印成散文，以免自由文本变成新的声明面。
     清单：clickhouse, duckdb, mongodb, redis, sqlite, sqlserver
";

/// The crates this guard does **not** scan must be named in the report, by name
/// and by count.
///
/// Not adding a guard to them is the right call — they have no contract binding
/// yet, and pretending otherwise would be a claim. What is not acceptable is an
/// unnamed gap, because an unnamed gap reads as coverage by whoever reads the
/// output later. A probe that injected an env-file read into `duckdb`'s tests
/// passed the whole suite: the suite was never going to see it, and the report
/// has to say so out loud rather than let the green result imply otherwise.
#[test]
fn the_report_names_every_crate_this_guard_does_not_scan() {
    assert!(
        !UNGUARDED_DRIVER_CRATES.is_empty(),
        "UNGUARDED_DRIVER_CRATES is empty while driver crates with tests remain unbound; if \
         every crate really is covered now, delete this test and the report section together"
    );

    let report = scope_text(Err("probe".to_string()));
    assert!(
        report.contains(&format!(
            "免检驱动 crate（共 {} 个）",
            UNGUARDED_DRIVER_CRATES.len()
        )),
        "the report must state how many crates escape the scan; a count is what tells a \
         reader whether the list below it is complete. Report reads:\n{report}"
    );
    for (name, _reason) in UNGUARDED_DRIVER_CRATES {
        assert!(
            report.contains(name),
            "the report must name the unscanned crate `{name}`; an unlisted gap is \
             indistinguishable from a covered crate"
        );
    }

    // The whole section, compared to the golden — not a per-name check, and not a
    // word blacklist. The blacklist was the previous answer and it did not hold:
    // appending 「以上 crate 均不读取 env 文件，已纳入防护体系，具有同等保护力度。」
    // avoids all eight forbidden words and left all 28 tests green while the report
    // contradicted the line above it. Whack-a-mole was never going to close a
    // free-text slot, so the slot is gone (see [`unscanned_crate_section`]) and
    // what is left is pinned.
    let start = report.find("\n--- 免检驱动 crate").unwrap_or_else(|| {
        panic!("the report has no unscanned-crate section at all, so the gap is unnamed: {report}")
    });
    let rest = &report[start..];
    let end = rest[1..].find("\n---").map_or(rest.len(), |i| i + 1);
    let section = &rest[..end];
    assert_eq!(
        section, UNGUARDED_CRATES_GOLDEN,
        "the unscanned-crate section no longer matches the golden. Every crate named in it \
         escapes this guard's scan, so any wording change here is a change to what a reader is \
         told about coverage — if the new text is accurate, re-pin the golden in the same commit."
    );
    assert!(
        section.contains("duckdb"),
        "the golden must actually list the crates; a section that only says 'the rest' is not \
         a report"
    );

    // And the boundary has to be attached to the *claim* it qualifies, not left as
    // a footnote: a reader who reads only the free-tier line and stops must still
    // be told, so the caveat comes first and no other claim may be interposed.
    let after: Vec<&str> = report
        .split('\n')
        .skip_while(|line| !line.contains("free 层"))
        .skip(1)
        .take(3)
        .collect();
    assert!(
        after
            .first()
            .is_some_and(|line| line.contains("运行期") && line.contains("扫不到")),
        "the static-scan boundary must be the line directly under the free-tier pass claim, \
         so a reader who stops there is not misled. Following lines: {after:?}\n{report}"
    );
    assert!(
        after.iter().any(|line| line.contains("免检驱动 crate")),
        "the unscanned-crate list must follow the free-tier claim without another claim in \
         between. Following lines: {after:?}\n{report}"
    );
}

/// A half-asserted dimension may never be counted as passed, and may never be
/// quietly dropped from the report.
///
/// Two failure modes are checked, and both were live: a row that vanishes from the
/// table (the dimension silently returns to the passed column), and a report that
/// keeps claiming full coverage for a dimension with a missing half.
#[test]
fn partial_obligations_are_never_reported_as_passed() {
    // Non-vacuity first. Without these two, deleting every row would make this
    // test pass by having nothing left to check — the same "the guard has no
    // subject" hole the unreachable note had.
    assert!(
        !CAPABILITIES_WITHOUT_TRAIT_SURFACE.is_empty(),
        "no capability is listed as lacking a testable surface, so this test cannot tell a \
         genuinely complete contract from an emptied one"
    );
    assert!(
        !PARTIAL_OBLIGATIONS.is_empty(),
        "PARTIAL_OBLIGATIONS is empty while {} still declares capabilities nothing can test; an \
         empty table is indistinguishable from a full pass",
        crate::CONTRACT.label
    );

    // The two lists must agree in both directions, one row per untestable field.
    for capability in CAPABILITIES_WITHOUT_TRAIT_SURFACE {
        let name = capability.capability;
        let owners: Vec<&str> = PARTIAL_OBLIGATIONS
            .iter()
            .filter(|o| o.capability == name)
            .map(|o| o.dimension)
            .collect();
        assert_eq!(
            owners.len(),
            1,
            "{name} is declared with no testable surface, so exactly one dimension must \
             carry the obligation; found {owners:?}"
        );
    }
    for o in PARTIAL_OBLIGATIONS {
        assert!(
            CAPABILITIES_WITHOUT_TRAIT_SURFACE
                .iter()
                .any(|c| c.capability == o.capability),
            "{} reports an obligation against `{}`, which is not listed as lacking a testable \
             surface — either the list is stale or the note is invented",
            o.dimension,
            o.capability
        );
    }

    // Both availability arms, so no claim can hide in the one this run does not
    // take: without a live tier the `Ok` wording is never printed, and a guard
    // over the printed text alone would never see it.
    for availability in [
        Ok(()),
        Err("probe: availability held fixed for this assertion".to_string()),
    ] {
        check_report_claims(&availability, scope_text(availability.clone()));
    }
}

/// `availability` is the *input* the report was rendered from, so the printed
/// availability line is checked against it rather than against itself.
///
/// The line used to be unchecked: nothing in this function read it, so the two
/// arms were interchangeable — swapping the `Err` wording for the `Ok` wording, and
/// swapping back, each left all 28 tests green. That is not a missing test, it is a
/// guard asserting something other than what it claimed to hold.
fn check_report_claims(availability: &Result<(), String>, report: String) {
    // Spelled out here rather than shared with `scope_text`, so rewording the
    // report turns this red instead of quietly rewriting the check with it. The
    // residual is stated rather than hidden: wording both sides together stays
    // possible, so what is asserted is the pairing and the mutual exclusion of the
    // two arms, not the exact sentence.
    let (expected_line, forbidden) = match availability {
        Ok(()) => (
            String::from(
                "live 层前置条件齐备；下面列出的维度，其 live 测试只有在本次真实数据库运行中才成立。",
            ),
            "live 层未启用",
        ),
        Err(reason) => (
            format!("live 层未启用：{reason}"),
            "live 层前置条件齐备",
        ),
    };
    // Compared line for line, not with `contains`: a substring test passes on a
    // prefix, so it cannot see where the sentence was supposed to end — dropping
    // the trailing "成立。" from the report leaves it green, which is the same
    // defect one level down.
    assert!(
        report.lines().any(|line| line == expected_line),
        "availability() returned {availability:?} and the report must print exactly that \
         availability line, as a whole line. Expected a line reading:\n{expected_line}\n\
         Report reads:\n{report}"
    );
    assert!(
        !report.contains(forbidden),
        "availability() returned {availability:?} yet the report still carries 「{forbidden}」: \
         a run can then claim the live tier is ready when availability() said otherwise, or \
         hide a missing live tier behind ready-looking wording. Report reads:\n{report}"
    );

    for o in PARTIAL_OBLIGATIONS {
        assert!(
            REQUIRED_DIMENSIONS
                .iter()
                .any(|(id, _, _)| *id == o.dimension),
            "{} carries a partial obligation but is not a required dimension — the note would \
             never be reachable from the report",
            o.dimension
        );
        assert!(
            report.contains(&format!("  - {} 已验证", o.dimension)),
            "{} owes a half-honest note and must appear in the unverified report; the report \
             currently reads:\n{report}",
            o.dimension
        );
        assert!(
            !o.asserted.is_empty() && !o.missing.is_empty() && !o.reason.is_empty(),
            "{} must name what was verified, what was not, and why — an empty field would let \
             the note read as a complete pass",
            o.dimension
        );
    }

    // The passed claim is line-shaped: find every line that asserts something
    // completed, and require that none of them sweep in a half-asserted dimension.
    //
    // Matched on the bare idea of a completion claim — 「通过」 / "passed" — rather
    // than on the two phrases this file happened to use. The narrow version of
    // this check was a guard in name only: appending 「已实际执行并通过，CM-26 亦然」 to
    // the free-tier line left the suite green, because the filter looked for
    // 「已通过」 and 「已真实执行并通过」 and the line said 「已实际执行并通过」. A
    // reworded sentence must not be able to smuggle a dimension back into the
    // passed column.
    for (line_no, line) in report.lines().enumerate() {
        if !(line.contains("通过") || line.contains("passed")) {
            continue;
        }
        for o in PARTIAL_OBLIGATIONS {
            assert!(
                !line.contains(o.dimension),
                "line {} claims completion — '{line}' — but {}{} is only half asserted, and a \
                 reworded claim must not be able to put it back in the passed column",
                line_no + 1,
                crate::CONTRACT.label,
                o.dimension
            );
        }
    }

    // The generic line that used to over-claim: "a dimension missing from the
    // output really executed and passed" is only true for a *fully* asserted one.
    assert!(
        !report.contains("说明其 live 测试已真实执行并通过"),
        "the report still claims that any dimension absent from the output has really executed \
         and passed — false for every row in PARTIAL_OBLIGATIONS"
    );
}

/// Strict mode is armed by exactly one string, and by nothing else.
///
/// The failure this guards against is a variable that is merely *present* — an
/// empty value, a path, someone's `DATAZEN_CONTRACT_REQUIRE_LIVE=/opt/fixtures` —
/// silently arming a mode whose whole job is to turn skips into failures. That
/// would make CI flaky in a way nobody could reproduce, which is worse than the
/// leniency it replaced.
#[test]
fn strict_mode_is_armed_by_exactly_one_string() {
    for lenient in [
        None,
        Some(String::new()),
        Some("   ".to_string()),
        Some("0".to_string()),
        Some("true".to_string()),
        Some("yes".to_string()),
        Some("01".to_string()),
        Some("/opt/fixtures".to_string()),
        Some("11".to_string()),
    ] {
        assert!(
            !strict_live_requested(lenient.clone()),
            "{lenient:?} must not arm strict mode — only the exact value \"1\" does"
        );
    }
    for armed in ["1", " 1 ", "\t1\n"] {
        assert!(
            strict_live_requested(Some(armed.to_string())),
            "{armed:?} is the documented way to demand the live tier and must arm it"
        );
    }
}

/// The default is lenient, and the opt-in is not — proven by *running* both paths
/// rather than by reading them.
///
/// `catch_unwind` is used instead of setting the variable because the env is
/// process-global: mutating it would race every other test in this binary that
/// runs on its own thread, and the resulting flake would be indistinguishable
/// from a real failure.
#[test]
fn an_unverifiable_dimension_skips_by_default_and_fails_only_when_asked() {
    let lenient = std::panic::catch_unwind(|| unverified_or_fail("CM-08", "no fixture", false));
    assert!(
        lenient.is_ok(),
        "the default must stay lenient or every machine without fixtures fails the whole suite"
    );

    let strict = std::panic::catch_unwind(|| unverified_or_fail("CM-08", "no fixture", true));
    let panic = strict.expect_err(
        "asking to prove the live tier must make an unverifiable dimension a failure, otherwise \
         the request is another thing that gets ignored and green keeps meaning 'nothing \
         objected'",
    );
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains("CM-08") && message.contains("no fixture"),
        "the failure must name the dimension and the reason, or it is not actionable: {message}"
    );
}

/// The strict path is only reachable if `open_live` actually passes the real
/// value through — this pins the wiring, not just the helper.
///
/// Without this, flipping the default to lenient inside `open_live` (or dropping
/// the argument at one of its three skip sites) would leave every test above
/// still green, because they all call `unverified_or_fail` directly.
#[test]
fn every_skip_site_in_open_live_goes_through_the_strict_aware_choke_point() {
    let source = template_sources()
        .into_iter()
        .find(|path| path.ends_with("real_driver_contract_plumbing.rs"))
        .unwrap_or_else(|| {
            panic!(
                "the template scan did not yield real_driver_contract_plumbing.rs, so this test \
                 cannot check the wiring it is here to check"
            )
        })
        .canonicalize()
        .and_then(std::fs::read_to_string)
        .expect("the template source must be readable");

    let body = source
        .split("pub async fn open_live")
        .nth(1)
        .expect("open_live must exist in the plumbing template")
        .split("\n}")
        .next()
        .unwrap_or_default();

    assert!(
        !body.contains("report_unverified("),
        "open_live calls report_unverified directly, so a skip bypasses strict mode: {body}"
    );
    assert_eq!(
        body.matches("unverified_or_fail(").count(),
        3,
        "open_live has three ways to be unable to verify a dimension (unavailable driver, \
         no profile, unreachable target) and each must route through the strict-aware \
         choke point, or one of them keeps skipping silently"
    );
    assert!(
        body.contains("let strict = strict_live();"),
        "open_live must read the flag once per call, otherwise the three sites can disagree"
    );
}
