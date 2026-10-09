//! 源码结构扫描的**通用基元** —— 唯一计数铁律闸门（`single_counter_audit`）的词法底座。
//!
//! 从审计模块拆出来只因为单文件规模纪律；这里**没有任何隧道语义**：
//! 判据、常量、登记表都住在 `single_counter_audit/`。分成两个模块还有一个好处：
//! 基元与规则的边界是可见的 —— 基元可以被别的铁律闸门复用，而规则不会假装自己是类型系统。
//!
//! 所有函数都只吃**已抹串**的文本（见 [`blank`]）：注释与字面量里的形状不是代码，
//! 把两者混在一起扫，闸门会被自己的文档点红。

/// 抹掉注释与字符串 / 字符字面量，**保留下标与换行**（报告的行号仍可对应原文）。
///
/// 处理 `//`、`/* … */`、`"…"`（含 `b"…"` 前缀）、`r"…"` / `r#"…"#`、`'c'` / `'\n'` / `b'"'`。
/// 字符字面量**必须**处理：扫描器自己的源码里满是 `b'"'`，那里面的 `"` 一旦被当成
/// 字符串起点，就会一路吞到下一个引号、把中间的**真代码**抹掉 —— 闸门于是安静地少扫一片。
pub(super) fn blank(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        // 行注释
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        // 块注释（含 `/*` 开头的文档块）
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            out.push_str("  ");
            while i < b.len() {
                if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    i += 2;
                    out.push_str("  ");
                    break;
                }
                out.push(if b[i] == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }
        // 字符 / 字节字面量整体抹成等长空白。
        if let Some(len) = char_literal_len(b, i) {
            for _ in 0..len {
                out.push(' ');
            }
            i += len;
            continue;
        }
        // 普通字符串 / 字节串字面量
        //
        // **左右引号一起抹掉**，与上面 `char_literal_len` 的处理一致。
        //
        // 过去左引号被吃掉却不补位（只给 `b` 补了一格），右引号却留着：于是
        // ① 每个字面量整体**短一个字节**，「保留下标」这句承诺当场失效；
        // ② 输出里留下一个**孤立的闭引号**，再 blank 一趟时它被当成新字符串的开头，
        //    一路吃到下一个引号 —— 把中间的**真代码**整片抹掉。实测把
        //    `blank(blank(x))` 喂进 `impl_blocks` 会让 impl 块从 1 个变成 0 个：
        //    不报错、不 panic，直接「什么都没扫到」。
        //
        // 引号对任何判据都没有用（规则只认 `Mutex<usize>` / `+= 1` / `self.` 这类记号），
        // 抹干净还顺带让 blank **幂等**：输出里再没有引号，第二趟无事可做。
        if b[i] == b'"' || (b[i] == b'b' && b.get(i + 1) == Some(&b'"')) {
            let is_byte = b[i] == b'b';
            out.push_str(if is_byte { "  " } else { " " });
            i += usize::from(is_byte) + 1; // 吃掉 `b` 与左引号
            while i < b.len() {
                if b[i] == b'\\' {
                    // `\` + 真换行是续行，必须把换行留住，否则行号会整片错位。
                    if b.get(i + 1) == Some(&b'\n') {
                        out.push_str(" \n");
                    } else {
                        out.push_str("  ");
                    }
                    i += 2;
                    continue;
                }
                if b[i] == b'"' {
                    i += 1;
                    out.push(' ');
                    break;
                }
                out.push(if b[i] == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }
        // 裸字符串 r"…" / r#"…"#
        if b[i] == b'r' && matches!(b.get(i + 1), Some(b'"') | Some(b'#')) {
            let mut j = i + 1;
            let mut hashes = 0usize;
            while b.get(j) == Some(&b'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) == Some(&b'"') {
                let close: String = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                // 同样把 `r"` 与收尾定界符一起抹成等长空白（理由见下面字符串分支的注释）。
                out.push_str("  ");
                i = j + 1;
                while i < b.len() {
                    if b[i] == b'"' && text[i..].starts_with(&close) {
                        i += close.len();
                        out.push_str(&" ".repeat(close.len()));
                        break;
                    }
                    out.push(if b[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
                continue;
            }
        }
        // 非 ASCII 字节一律落成空格（**一个字节 → 一个空格**，长度与行号守得住）。
        //
        // 这里曾是 `out.push(b[i] as char)`：把 UTF-8 的字节当 Latin-1 字符回推，长度是守住了，
        // 可输出仍是多字节字符，于是下游每一个按字节推进、再做 `text[i..]` 切片的扫描器
        // （`impl_blocks` / `functions` / …）都可能在字符中间切片而 panic
        // （实测 `byte index 16669 is not a char boundary; inside 'ï'`）。
        // 闸门里没有任何判据依赖非 ASCII 文本，Rust 关键字与类型名也全是 ASCII，
        // 所以在**入口**抹成空格即可一次性让整条扫描链安全，而不是逐个函数打补丁。
        out.push(if b[i] >= 0x80 { ' ' } else { b[i] as char });
        i += 1;
    }
    out
}

/// `'c'`(3) / `'\n'`(4) / `b'c'`(4) / `b'\n'`(5) 的长度；不是字面量则 `None`。
///
/// 生命周期标注（`'a`、`'static`、`&'static str`）在合法窗口内没有收尾引号，
/// 因此返回 `None` 并**原样保留** —— 它们是类型的一部分，抹掉会把代码扫歪。
pub(super) fn char_literal_len(b: &[u8], at: usize) -> Option<usize> {
    if b.get(at) == Some(&b'b') {
        return char_literal_len(b, at + 1).map(|inner| inner + 1);
    }
    if b.get(at) != Some(&b'\'') {
        return None;
    }
    if b.get(at + 1) == Some(&b'\\') && b.get(at + 3) == Some(&b'\'') {
        return Some(4);
    }
    if b.get(at + 2) == Some(&b'\'') {
        return Some(3);
    }
    None
}

pub(super) fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `text[at]` 处的 `(` / `[` / `{` 的**配对闭括号之后**的下标。
pub(super) fn match_open(text: &str, at: usize) -> Option<usize> {
    let b = text.as_bytes();
    let (open, close): (u8, u8) = match *b.get(at)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0usize;
    let mut i = at;
    while i < b.len() {
        if b[i] == open {
            depth += 1;
        } else if b[i] == close {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
        i += 1;
    }
    None
}

/// 一个 `fn` 的位置信息（所有下标均落在**抹串后**的文本里）。
#[derive(Debug, Clone)]
pub(super) struct Func {
    pub name: String,
    /// 形参表正文（不含左右括号）。
    pub params: String,
    /// 返回类型文本；没有 `->` 时为空。
    pub returns: String,
    /// 函数体范围（含花括号）。
    pub body: (usize, usize),
    /// `fn` 关键字的下标，用于定位「函数属于哪个 impl」。
    pub at: usize,
}

/// 抓出一个片段里的所有 `fn`（含 `async fn` / `pub(crate) fn` / 嵌套 `fn`）。
pub(super) fn functions(text: &str) -> Vec<Func> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 3 <= b.len() {
        let is_kw = text[i..].starts_with("fn ");
        let boundary = i == 0 || !is_ident_byte(b[i - 1]);
        let not_method_call = !text[..i].trim_end().ends_with('.');
        if is_kw && boundary && not_method_call {
            if let Some(f) = parse_fn(text, i + 3, i) {
                // 跳过整个函数体：嵌套 `fn` 由下一次扫描自然命中，这里不重复推进。
                out.push(f.clone());
                i = f.body.1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// 从 `fn` 关键字后第一个字符（名字起点）解析一个函数。
fn parse_fn(text: &str, name_at: usize, at: usize) -> Option<Func> {
    let b = text.as_bytes();
    let mut j = name_at;
    while j < b.len() && (is_ident_byte(b[j]) || b[j] == b'<' || b[j] == b'>') {
        j += 1;
    }
    let name = text[name_at..j]
        .split('<')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if name.is_empty() {
        return None;
    }
    let paren = j + text[j..].find('(')?;
    let paren_end = match_open(text, paren)?;
    let params = text[paren + 1..paren_end - 1].to_owned();
    let brace_rel = text[paren_end..].find('{')?;
    let head = &text[paren_end..paren_end + brace_rel];
    let returns = match head.find("->") {
        Some(arrow) => head[arrow + 2..].trim().to_owned(),
        None => String::new(),
    };
    let body_start = paren_end + brace_rel;
    let body_end = match_open(text, body_start)?;
    Some(Func {
        name,
        params,
        returns,
        body: (body_start, body_end),
        at,
    })
}

/// 一个 `impl Trait for Ty { … }` / `impl Ty { … }` 块。
#[derive(Debug, Clone)]
pub(super) struct ImplBlock {
    /// `for` 左边的 trait 名；无 trait 的固有 impl 为空串。
    pub trait_name: String,
    /// 被实现的类型名（已去泛型与路径前缀）。
    pub type_name: String,
    pub body: (usize, usize),
}

pub(super) fn impl_blocks(text: &str) -> Vec<ImplBlock> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 < b.len() {
        let is_kw = text[i..].starts_with("impl");
        let boundary = i == 0 || !is_ident_byte(b[i - 1]);
        let not_impling = !is_ident_byte(b[i + 4]);
        if is_kw && boundary && not_impling {
            if let Some(brace_rel) = text[i..].find('{') {
                let body_start = i + brace_rel;
                if let Some(body_end) = match_open(text, body_start) {
                    let head = text[i..body_start].trim_end();
                    let head = head.strip_prefix("unsafe ").unwrap_or(head);
                    let (trait_part, type_part) = match head.find(" for ") {
                        Some(at) => (&head[4..at], &head[at + 5..]),
                        None => ("", head[4..].trim_start()),
                    };
                    // 去泛型、去路径前缀：`Vec<P<T>>` / `crate::x::Ty` 都只取末段裸名。
                    let bare = |s: &str| -> String {
                        let s = s
                            .trim()
                            .split('<')
                            .next()
                            .unwrap_or_default()
                            .trim()
                            .to_owned();
                        s.rsplit("::").next().unwrap_or_default().to_owned()
                    };
                    out.push(ImplBlock {
                        trait_name: bare(trait_part),
                        type_name: bare(type_part),
                        body: (body_start, body_end),
                    });
                    i = body_end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// 把一段文本按**顶层**逗号拆开（嵌套括号里的逗号不算分隔）。
pub(super) fn split_top_level(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in b.iter().enumerate() {
        match c {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                let piece = text[start..i].trim();
                if !piece.is_empty() {
                    out.push(piece.to_owned());
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_owned());
    }
    out
}

pub(super) fn strip_space(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `Name` 的**本地**结构体 / 元组结构体 / 类型别名右端；`None` = 本文件里没有这个本地类型。
///
/// 返回「字段清单」文本（`type Name = X;` 返回 `X`；单元结构体返回空串）。
/// 这里下标算错一个字符，R4 就会静默漏抓，所以刻意不用索引相乘那类聪明写法。
pub(super) fn local_decl(text: &str, name: &str) -> Option<String> {
    let b = text.as_bytes();
    let needle = format!("struct {name}");
    if let Some(at) = text.find(&needle) {
        // 名字后面不能再有标识符字符（`struct Port` 不许命中 `struct Portal`）。
        let after = at + needle.len();
        if b.get(after).is_some_and(|c| is_ident_byte(*c)) {
            return None;
        }
        let rest = &text[after..];
        let mut picks = Vec::new();
        for opener in ['{', '(', ';'] {
            if let Some(p) = rest.find(opener) {
                picks.push((p, opener));
            }
        }
        let (rel, opener) = picks.into_iter().min_by_key(|(p, _)| *p)?;
        let abs = after + rel;
        if opener == ';' {
            return Some(String::new());
        }
        let inner_end = match_open(text, abs)?.checked_sub(1)?;
        if inner_end <= abs {
            return Some(String::new());
        }
        return Some(text[abs + 1..inner_end].to_owned());
    }
    let alias = format!("type {name}");
    if let Some(at) = text.find(&alias) {
        let after = at + alias.len();
        if !b.get(after).is_some_and(|c| c.is_ascii_whitespace()) {
            return None;
        }
        let rest = &text[after..];
        let start = match rest.find('=') {
            Some(eq) => eq + 1,
            None => 0,
        };
        let end = rest[start..].find(';')?;
        return Some(rest[start..start + end].trim().to_owned());
    }
    None
}

/// `Name` 结构体的字段 `(名字, 类型)` 表。
///
/// 解析不出（没有这个结构体 / 括号不配对）⇒ 空表：调用方按「无字段」处理，
/// 而 R4 的正向用例另有「实现数量必须恰好 3」的断言兜住「扫不到东西」。
pub(super) fn struct_fields(text: &str, name: &str) -> Vec<(String, String)> {
    fields_of(text, name).unwrap_or_default()
}

fn fields_of(text: &str, name: &str) -> Option<Vec<(String, String)>> {
    let b = text.as_bytes();
    let needle = format!("struct {name}");
    let at = text.find(&needle)? + needle.len();
    if b.get(at).is_some_and(|c| is_ident_byte(*c)) {
        return Some(Vec::new());
    }
    let brace = at + text[at..].find('{')?;
    let end = match_open(text, brace)?.checked_sub(1)?;
    Some(
        split_top_level(&text[brace + 1..end])
            .into_iter()
            .filter_map(|field| {
                let field = field.trim().trim_end_matches(',');
                let (f_name, f_ty) = field.split_once(':')?;
                Some((f_name.trim().to_owned(), f_ty.trim().to_owned()))
            })
            .collect(),
    )
}

/// 函数所在的最内层 impl 块（`"Ty"` 或 `"Trait for Ty"`）。
pub(super) fn enclosing_impl(func: &Func, impls: &[ImplBlock]) -> Option<String> {
    impls
        .iter()
        .filter(|blk| blk.body.0 < func.at && func.at < blk.body.1)
        .min_by_key(|blk| blk.body.1 - blk.body.0)
        .map(|blk| {
            if blk.trait_name.is_empty() {
                blk.type_name.clone()
            } else {
                format!("{} for {}", blk.trait_name, blk.type_name)
            }
        })
}

/// `needle` 在 `text` 里的行号（1 基），供 kill test 与报告自证「扫描确实看见了东西」。
pub(super) fn line_of(text: &str, needle: &str) -> Option<usize> {
    text.find(needle)
        .map(|at| 1 + text[..at].matches('\n').count())
}
