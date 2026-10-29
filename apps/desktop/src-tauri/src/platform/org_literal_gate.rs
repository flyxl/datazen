//! 宿主侧的**组织 id 字面量闸门**：宿主**生产代码**里不得把组织 id 写死成
//! 字符串字面量。
//!
//! # 这条闸门为什么存在
//!
//! 桌面的**规范**本地组织 id 是 [`super::identity::LOCAL_ORGANIZATION_ID`]
//! （`"datazen-desktop-local"`，进程常量，见 [`super::identity`]）。它用于桌面身份
//! 组装、`scope.rs` 的 `local_organization_scope()` / `is_local_scope()`、以及
//! schema-diff 评审计划的存储绑定。
//!
//! 但 2026-10-09 引入 `jobs` 表的那次提交里，迁移三件套与启动期恢复扫描把
//! **另一个**字面量 `"datazen-local"` 写死成了组织 id。两个值落在
//! `jobs.organization_id` 的**同一列**上 ⇒ 同一张表里落着两套组织。
//!
//! 这种分裂是「偶然对齐」而不是契约保证：每条特性自己受理、自己读取，用的是同一个
//! ctx，于是看不出症状；但只要有人再写一处字面量，分裂就会立刻变成「任务互相看不见」。
//! 因此本闸门把「生产代码里不得内联组织 id 字面量」钉成**常驻断言**。
//!
//! # 规则（G-ORG-1）
//!
//! 宿主 `src-tauri/src/` 下**生产代码**（排除 `#[cfg(test)]` 体与测试专用文件）里，
//! 每一次
//!
//! ```text
//! OrganizationId::new("<字符串字面量>")
//! ```
//!
//! 都是违规——**组织 id 被写死成字面量**。
//!
//! 以下**不算**违规：
//!
//! * `OrganizationId::new(LOCAL_ORGANIZATION_ID)` —— 取**规范常量**；
//! * `OrganizationId::new(某个变量)` —— 取**运行时值**；
//! * `#[cfg(test)]` 体与测试专用文件里的字面量 —— 那是夹具，不是生产取值。
//!
//! # 判据为什么这么窄
//!
//! **不要**改判据成「扫所有形如 `datazen-…` 的字面量」：那会命中 crate 名、临时目录
//! 前缀、测试夹具里的组织名，噪声大到没人愿意看，最后只会被整体关掉。
//! 「组织 id 被写死」这件事有一个**窄而准**的形态：`OrganizationId::new("…")`。
//!
//! # 每条规则自带 kill test、不误伤用例与自证
//!
//! 只做匹配而不证明「它抓得到东西」的闸门比没有闸门更危险（它给出虚假保证）。
//! 故本文件有三类用例：
//!
//! * **kill test**：植入一处违规字面量，断言判据转红；
//! * **不误伤**：`#[cfg(test)]` 夹具里的字面量不得转红，**且必须同时断言扫描器确实
//!   看得见那个夹具字面量**——否则「不转红」只是因为它瞎；
//! * **自证**：断言扫描器**确实**覆盖到本波修好的那些生产站点（把它们改回字面量，
//!   闸门必须转红），并给出一条「至少扫到 N 处调用点」的地板——否则闸门可能
//!   「一直绿 = 空转」。
//!
//! # 与另一条轨的闸门的关系
//!
//! 本文件与 `scope_gate.rs`（另一条轨：组织**作用域**直读闸门）是**两个独立**的
//! 闸门：两条轨会同时存在于 `main`，因此本文件**自带**所需的全部扫描器实现，
//! 不引用、不依赖 `scope_gate.rs` 的任何符号。改写自 `scope_gate.rs` 的部分在下方
//! 逐处注明来源。
//!
//! # 闸门的边界（不留模糊措辞）
//!
//! * 它是**测试期的机械闸门**，不是类型系统保证：它查的是**代码形态**，
//!   管不了运行时行为。
//! * 它只扫宿主 crate 的 `src/`。**共享包（`packages/**`）不在范围内**——那边另有
//!   `scripts/check-platform-crate-boundaries.mjs` 管边界，而共享包**本来就不该**
//!   出现桌面常量。
//! * 它**不引入任何进程级全局状态**：全部是纯函数 + 只读的文件系统遍历。
//! * 蓄意对抗（把构造拆成 `concat!`、用 `#[cfg]` 藏起来）挡不住。

use std::path::{Path, PathBuf};

/// 被闸门盯着的那一处构造。
///
/// **刻意写成字符串常量**而不是路径里的标识符：闸门扫的是**代码层**
/// （注释与字符串字面量已抹掉），本文件自己若在代码里写出这个名字，
/// 就会被自己的规则点红。
const ORG_CTOR: &str = concat!("OrganizationId", "::new");

/// 宿主 crate 里被扫的子树（相对 crate 根）。
const SRC: &str = "src";

/// 本波统一掉的四个生产站点（相对 `src-tauri/src`）。
///
/// 这四处曾经各自写死一个组织 id 字面量；统一之后它们取**规范常量**。
/// 自证用例会把它们**改回字面量**再扫一遍——闸门必须转红 thereby 证明它确实
/// 覆盖到了这些站点（而不是「因为扫不到所以没报」）。
const UNIFIED_SITES: &[&str] = &[
    "bootstrap/app_state.rs",
    "commands/schema_diff/job.rs",
    "commands/data_transfer/job_api/runtime.rs",
    "commands/sync/jobs.rs",
];

/// 统一之后这四处应当长成的形态（组织取规范常量）。
const CANONICAL_CALL: &str = concat!(
    "OrganizationId",
    "::new(crate::platform::LOCAL_ORGANIZATION_ID)"
);

/// 统一之前这四处长成的形态（组织被写死成字面量）。
const LEGACY_CALL: &str = concat!("OrganizationId", "::new(\"datazen-local\")");

/// 曾经写死的那个组织 id 字面量（自证用例据此定位）。
const LEGACY_ORGANIZATION_ID: &str = "datazen-local";

/// 生产代码里 `OrganizationId::new(` 调用点的**地板**。
///
/// 没有地板的话，「扫描范围整个漏掉」与「一切合规」在断言上长得一模一样。
/// 当前生产代码里这样的调用点有 12 处（4 处本波统一的 + 8 处既有），
/// 地板取 8：既有冗余，又高到「不可能是空转」。
const MIN_PRODUCTION_CALL_SITES: usize = 8;

/// 一处「组织 id 被写死成字面量」。
#[derive(Debug, Clone)]
struct LiteralOrg {
    /// 相对 `src-tauri/src` 的路径（正斜杠分隔）。
    file: String,
    /// 1 基行号。
    line: usize,
    /// 字面量的内容（不含引号）。
    literal: String,
}

impl LiteralOrg {
    fn describe(&self) -> String {
        format!(
            "{}:{}  OrganizationId::new({:?})",
            self.file, self.line, self.literal
        )
    }
}

// ─────────────────────────── 判据 ───────────────────────────

/// 宿主 crate 的根目录（编译期常量，不是运行时全局）。
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 宿主 `src/` 下的全部 `.rs` 源码（相对 `src/`，正斜杠分隔）。
fn source_files_on_disk() -> Vec<String> {
    let root = crate_root().join(SRC);
    let mut out = Vec::new();
    collect_rs(&root, &root, &mut out);
    out.sort();
    out
}

fn collect_rs(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

/// 读一个源码文件的内容（相对 `src/`）。
fn read_source(rel: &str) -> String {
    let path = crate_root().join(SRC).join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("读不到 {rel}：{err}（闸门必须真的看见源码）"))
}

/// 这个路径是不是**测试专用**的文件。
///
/// 三条判据：路径里有 `tests` 目录段；文件名是 `tests.rs` 或 `*_tests.rs`；
/// 或者它被某个 `#[cfg(test)] mod …;` 声明**包含**（那种包含在第一遍扫描里收集）。
fn is_test_only_path(rel: &str, included: &[String]) -> bool {
    if included.iter().any(|p| p == rel) {
        return true;
    }
    let segments: Vec<&str> = rel.split('/').collect();
    if segments.contains(&"tests") {
        return true;
    }
    let name = segments.last().copied().unwrap_or_default();
    name == "tests.rs" || name.ends_with("_tests.rs")
}

/// 一个 `#[cfg(test)]` 项的解析结果。
struct CfgTestItem {
    /// 项体内联花括号的区间（`mod x { … }` / `fn x() { … }`）。
    body: Option<(usize, usize)>,
    /// 外部文件（`mod x;`）：相对 `src/` 的路径。
    external: Option<String>,
}

/// 找出源码里所有 `#[cfg(test)]` 项。
///
/// 只认两种形态（本仓库的全部用法都是这两种）：
///
/// * `#[cfg(test)] mod x { … }` —— 内联，测试区 = 花括号体；
/// * `#[cfg(test)] mod x;`（可带 `#[path = "…"]`）—— 外部文件，整个文件都是测试区。
///
/// 其余（`fn` / `use` / `const` / `static` / `impl` …）也一并处理：
/// 有花括号体的取体，无花括号体的取到分号为止。
///
/// 来源：改写自 `scope_gate.rs` 的 `cfg_test_items()`（同一手法，独立实现）。
///
/// `code` 是**抹掉注释与字符串**后的代码层（用来算花括号区间），`original` 是原文
/// ——`#[path = "…"]` 的**值**是字符串字面量，在代码层里已被抹掉；不回原文读就拿不到
/// 路径，外部测试文件会因此认不出来（它会被当生产代码扫）。
fn cfg_test_items(code: &str, original: &str) -> Vec<CfgTestItem> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = code[cursor..].find("#[cfg(test)]") {
        let at = cursor + rel;
        let mut j = at + "#[cfg(test)]".len();
        // 跳过后续属性，并记下 `#[path = "…"]`。
        let mut path_attr: Option<String> = None;
        loop {
            let rest = &code[j..];
            let trimmed = rest.trim_start();
            let skip = rest.len() - trimmed.len();
            if !trimmed.starts_with("#[") {
                j += skip;
                break;
            }
            let Some(end) = trimmed.find(']') else {
                j += skip;
                break;
            };
            // 属性文本在**原文**里的同一段字节（抹串后逐字节等长）。
            let attr_start = j + skip;
            let attr = original
                .get(attr_start..attr_start + end)
                .unwrap_or_default();
            if let Some(value) = extract_path_attr(attr) {
                path_attr = Some(value);
            }
            j += skip + end + 1;
        }
        // j 现在指向被 `#[cfg(test)]` 标注的项。
        let rest = &code[j..];
        let trimmed = rest.trim_start();
        let lead = rest.len() - trimmed.len();
        let item_start = j + lead;

        if let Some(name) = mod_name(trimmed) {
            // `mod x;` 与 `mod x { … }` 必须分开：前者是**外部文件**，
            // 后者是内联体。看谁先出现——`;` 先出现就是外部文件。
            let brace = trimmed.find('{');
            let semi = trimmed.find(';');
            let external_first = match (brace, semi) {
                (None, Some(_)) => true,
                (Some(b), Some(s)) => s < b,
                _ => false,
            };
            if external_first {
                // `mod x;`：外部文件。
                let external = path_attr.unwrap_or_else(|| format!("{name}.rs"));
                out.push(CfgTestItem {
                    body: None,
                    external: Some(external),
                });
            } else if let Some(brace) = brace {
                // `mod x { … }`：测试区 = 花括号体。
                if let Some(end) = match_brace(code, item_start + brace) {
                    out.push(CfgTestItem {
                        body: Some((item_start + brace + 1, end - 1)),
                        external: None,
                    });
                }
            }
        } else {
            // 其它项：有体取体，无体取到分号。取两者的**较大**结束位置，
            // 这样 `static X = LazyLock::new(|| { … });` 这种「体在初始化式里」
            // 的写法也整条被算成测试区。
            let brace = trimmed
                .find('{')
                .and_then(|b| match_brace(code, item_start + b));
            let semi = trimmed.find(';').map(|s| item_start + s + 1);
            let end = match (brace, semi) {
                (Some(b), Some(s)) => Some(b.max(s)),
                (Some(b), None) => Some(b),
                (None, Some(s)) => Some(s),
                (None, None) => None,
            };
            if let Some(end) = end {
                out.push(CfgTestItem {
                    body: Some((item_start, end)),
                    external: None,
                });
            }
        }
        cursor = j + 1;
    }
    out
}

/// `mod x` / `mod x;` 里的模块名。
fn mod_name(item: &str) -> Option<String> {
    let rest = item.strip_prefix("mod ")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// 从属性文本里取 `path = "…"` 的值。
fn extract_path_attr(attr: &str) -> Option<String> {
    let rest = attr.strip_prefix("path")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// `open` 处 `{` 的匹配 `}` 之后的下标。
fn match_brace(code: &str, open: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// 一个字节是不是标识符字符。
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// 抹掉注释与字符串 / 字符字面量，**逐字节保留下标与换行**。
///
/// 与 `scope_gate.rs` 的同名函数的**唯一**差别：这里连引号本身也替换成空格，
/// 因此抹完的文本与原文**逐字节等长**——本闸门要按偏移回**原文**取字面量内容，
/// 偏移漂了就取错东西。（`scope_gate` 的版本把引号直接丢掉，长度会漂；它只需要
/// 行号，故不受影响。）
///
/// 来源：改写自 `scope_gate.rs` 的 `blank()`（注释 / 字符串 / 字符字面量 / 裸字符串
/// 四类处理沿用同一手法）。
fn blank(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        // 行注释（**到行尾为止**：换行本身留给外层循环处理，位置才不会漂）
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(b' ');
                i += 1;
            }
            continue;
        }
        // 块注释（Rust 允许嵌套）
        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let mut depth = 0usize;
            while i < bytes.len() {
                if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    out.extend(std::iter::repeat_n(b' ', 2));
                    i += 2;
                    continue;
                }
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    out.extend(std::iter::repeat_n(b' ', 2));
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                out.push(if bytes[i] == b'\n' { b'\n' } else { b' ' });
                i += 1;
            }
            continue;
        }
        // 字符字面量
        if b == b'\'' {
            if let Some(len) = char_literal_len(bytes, i) {
                out.extend(std::iter::repeat_n(b' ', len));
                i += len;
                continue;
            }
            // 生命周期标注：原样保留（它不是字面量）。
            out.push(b'\'');
            i += 1;
            continue;
        }
        // 裸字符串 r"…" / r#"…"#
        if b == b'r' && matches!(bytes.get(i + 1), Some(b'"') | Some(b'#')) {
            if let Some(len) = raw_string_len(source, i) {
                for k in 0..len {
                    out.push(if bytes[i + k] == b'\n' { b'\n' } else { b' ' });
                }
                i += len;
                continue;
            }
        }
        // 普通字符串 / 字节串字面量
        if b == b'"' {
            out.push(b' ');
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    out.push(if bytes.get(i + 1) == Some(&b'\n') {
                        b'\n'
                    } else {
                        b' '
                    });
                    out.push(b' ');
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    out.push(b' ');
                    i += 1;
                    break;
                }
                out.push(if bytes[i] == b'\n' { b'\n' } else { b' ' });
                i += 1;
            }
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `'c'` / `'\n'` / `'\u{…}'` 的长度；不是字符字面量则 `None`（生命周期）。
fn char_literal_len(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'\'') {
        return None;
    }
    match bytes.get(at + 1) {
        Some(b'\\') => {
            let mut j = at + 2;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'\'' {
                    return Some(j + 1 - at);
                }
                if bytes[j] == b'\n' {
                    return None;
                }
                j += 1;
            }
            None
        }
        Some(_) => {
            if bytes.get(at + 2) == Some(&b'\'') {
                Some(3)
            } else {
                None
            }
        }
        None => None,
    }
}

/// 裸字符串的总长度（含 `r` 与收尾定界符）。
fn raw_string_len(source: &str, at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut j = at + 1;
    let mut hashes = 0usize;
    while bytes.get(j) == Some(&b'#') {
        hashes += 1;
        j += 1;
    }
    if bytes.get(j) != Some(&b'"') {
        return None;
    }
    let close: String = std::iter::once('"')
        .chain(std::iter::repeat_n('#', hashes))
        .collect();
    let rest = &source[j + 1..];
    let end = rest.find(&close)?;
    Some(j + 1 + end + close.len() - at)
}

/// 源码里所有 `OrganizationId::new(` 调用点的**左括号下标**（代码层，不含注释与
/// 字符串内）。
///
/// 只认**调用**：名字后面必须紧跟左括号（允许空白）。
fn org_ctor_offsets(code: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = code[from..].find(ORG_CTOR) {
        let at = from + rel;
        from = at + ORG_CTOR.len();
        // 标识符边界：`MyOrganizationId::new(` 不是 `OrganizationId::new(`。
        if at > 0 && is_ident_byte(code.as_bytes()[at - 1]) {
            continue;
        }
        let after = &code[at + ORG_CTOR.len()..];
        let trimmed = after.trim_start();
        if !trimmed.starts_with('(') {
            continue;
        }
        let paren = at + ORG_CTOR.len() + (after.len() - trimmed.len());
        out.push(paren);
    }
    out
}

/// 调用点的**实参**是不是字符串字面量；是则给出字面量内容。
///
/// `open_paren` 是左括号的下标（指向**原文**）。
fn string_literal_argument(source: &str, open_paren: usize) -> Option<String> {
    let bytes = source.as_bytes();
    let start = skip_trivia(source, open_paren + 1);
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let content_start = start + 1;
    let mut j = content_start;
    let mut end = None;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j = (j + 2).min(bytes.len()),
            b'"' => {
                end = Some(j);
                break;
            }
            _ => j += 1,
        }
    }
    end.map(|e| source[content_start..e].to_string())
}

/// 跳过空白与注释（从 `i` 开始）。
fn skip_trivia(source: &str, mut i: usize) -> usize {
    let bytes = source.as_bytes();
    loop {
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'*') {
            let mut depth = 0usize;
            while i < bytes.len() {
                if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                    continue;
                }
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                i += 1;
            }
            continue;
        }
        return i;
    }
}

/// 一个文件里**生产区**的全部调用点（左括号下标）。
fn production_ctor_offsets(code: &str, original: &str, test_only_file: bool) -> Vec<usize> {
    if test_only_file {
        return Vec::new();
    }
    let regions: Vec<(usize, usize)> = cfg_test_items(code, original)
        .into_iter()
        .filter_map(|item| item.body)
        .collect();
    org_ctor_offsets(code)
        .into_iter()
        .filter(|at| !regions.iter().any(|(s, e)| *at >= *s && *at <= *e))
        .collect()
}

/// 一个文件里**生产区**的全部「组织 id 字面量」。
fn production_literals(rel: &str, source: &str, test_only_file: bool) -> Vec<LiteralOrg> {
    let code = blank(source);
    production_ctor_offsets(&code, source, test_only_file)
        .into_iter()
        .filter_map(|at| {
            string_literal_argument(source, at).map(|literal| LiteralOrg {
                file: rel.to_string(),
                // 行号取自**抹串后**的代码层：抹串保留了换行，行号因此一致；
                // 而抹串后的文本全是 ASCII，按字节切片不会切在字符中间。
                line: 1 + code[..at].matches('\n').count(),
                literal,
            })
        })
        .collect()
}

/// 一个文件里**全部**的「组织 id 字面量」（含测试区）。
///
/// 这是「不误伤」用例的**另一半**：它证明扫描器**看得见**测试区里的那个字面量。
fn all_literals(rel: &str, source: &str) -> Vec<LiteralOrg> {
    let code = blank(source);
    org_ctor_offsets(&code)
        .into_iter()
        .filter_map(|at| {
            string_literal_argument(source, at).map(|literal| LiteralOrg {
                file: rel.to_string(),
                line: 1 + code[..at].matches('\n').count(),
                literal,
            })
        })
        .collect()
}

/// 扫描宿主 `src/`：**生产代码**里的全部「组织 id 字面量」。
fn scan_production_literals() -> Vec<LiteralOrg> {
    let files = source_files_on_disk();
    let included = included_test_files(&files);
    // 第二遍：生产区的字面量。
    let mut out = Vec::new();
    for rel in &files {
        let source = read_source(rel);
        out.extend(production_literals(
            rel,
            &source,
            is_test_only_path(rel, &included),
        ));
    }
    out
}

/// 被某个 `#[cfg(test)] mod …;` 包含的**外部测试文件**（相对 `src/`）。
fn included_test_files(files: &[String]) -> Vec<String> {
    let mut included: Vec<String> = Vec::new();
    for rel in files {
        let source = read_source(rel);
        let code = blank(&source);
        for item in cfg_test_items(&code, &source) {
            if let Some(external) = item.external {
                let dir = match rel.rfind('/') {
                    Some(at) => rel[..at].to_string(),
                    None => String::new(),
                };
                let joined = if dir.is_empty() {
                    external
                } else {
                    format!("{dir}/{external}")
                };
                included.push(joined);
            }
        }
    }
    included
}

/// 生产代码里 `OrganizationId::new(` 调用点的总数（不区分实参是不是字面量）。
fn production_call_site_count() -> usize {
    let files = source_files_on_disk();
    let included = included_test_files(&files);
    let mut total = 0usize;
    for rel in &files {
        let source = read_source(rel);
        let code = blank(&source);
        total += production_ctor_offsets(&code, &source, is_test_only_path(rel, &included)).len();
    }
    total
}

// ─────────────────────────── 用例 ───────────────────────────

/// **kill test**：植入一处违规字面量，判据必须转红。
///
/// 判据与扫描用的是同一套代码（[`production_literals`]），所以这条用例证明的是
/// 「闸门真的抓得到」，而不是「另写了一个恰好也能抓到的检查」。
#[test]
fn the_gate_catches_a_planted_organization_literal() {
    let planted = "fn f() {\n    let org = OrganizationId::new(\"datazen-local\");\n}\n";
    let found = production_literals("planted.rs", planted, false);
    assert_eq!(
        found.len(),
        1,
        "闸门没抓到植入的字面量 —— 规则形同虚设（实得 {found:?}）"
    );
    assert_eq!(found[0].literal, "datazen-local");
    assert_eq!(found[0].line, 2, "行号必须指到真正的那一行");
}

/// **不误伤**：`#[cfg(test)]` 夹具里的字面量不得转红，**且扫描器必须看得见它**。
///
/// 后半句是关键：只断言「不转红」的话，闸门因为「什么也没扫到」而不转红时，
/// 这条用例照样绿——那等于没有闸门。
#[test]
fn the_gate_does_not_flag_a_literal_in_a_test_fixture_but_can_see_it() {
    let source = "fn f() {}\n\
                  #[cfg(test)]\n\
                  mod tests {\n\
                  \x20   fn g() {\n\
                  \x20       let org = OrganizationId::new(\"org-1\");\n\
                  \x20   }\n\
                  }\n";
    assert!(
        production_literals("in_test.rs", source, false).is_empty(),
        "测试区里的字面量被当成了生产字面量 —— 判据必须先分清测试区"
    );
    let all = all_literals("in_test.rs", source);
    assert_eq!(
        all.len(),
        1,
        "扫描器必须**看得见**测试区里的那个字面量，否则上面的「不转红」只是因为它瞎（实得 {all:?}）"
    );
    assert_eq!(all[0].literal, "org-1");

    // 测试专用文件里的字面量同样不算生产字面量，但扫描器仍要看得见。
    assert!(
        production_literals("tests/whatever.rs", source, true).is_empty(),
        "测试专用文件里的字面量被当成了生产字面量"
    );
    assert_eq!(
        all_literals("tests/whatever.rs", source).len(),
        1,
        "扫描器必须看得见测试专用文件里的那个字面量"
    );
}

/// **不误伤**（其二）：取**规范常量**与取**变量**都不是违规。
#[test]
fn the_gate_does_not_flag_the_canonical_constant_or_a_variable() {
    let source = "fn f(org: &str) {\n\
                  \x20   let a = OrganizationId::new(LOCAL_ORGANIZATION_ID);\n\
                  \x20   let b = OrganizationId::new(org);\n\
                  \x20   let c = OrganizationId::new(crate::platform::LOCAL_ORGANIZATION_ID);\n\
                  }\n";
    assert!(
        production_literals("ok.rs", source, false).is_empty(),
        "取规范常量 / 取变量都不是违规（实得 {:?}）",
        production_literals("ok.rs", source, false)
    );
    // 但扫描器确实看见了那三处调用点——「不转红」不是因为它瞎。
    let code = blank(source);
    assert_eq!(
        production_ctor_offsets(&code, source, false).len(),
        3,
        "扫描器必须看见这三处调用点"
    );
}

/// **自证**：扫描器**确实**覆盖到本波统一掉的四个生产站点。
///
/// 做法：把这四处**改回字面量**再扫一遍——闸门必须转红。这比「断言扫到 N 处」
/// 更强：它逐点证明了「闸门看的正是这些站点」。
#[test]
fn the_scanner_reaches_every_unified_production_site() {
    for rel in UNIFIED_SITES {
        let source = read_source(rel);
        let untouched = production_literals(rel, &source, false);
        assert!(
            !untouched
                .iter()
                .any(|hit| hit.literal == LEGACY_ORGANIZATION_ID),
            "{rel} 仍在生产代码里写死组织 id 字面量：{untouched:?}"
        );

        // 把规范常量换回字面量 ⇒ 闸门必须转红。
        let mutated = source.replace(CANONICAL_CALL, LEGACY_CALL);
        assert_ne!(
            mutated, source,
            "{rel} 不再是「取规范常量」的形态（找不到 {CANONICAL_CALL}）——\
             闸门的自证需要跟着改"
        );
        let found = production_literals(rel, &mutated, false);
        assert!(
            found
                .iter()
                .any(|hit| hit.literal == LEGACY_ORGANIZATION_ID),
            "把 {rel} 改回字面量后闸门仍不报 ⇒ 扫描面漏了这个站点（实得 {found:?}）"
        );
    }
}

/// **自证**（其二）：扫描面不得小到可疑。
///
/// 「生产代码里 `OrganizationId::new(` 的调用点」的数量有地板。没有地板的话，
/// 「扫描范围整个漏掉」与「一切合规」在断言上长得一模一样。
#[test]
fn the_scanner_examines_a_non_trivial_number_of_production_call_sites() {
    let sites = production_call_site_count();
    assert!(
        sites >= MIN_PRODUCTION_CALL_SITES,
        "生产代码里只扫到 {sites} 处组织 id 构造调用（地板 {MIN_PRODUCTION_CALL_SITES}）——\
         扫描面小得可疑，闸门可能一直是空转"
    );
}

/// **主规则**：生产代码里不得有「组织 id 被写死成字面量」。
#[test]
fn no_production_code_hardcodes_an_organization_id_literal() {
    let found = scan_production_literals();
    assert!(
        found.is_empty(),
        "宿主生产代码里不得把组织 id 写死成字符串字面量。\
         请改用 `crate::platform::LOCAL_ORGANIZATION_ID`（或从请求上下文取）。\
         下面这些调用点把组织钉死在字面量上：\n{}",
        found
            .iter()
            .map(LiteralOrg::describe)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
