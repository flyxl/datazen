//! CM-70-FU2 的机器闸门：panic 文本不得插值凭据。
//!
//! 为什么要有这道闸门：`cm70_no_disk.rs` / `cm70/forgery.rs` 里的断言每一条都在检查
//! 「凭据别漏进输出面」，可它们**自己的** panic 文本也是输出面。一次断言失败会把
//! panic 文本写进 CI 日志，于是「检测泄漏的那条断言」自己成了泄漏路径。人眼看不住的
//! 是下一个人顺手补一句 `"……：{token}"`，把洞重新开回来。
//!
//! ## 为什么扫「格式串结构」而不是「变量名出现」
//!
//! 本文件自己的源码里就写满了那些词（登记表、豁免理由都得提），而
//! `gateway_contract/invariants.rs` 里也合法地有一个叫 `token` 的变量——那是它扫描
//! 生产源码时**命中的模式名**，取自写死的标点数组，与凭据毫无关系。只按「某个文件里
//! 出现过这个词」去判，必然同时误伤这两处。
//!
//! 所以这里的判据是**结构**：
//!
//! 1. 只看 `assert!` / `assert_eq!` / `assert_ne!` / `panic!` 的调用范围；
//! 2. 只取这些范围内的两类插值点：(a) 字符串字面量里的 `{占位符}`，
//!    (b) 第一个字符串参数**之后**那些顶层实参的首标识符——`"……{}……", rendered`
//!    这种位置传参同样会插值，光扫花括号会被绕过；
//! 3. 拿这些**名字**去撞禁用词表；
//! 4. 同一文件里同名标识符若**全部**只绑定在字面量上，则按豁免放行，理由写在豁免表里；
//!    一旦它改绑到任何函数调用的结果上，豁免当场失效，闸门转红。
//!
//! 于是 `invariants.rs` 的 `token` 因绑定 `for token in [".unwrap(", …]` 而放行；
//! 而本文件自己虽然写满那些词，却一个都没出现在 panic 范围里，照样全绿——
//! **本文件也在扫描登记表里，这条自证不是空话**。
//!
//! 本测试只报「哪个文件第几行出现了禁用占位符」，**绝不回显匹配到的源码文本**——
//! 回显匹配内容等于把要防的东西再抄一遍。

/// 会被当成 panic 出面的宏。
const MACROS: &[&str] = &["assert", "assert_eq", "assert_ne", "panic"];

/// 禁用占位符名。
///
/// 这一组是「值本身可能就是凭据」的：令牌、它两段凭据，以及**正被怀疑**装了凭据的
/// 渲染文本（`rendered` 之所以要禁，恰恰因为断言此刻无法确认它干不干净）。后两个是
/// 「被查对象本身」：语义指纹、目录新增条目——报错时把它们整个打出来，等于在排查
/// 文本的断言上再开一个输出面。
///
/// **故意不在表里的**：`error` / `err` / `other` / `forged` / `unreadable` / `fenced`
/// / `conflict` / `message` 这一族都是 `GatewayError` 或它的字段，值来自构造点，
/// 已逐字段核对为编译期字面量与指纹；它们在构造点带不出凭据，源级扫描也没法区分
/// 「一个叫 `forged` 的 `GatewayError`」和「一个叫 `forged` 的伪造令牌」，按「值本身
/// 是不是凭据」统一裁定：`forgery.rs` 里真正的令牌变量叫 `parts` / `hex`，那两个字
/// 仍在表内，缺陷类别照样拦得住。
const FORBIDDEN: &[&str] = &[
    "token", "rendered", "segment", "parts", "hex", "mac", "genuine", "stale", "incoming", "added",
];

/// 扫描范围。**本文件自己也在里面**（见文件头末段的自证）。
const SOURCES: &[(&str, &str)] = &[
    ("tests/cm70_no_disk.rs", include_str!("cm70_no_disk.rs")),
    (
        "tests/cm70_no_disk/error_projection.rs",
        include_str!("cm70_no_disk/error_projection.rs"),
    ),
    ("tests/cm70/expiry.rs", include_str!("cm70/expiry.rs")),
    ("tests/cm70/forgery.rs", include_str!("cm70/forgery.rs")),
    (
        "tests/cm70/owner_restart.rs",
        include_str!("cm70/owner_restart.rs"),
    ),
    ("tests/cm70/redaction.rs", include_str!("cm70/redaction.rs")),
    ("tests/cm70/retention.rs", include_str!("cm70/retention.rs")),
    ("tests/cm70/retries.rs", include_str!("cm70/retries.rs")),
    (
        "tests/gateway_contract/acceptance.rs",
        include_str!("gateway_contract/acceptance.rs"),
    ),
    (
        "tests/gateway_contract/cancel.rs",
        include_str!("gateway_contract/cancel.rs"),
    ),
    (
        "tests/gateway_contract/events.rs",
        include_str!("gateway_contract/events.rs"),
    ),
    (
        "tests/gateway_contract/idempotency.rs",
        include_str!("gateway_contract/idempotency.rs"),
    ),
    (
        "tests/gateway_contract/invariants.rs",
        include_str!("gateway_contract/invariants.rs"),
    ),
    (
        "tests/gateway_contract/provenance.rs",
        include_str!("gateway_contract/provenance.rs"),
    ),
    (
        "tests/gateway_contract/revision.rs",
        include_str!("gateway_contract/revision.rs"),
    ),
    (
        "tests/gateway_contract/timing.rs",
        include_str!("gateway_contract/timing.rs"),
    ),
    (
        "tests/gateway_fixtures/mod.rs",
        include_str!("gateway_fixtures/mod.rs"),
    ),
    (
        "tests/cm70_panic_redaction_guard.rs",
        include_str!("cm70_panic_redaction_guard.rs"),
    ),
];

/// 豁免：(文件标签, 名字, 为什么放行)。
///
/// 豁免**不是**白名单里的一行字：它只在同名标识符于该文件里全部绑定字面量时成立。
/// 谁把这个标识符改成绑定某个调用的结果，豁免立刻失效、闸门转红。
const EXEMPTIONS: &[(&str, &str, &str)] = &[(
    "tests/gateway_contract/invariants.rs",
    "token",
    "它是扫描生产源码时命中的**模式名**，绑在写死的标点数组上（for token in [\".unwrap(\", …]），不是凭据",
)];

/// `bytes` 里该下标起是否是一段普通或裸字符串字面量。
fn literal_end(text: &str, at: usize) -> Option<usize> {
    let b = text.as_bytes();
    if b[at] == b'"' {
        let mut i = at + 1;
        while i < b.len() {
            match b[i] {
                b'\\' => i += 2,
                b'"' => return Some(i + 1),
                _ => i += 1,
            }
        }
        return None;
    }
    // 裸串：`r"…"` / `r#"…"#`
    let mut i = at + 1;
    if i >= b.len() || b[i] != b'r' {
        return None;
    }
    i += 1;
    let hash_start = i;
    while i < b.len() && b[i] == b'#' {
        i += 1;
    }
    if i >= b.len() || b[i] != b'"' {
        return None;
    }
    let hashes = b.len() - hash_start;
    let close: Vec<u8> = std::iter::once(b'"')
        .chain(std::iter::repeat(b'#').take(hashes))
        .collect();
    let tail = &b[i + 1..];
    let mut j = 0;
    while j + close.len() <= tail.len() {
        if tail[j..j + close.len()] == close[..] {
            return Some(i + 1 + j + close.len());
        }
        j += 1;
    }
    None
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

/// `open` 处的 `(` 前面是不是一个被关注的宏名（`panic!` 的 `!` 一并跳过）。
fn macro_before(text: &str, open: usize) -> bool {
    let b = text.as_bytes();
    let mut start = open;
    if start > 0 && b[start - 1] == b'!' {
        start -= 1;
    }
    let mut name_start = start;
    while name_start > 0 && is_ident(b[name_start - 1]) {
        name_start -= 1;
    }
    MACROS.contains(&&text[name_start..start])
}

/// 一段文本里有几个换行。
fn newlines(s: &str) -> usize {
    s.bytes().filter(|b| *b == b'\n').count()
}

/// 从一段源码里抓出 `assert!` / `panic!` 调用范围内的插值名。
///
/// 返回 `(行号, 名字)`。**只给位置，不给被匹配的文本**——这是本测试的自我约束。
fn interpolations(text: &str) -> Vec<(usize, String)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' if macro_before(text, i) => {
                let (end, first_arg) = scan_call(text, i, line, &mut out);
                // 第一个字符串参数之后的顶层实参也算插值点：位置传参同样会进 panic 文本。
                for (start, arg_line) in &first_arg.1 {
                    if *start <= first_arg.0 {
                        continue;
                    }
                    let tail = &text[*start..end];
                    if let Some(name) = leading_name(tail) {
                        out.push((*arg_line, name));
                    }
                }
                line += newlines(&text[i..end]);
                i = end;
            }
            b'"' | b'r' => match literal_end(text, i) {
                Some(end) => {
                    i = end;
                }
                None => i += 1,
            },
            _ => i += 1,
        }
    }
    out
}

/// 从 `open`（宏名后的 `(`）扫到配对的 `)`。
///
/// 深度以**宏自己的括号内部**为 0：只有这一层的 `,` 才是实参分隔符，只有这一层的 `)`
/// 才是宏的收尾，内层 `fields.len()` 之类不得把它们偷走。
///
/// 返回 `(右括号之后的下标, (第一个字符串实参的起点, 其后各顶层实参的 (起点, 行号)))`。
fn scan_call(
    text: &str,
    open: usize,
    start_line: usize,
    out: &mut Vec<(usize, String)>,
) -> (usize, (usize, Vec<(usize, usize)>)) {
    let b = text.as_bytes();
    let mut i = open + 1;
    let mut depth = 0usize;
    let mut line = start_line;
    let mut first_string = usize::MAX;
    let mut args: Vec<(usize, usize)> = Vec::new();
    let mut arg_start = open + 1;
    let mut arg_line = start_line;
    let mut arg_is_string = false;
    let mut seen_non_space = false;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let close = text[i + 2..].find("*/").map(|p| i + 2 + p + 2);
                match close {
                    Some(end) => {
                        line += newlines(&text[i..end]);
                        i = end;
                    }
                    None => {
                        i = b.len();
                    }
                }
            }
            b'"' | b'r' => match literal_end(text, i) {
                Some(end) => {
                    if !seen_non_space {
                        arg_is_string = true;
                        if first_string == usize::MAX {
                            first_string = arg_start;
                        }
                    }
                    placeholders_in(&text[i..end], line, out);
                    line += newlines(&text[i..end]);
                    seen_non_space = true;
                    i = end;
                }
                None => i += 1,
            },
            b'(' | b'[' | b'{' => {
                depth += 1;
                seen_non_space = true;
                i += 1;
            }
            b')' | b']' | b'}' => {
                if depth == 0 && b[i] == b')' {
                    args.push((arg_start, arg_line));
                    return (i + 1, (first_string, args));
                }
                depth = depth.saturating_sub(1);
                seen_non_space = true;
                i += 1;
            }
            b',' if depth == 0 => {
                if arg_is_string || seen_non_space {
                    args.push((arg_start, arg_line));
                }
                arg_start = i + 1;
                arg_line = line;
                arg_is_string = false;
                seen_non_space = false;
                i += 1;
            }
            _ => {
                if !b[i].is_ascii_whitespace() {
                    seen_non_space = true;
                }
                i += 1;
            }
        }
    }
    (b.len(), (first_string, args))
}

/// 一段字符串字面量里的 `{name}` / `{name:?}` / `{name:>8}` 里的 `name`。
fn placeholders_in(literal: &str, start_line: usize, out: &mut Vec<(usize, String)>) {
    let body = match (literal.find('"'), literal.rfind('"')) {
        (Some(a), Some(z)) if z > a => &literal[a + 1..z],
        _ => return,
    };
    let b = body.as_bytes();
    let mut i = 0;
    let mut line = start_line;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'{' if b.get(i + 1) == Some(&b'{') => i += 2,
            b'{' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && is_ident(b[j]) {
                    j += 1;
                }
                if j > start && is_ident_start(b[start]) {
                    out.push((line, body[start..j].to_owned()));
                }
                i = j;
            }
            _ => i += 1,
        }
    }
}

/// 一段实参文本的首标识符（`rendered` / `rendered.trim()` 都算 `rendered`）。
///
/// 字符串字面量开头的实参没有标识符，返回 `None`。
fn leading_name(arg: &str) -> Option<String> {
    let trimmed = arg.trim_start();
    let b = trimmed.as_bytes();
    if b.is_empty() || !is_ident_start(b[0]) {
        return None;
    }
    let mut j = 0;
    while j < b.len() && is_ident(b[j]) {
        j += 1;
    }
    Some(trimmed[..j].to_owned())
}

/// 该标识符在 `text` 里的每一次 `let` / `for … in` 绑定是不是都只来自字面量。
///
/// 返回 `false` 有两种含义：它绑过非字面量（豁免作废），或者压根没绑过（豁免已过期）。
fn bound_to_literals_only(text: &str, name: &str) -> bool {
    let mut found = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let init = if let Some(rest) = trimmed.strip_prefix("for ") {
            match rest.trim_start().strip_prefix(name) {
                Some(r) => match r.trim_start().strip_prefix("in ") {
                    Some(x) => x,
                    None => continue,
                },
                None => continue,
            }
        } else if let Some(rest) = trimmed.strip_prefix("let ") {
            match rest.trim_start().strip_prefix(name) {
                Some(r) => match r.find('=') {
                    Some(p) => &r[p + 1..],
                    None => continue,
                },
                None => continue,
            }
        } else {
            continue;
        };
        found = true;
        let bare = strip_literals(init);
        if bare.contains('(') || bare.contains('.') {
            return false;
        }
    }
    found
}

/// 把字符串字面量抹成等长空白，判「有没有调用」时不被字面量里的括号骗。
fn strip_literals(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' {
            out.push('"');
            out.push('"');
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

fn exempted(file: &str, name: &str, text: &str) -> bool {
    EXEMPTIONS
        .iter()
        .any(|(f, n, _)| *f == file && *n == name && bound_to_literals_only(text, name))
}

/// 主闸门。
#[test]
fn no_panic_message_interpolates_credential_material() {
    let mut violations: Vec<String> = Vec::new();
    for (file, text) in SOURCES {
        for (line, name) in interpolations(text) {
            if !FORBIDDEN.contains(&name.as_str()) {
                continue;
            }
            if exempted(file, &name, text) {
                continue;
            }
            violations.push(format!("{file}:{line} 的 panic 文本插值了禁用名 {name}"));
        }
    }
    assert!(
        violations.is_empty(),
        "凭据插值点：\n{}",
        violations.join("\n")
    );
}

/// 登记表本身不许被悄悄摘掉一个文件——否则上面那条会安静地少扫一片。
#[test]
fn the_scan_registry_covers_every_audited_file() {
    let expected = [
        "tests/cm70_no_disk.rs",
        "tests/cm70_no_disk/error_projection.rs",
        "tests/cm70/expiry.rs",
        "tests/cm70/forgery.rs",
        "tests/cm70/owner_restart.rs",
        "tests/cm70/redaction.rs",
        "tests/cm70/retention.rs",
        "tests/cm70/retries.rs",
        "tests/gateway_contract/acceptance.rs",
        "tests/gateway_contract/cancel.rs",
        "tests/gateway_contract/events.rs",
        "tests/gateway_contract/idempotency.rs",
        "tests/gateway_contract/invariants.rs",
        "tests/gateway_contract/provenance.rs",
        "tests/gateway_contract/revision.rs",
        "tests/gateway_contract/timing.rs",
        "tests/gateway_fixtures/mod.rs",
        "tests/cm70_panic_redaction_guard.rs",
    ];
    for label in expected {
        assert!(
            SOURCES.iter().any(|(f, _)| *f == label),
            "扫描登记表漏了 {label}"
        );
    }
    assert_eq!(SOURCES.len(), expected.len(), "扫描登记表与预期文件数不符");
}

/// 豁免表不许养僵尸条目：理由还在、被豁的名字却已经没人用了，就要连理由一起删。
#[test]
fn every_exemption_is_still_load_bearing() {
    for (file, name, _reason) in EXEMPTIONS {
        let text = SOURCES
            .iter()
            .find(|(f, _)| f == file)
            .map(|(_, t)| *t)
            .unwrap_or_else(|| panic!("豁免指向未登记的文件 {file}"));
        let used = interpolations(text).into_iter().any(|(_, n)| n == *name);
        assert!(
            used,
            "豁免条目已过期：{file} 里不再有 {name} 的插值，理由请一并删掉"
        );
        assert!(
            bound_to_literals_only(text, name),
            "豁免理由不再成立：{file} 里的 {name} 已经绑到非字面量上"
        );
    }
}

/// 扫描器自证：它必须抓得住**真**违规。
///
/// 用一段源码喂它，里面是本测试真实判据下必须转红的写法（含位置传参这一条绕过路径），
/// 断言三处全被抓到。这条是护栏自己的护栏——它不跑，闸门就只是「看起来在扫」。
#[test]
fn the_scanner_catches_a_planted_violation() {
    let planted = "\
fn sample() {
    let token = issue();
    let rendered = format!(\"{:?}\", token);
    let parts = split(&token);
    assert!(!rendered.is_empty(), \"漏了：{token}\");
    assert!(true, \"漏了：{}\", rendered);
    assert!(true, \"漏了：{parts:?}\");
}
";
    let found: Vec<String> = interpolations(planted)
        .into_iter()
        .map(|(_, n)| n)
        .filter(|n| FORBIDDEN.contains(&n.as_str()))
        .collect();
    assert_eq!(found.len(), 3, "扫描器漏抓：{found:?}");
    assert!(found.contains(&"token".to_owned()));
    assert!(found.contains(&"rendered".to_owned()));
    assert!(found.contains(&"parts".to_owned()));
}
