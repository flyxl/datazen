//! 端口清单的**解析机器**：从源码、模块头与跨文档表格里抽出可比对的事实。
//!
//! 这个模块只服务 [`super::PORT_TRAITS`](super::PORT_TRAITS) 的四条单测，因此整段
//! `#[cfg(test)]`。拆出来是为了让清单文件本身只留「清单 + 断言」，解析细节各自成文件，
//! 双方都离 AGENTS.md 的单文件规模上限很远。
//!
//! 三类事实各有各的解析口径：
//!
//! * **代码**：`pub trait` 声明（要抹掉注释与字符串字面量，否则分不清声明与写法示例）；
//! * **模块头**：`//!` 文档注释里的构成说明（用来核对「清单里不在 §6.4 表里的那些」）；
//! * **文档**：[概要 §6.4](system-overview.md#64-repositories-与环境-ports) 的端口表。

use std::path::Path;

/// `src/ports/` 下的**全部**源文件与其内容（`include_str!` 读进来，不在运行时碰文件系统）。
///
/// 路径相对 `src/ports/`，顺序为路径字典序。**这份表必须覆盖磁盘上的每一个 `.rs`
/// 文件**——否则新文件里的 trait 会被静默漏掉；`every_port_source_file_is_covered_by_the
/// _include_list` 用一次目录扫描把这件事钉死。
pub(super) const SOURCE_FILES: &[(&str, &str)] = &[
    ("artifact.rs", include_str!("../artifact.rs")),
    ("budget/control.rs", include_str!("../budget/control.rs")),
    (
        "budget/coordinator.rs",
        include_str!("../budget/coordinator.rs"),
    ),
    (
        "budget/dimension.rs",
        include_str!("../budget/dimension.rs"),
    ),
    ("budget/mod.rs", include_str!("../budget/mod.rs")),
    ("budget/node.rs", include_str!("../budget/node.rs")),
    ("budget/pool.rs", include_str!("../budget/pool.rs")),
    (
        "budget/pool_ledger.rs",
        include_str!("../budget/pool_ledger.rs"),
    ),
    (
        "budget/pool_ledger/tests.rs",
        include_str!("../budget/pool_ledger/tests.rs"),
    ),
    ("clock.rs", include_str!("../clock.rs")),
    ("event.rs", include_str!("../event.rs")),
    (
        "execution_settings.rs",
        include_str!("../execution_settings.rs"),
    ),
    (
        "execution_target.rs",
        include_str!("../execution_target.rs"),
    ),
    ("identity.rs", include_str!("../identity.rs")),
    ("inventory.rs", include_str!("../inventory.rs")),
    (
        "inventory/parser.rs",
        include_str!("../inventory/parser.rs"),
    ),
    ("job.rs", include_str!("../job.rs")),
    ("known_hosts.rs", include_str!("../known_hosts.rs")),
    ("master_key.rs", include_str!("../master_key.rs")),
    (
        "migration_profile.rs",
        include_str!("../migration_profile.rs"),
    ),
    ("mod.rs", include_str!("../mod.rs")),
    ("monitor_grant.rs", include_str!("../monitor_grant.rs")),
    ("native_tool.rs", include_str!("../native_tool.rs")),
    ("network.rs", include_str!("../network.rs")),
    ("policy.rs", include_str!("../policy.rs")),
    ("profile.rs", include_str!("../profile.rs")),
    ("profile_reader.rs", include_str!("../profile_reader.rs")),
    ("scope.rs", include_str!("../scope.rs")),
    ("secret.rs", include_str!("../secret.rs")),
    (
        "session_directory.rs",
        include_str!("../session_directory.rs"),
    ),
    ("settings.rs", include_str!("../settings.rs")),
    ("token.rs", include_str!("../token.rs")),
    ("tunnel.rs", include_str!("../tunnel.rs")),
];

/// 端口模块头（`ports/mod.rs`）与 crate 头（`lib.rs`）。
pub(super) const MODULE_HEADER: &str = include_str!("../mod.rs");
pub(super) const CRATE_HEADER: &str = include_str!("../../lib.rs");

/// [概要 §6.4 repositories 与环境 ports](docs/architecture/platform/system-overview.md)
/// 的端口表——跨文档的那一半清单。
pub(super) const SYSTEM_OVERVIEW: &str =
    include_str!("../../../../../docs/architecture/platform/system-overview.md");

/// `ports/mod.rs` 模块头里「列举端口构成」那一段的锚点：**概要 §6.4 的链接**。
///
/// 端口构成那段话讲的就是「相对 §6.4 而言还差哪些」，所以它必然带这个链接；
/// 用它当锚点，段落被改写时会**明确报错**，而不是悄悄放过一份已经对不上的模块头。
pub(super) const SECTION_6_4_ANCHOR: &str = "system-overview.md#64-repositories-与环境-ports";

/// 标识符字符（ASCII 字母、数字、下划线）。
fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// 把源码里的**注释与字符串字面量**抹掉，只留代码形状。
///
/// 为什么需要它：`pub trait` 这个片段在端口源码里出现在三处——真正的 trait 声明、
/// `ports/mod.rs` 模块头里作为**写法示例**的文档注释（`` `#[async_trait] pub trait X` ``）、
/// 以及若干端口自测里用来定位声明的**字符串**（`.split("pub trait JobRepository")`）。
/// 只看代码形状，三者才分得开：文档注释与字符串都会被抹掉，声明留下。
///
/// 字符字面量 `'x'` 与生命周期 `'static` 在这里必须分开：前者闭合、后者不闭合。
fn code_shape(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            // 行注释（含 `///` 与 `//!`）：吃掉到行尾，但保留换行以免行号漂移。
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            // 块注释：支持嵌套（Rust 允许）。
            '/' if chars.get(i + 1) == Some(&'*') => {
                let mut depth = 0usize;
                while i < chars.len() {
                    if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                        depth += 1;
                        i += 2;
                    } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if chars[i] == '\n' {
                            out.push('\n');
                        }
                        i += 1;
                    }
                }
            }
            // 原始字符串：`r"..."` / `r#"..."#`。
            'r' => {
                let mut j = i + 1;
                let mut hashes = 0usize;
                while chars.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                let previous_is_ident = i > 0 && is_ident_char(chars[i - 1]);
                if chars.get(j) == Some(&'"') && !previous_is_ident {
                    j += 1;
                    while j < chars.len() {
                        if chars[j] == '"' {
                            let mut k = j + 1;
                            let mut seen = 0usize;
                            while seen < hashes && chars.get(k) == Some(&'#') {
                                k += 1;
                                seen += 1;
                            }
                            if seen == hashes {
                                j = k;
                                break;
                            }
                        }
                        if chars[j] == '\n' {
                            out.push('\n');
                        }
                        j += 1;
                    }
                    i = j;
                } else {
                    out.push('r');
                    i += 1;
                }
            }
            // 普通字符串字面量（含 `b"..."` 的字节串）：内容整体抹掉。
            '"' => {
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == '"' {
                        i += 1;
                        break;
                    }
                    if chars[i] == '\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
            }
            // `'` 可能是字符字面量，也可能是生命周期。
            '\'' => {
                let is_char_literal = match chars.get(i + 1).copied() {
                    Some('\\') => true,
                    Some(_) => chars.get(i + 2).copied() == Some('\''),
                    None => false,
                };
                if is_char_literal {
                    i += 1;
                    while i < chars.len() {
                        if chars[i] == '\\' {
                            i += 2;
                            continue;
                        }
                        if chars[i] == '\'' {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                } else {
                    out.push('\'');
                    i += 1;
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// 抽出源码里**全部** `pub trait <Name>` 声明的名字。
///
/// 找不到名字就**报错**（而不是跳过）：代码形状里剩下的 `pub trait` 必然是声明，
/// 它没有名字说明源码坏了或者解析器坏了，两种都必须让测试红。
pub(super) fn declared_traits(source: &str) -> Vec<String> {
    let code = code_shape(source);
    let mut names = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = code[cursor..].find("pub trait") {
        let start = cursor + offset;
        cursor = start + "pub trait".len();
        // `pub` 必须是独立 token：前面不能还连着标识符字符。
        if let Some(before) = code[..start].chars().next_back() {
            if is_ident_char(before) {
                continue;
            }
        }
        let rest = code[cursor..].trim_start();
        let name: String = rest.chars().take_while(|c| is_ident_char(*c)).collect();
        assert!(
            !name.is_empty(),
            "源码的代码形状里出现了没有名字的 `pub trait`（偏移 {start}）——\
             端口清单无法与代码核对，测试必须在这里停下。",
        );
        names.push(name);
    }
    names
}

/// 文件开头的 `//!` 模块头（连续的文档注释行）。
pub(super) fn module_header(source: &str) -> String {
    let mut lines = Vec::new();
    for line in source.lines() {
        if !line.starts_with("//!") {
            break;
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// 把 `//!` 模块头切成段落（一条空的文档注释行是段落分隔）。
fn module_header_paragraphs(source: &str) -> Vec<String> {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let header = module_header(source);
    for line in header.lines() {
        let text = line.trim_start_matches("//!").trim();
        if text.is_empty() {
            if !current.is_empty() {
                paragraphs.push(current.join("\n"));
                current.clear();
            }
            continue;
        }
        current.push(text);
    }
    if !current.is_empty() {
        paragraphs.push(current.join("\n"));
    }
    paragraphs
}

/// 中文数词（**不含「一」**：「一个端口」是泛指，不是规模计数）。
const CHINESE_NUMERALS: [char; 10] = ['二', '三', '四', '五', '六', '七', '八', '九', '十', '两'];

/// 文本结尾处的数词（十进制数字或中文数词），允许中间隔空格。
fn numeral_before(text: &str) -> Option<String> {
    let trimmed = text.trim_end_matches([' ', '\t']);
    let ascii: String = trimmed
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if !ascii.is_empty() {
        return Some(ascii.chars().rev().collect());
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let last = *chars.last()?;
    if !CHINESE_NUMERALS.contains(&last) {
        return None;
    }
    // 「同一个」「每一个」是泛指，不是计数。
    if chars.len() >= 2 && matches!(chars[chars.len() - 2], '同' | '每') {
        return None;
    }
    Some(last.to_string())
}

/// 模块头里的**端口族规模计数**：`N 个 trait` / `N 个端口`（N 是十进制数字或中文数词）、
/// `N 行`（§6.4 表的行数这类）以及 `第 N 个/行` 这类序数。
///
/// 判据刻意收窄，免得把正常散文判成违规：
/// * 量词之后允许有修饰词（「六个窄端口」「三个物理预算端口」），所以只看「个」之后
///   紧邻的一小段窗口；
/// * 「一个端口」这种泛指不是计数，因此中文数词「一」不参与判定。
pub(super) fn scale_counts(header: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (index, line) in header.lines().enumerate() {
        // 形态一：数词 + 「个」 + （修饰词）+ trait/端口
        for (at, _) in line.match_indices('个') {
            let Some(numeral) = numeral_before(&line[..at]) else {
                continue;
            };
            let tail = &line[at + '个'.len_utf8()..];
            let window: String = tail.chars().take(12).collect();
            if !(window.contains("trait") || window.contains("端口")) {
                continue;
            }
            found.push((index + 1, format!("{numeral} 个 …")));
        }
        // 形态二：十进制数字 + 「行」（只判**端口表行数**这一类：单文件行数上限、
        // 测试条数等不是端口族规模，不归本断言管）
        if line.contains("§6.4") || line.contains("端口") {
            for (at, _) in line.match_indices('行') {
                let Some(digits) = numeral_before(&line[..at]) else {
                    continue;
                };
                if !digits.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                found.push((index + 1, format!("{digits} 行")));
            }
        }
        // 形态三：序数「第 N 个/行/条/项/位」
        for (at, _) in line.match_indices('第') {
            let rest = &line[at + '第'.len_utf8()..].trim_start_matches([' ', '\t']);
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                continue;
            }
            let after = rest[digits.len()..].trim_start_matches([' ', '\t']);
            if let Some(quantifier) = after.chars().next() {
                if "个行条项位列".contains(quantifier) {
                    found.push((index + 1, format!("第 {digits} {quantifier}")));
                }
            }
        }
    }
    found
}

/// 解析一行 Markdown 表格；不是表格行返回 `None`。
fn table_cells(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') {
        return None;
    }
    let inner = trimmed.strip_prefix('|')?.strip_suffix('|')?;
    Some(
        inner
            .split('|')
            .map(|cell| cell.trim().to_string())
            .collect(),
    )
}

/// 解析 [概要 §6.4](system-overview.md#64-repositories-与环境-ports) 的端口表，
/// 返回表格第一列的接口名。
///
/// **解析不出来就是错误**，不是「没有可对照项」：表格是跨文档契约的另一半，格式变了
/// 必须让 CI 红并指出，而不是悄悄放过一份已经对不上的文档。
pub(super) fn section_6_4_table(doc: &str) -> Result<Vec<String>, String> {
    let lines: Vec<&str> = doc.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.starts_with("### 6.4"))
        .ok_or_else(|| {
            "system-overview.md 里找不到 §6.4 小节标题（形如 `### 6.4 …`）".to_string()
        })?;
    // §6.4 小节的范围：到下一个同级或更高级标题为止。
    let mut end = lines.len();
    for (index, line) in lines.iter().enumerate().skip(start + 1) {
        if line.starts_with("## ") || line.starts_with("### ") {
            end = index;
            break;
        }
    }
    let section = &lines[start..end];

    let header = section
        .iter()
        .position(|line| {
            table_cells(line)
                .and_then(|cells| cells.first().cloned())
                .map(|first| first == "接口")
                .unwrap_or(false)
        })
        .ok_or_else(|| "§6.4 里找不到第一列为「接口」的表头".to_string())?;

    let separator = section
        .get(header + 1)
        .and_then(|line| table_cells(line))
        .ok_or_else(|| "§6.4 的表头后面没有表格行".to_string())?;
    if !separator.iter().all(|cell| {
        cell.chars()
            .all(|c| c == '-' || c == ':' || c.is_whitespace())
    }) {
        return Err("§6.4 的表头后面不是 Markdown 分隔行（形如 `| --- | --- |`）".to_string());
    }

    let mut names = Vec::new();
    for line in &section[header + 2..] {
        let cells = match table_cells(line) {
            Some(cells) => cells,
            None => break,
        };
        let name = cells[0].trim().to_string();
        if name.is_empty() {
            return Err("§6.4 的表格里有空的第一列".to_string());
        }
        names.push(name);
    }
    if names.is_empty() {
        return Err("§6.4 的表格里没有任何数据行".to_string());
    }
    Ok(names)
}

/// `ports/mod.rs` 模块头里列举端口构成的那一段。
///
/// 锚点是**概要 §6.4 的链接**。找不到、或找到不止一段，都**明确报错**：解析不出来
/// 不能当「没有可对照项」放行。
fn composition_paragraph(source: &str) -> Result<String, String> {
    let paragraphs = module_header_paragraphs(source);
    let matched: Vec<&String> = paragraphs
        .iter()
        .filter(|paragraph| paragraph.contains(SECTION_6_4_ANCHOR))
        .collect();
    match matched.len() {
        1 => Ok(matched[0].clone()),
        0 => Err(format!(
            "ports/mod.rs 的模块头里找不到含 §6.4 链接（`{SECTION_6_4_ANCHOR}`）的段落——\
             那才是列举端口构成的那一段；写法变了就同步本测试。"
        )),
        count => Err(format!(
            "ports/mod.rs 的模块头里有 {count} 段含 §6.4 链接，无法确定哪一段是「端口构成」\
             ——锚点必须唯一；写法变了就同步本测试。"
        )),
    }
}

/// 抽出文本里成对反引号包住的片段。
fn backtick_tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '`' {
            i += 1;
            continue;
        }
        i += 1;
        let start = i;
        while i < chars.len() && chars[i] != '`' {
            i += 1;
        }
        if i < chars.len() {
            tokens.push(chars[start..i].iter().collect::<String>());
            i += 1;
        }
    }
    tokens
}

/// 是不是 trait 形状的名字（首字母大写、其余是 ASCII 字母数字）。
///
/// 模块头里同时出现模块名（`` [`budget`] ``、`` [`clock`] ``）与 trait 名；模块名小写，
/// 因此「首字母大写」这条判据把两类分开。
fn is_trait_shape(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric())
}

/// `ports/mod.rs` 模块头的构成说明里**逐个点名**的端口 trait。
pub(super) fn header_named_traits(source: &str) -> Vec<String> {
    let paragraph = composition_paragraph(source)
        .unwrap_or_else(|error| panic!("解析 ports/mod.rs 模块头的构成说明失败：{error}"));
    let mut names: Vec<String> = backtick_tokens(&paragraph)
        .into_iter()
        .filter(|token| is_trait_shape(token))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// 端口源码里实际声明的 trait 名字（去重后按字母序）。
pub(super) fn declared_port_traits() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (_, source) in SOURCE_FILES {
        names.extend(declared_traits(source));
    }
    names.sort();
    names.dedup();
    names
}

/// 递归收集目录下的 `.rs` 文件（相对路径）。
pub(super) fn collect_rs_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("读不到端口源码目录 {}：{error}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("目录项不可读：{error}"));
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(root, &path, out);
            continue;
        }
        if path.extension().map(|ext| ext == "rs").unwrap_or(false) {
            let relative = path
                .strip_prefix(root)
                .expect("收集到的文件必须位于端口源码目录内")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(relative);
        }
    }
}
