//! 生产接线守卫：恒定放行的作者器**不得**被接进任何生产路径。
//!
//! # 为什么需要它
//!
//! 网关的作者器是**构造参数**（`ExecutionGateway::new(port, authorizer, store, clock)`），
//! 漏传是编译错误；但传错——传 `AlwaysAllow`——**编译照过、行为静默失效**：归属闸门对所有人
//! 放行，CM-05 / CM-06 在运行期形同虚设，而 `owner_binding.rs` 里的行为测试**依然全绿**
//! （它自己传的就是对的作者器）。散文警告对不住这种失效，本文件把它变成一条会红的门禁。
//!
//! 今天全仓 `AlwaysAllow` / `AlwaysDeny` 只出现在：定义点、`pub use` 重导出、以及测试代码里。
//! 所以这条守卫**现在是空绿**：它证明的是「第一个把网关接进生产路径的人会红」，
//! **不是**「今天的接线是对的」——后者靠的是上面那些行为测试，不是这条扫描。
//!
//! # 为什么能判「生产」
//!
//! 判定按**路径与模块声明**，不做任何「文件名里含不含 test」的子串匹配
//! （`"latest".contains("test")` 是这类实现的经典假阳性）：
//!
//! * 路径里有 `tests` 目录段、或文件名以 `_test` / `_tests` 结尾 ⇒ 测试；
//! * 父目录的 `mod.rs` / `lib.rs` / `main.rs` 用 `#[cfg(test)]` 或 `#[cfg(doctest)]` 声明了
//!   `mod <文件名去扩展名>` ⇒ 测试（`src/gateway/` 下 `facade_support.rs` 这类「住在生产
//!   目录里、实为单测」文件的判据）；其余 ⇒ 生产。
//!
//! 对生产文件，判定前先剥掉三层**不是接线**的东西：注释与字符串字面量的内容、
//! `#[cfg(test)]` 内联模块、以及 `use` / `pub use` 导入（重导出不算使用）。剥完还出现
//! 目标名字，才是真正的接线。
//!
//! # 已知边界（比能力弱，不比要求弱）
//!
//! 真正的语义接线（经 trait 对象、宏、反射到达 `AlwaysAllow`）不在扫描能力内——
//! 那需要 lint 级数据流分析，超出本轨范围；原始字符串（`r#"…"#`）内部的提及也识别不了，
//! 只影响「是否漏报」，不影响「是否误报」。
//! 剥除过程用等量换行补回被吞掉的字符，所以**报出来的行号就是原文件的真实行号**
//! （实测：探针插到 `gateway/mod.rs` 的第 67 行，报告里就是 `mod.rs:67`；插到文件末尾
//! 则是 `mod.rs:800`）。这条是硬要求——守卫红了却指着错行，等于让人去改一个没问题的地方。
//!
//! # 反证（这个守卫怎么被证伪过）
//!
//! 三条断言是这份文件自己的失败证据：
//!
//! 1. `the_wiring_guard_catches_a_planted_production_wiring` —— 合成接线被抓住，
//!    诚实的 `OwnerMatchAuthorizer::shared()` 不被误报。
//! 2. `the_wiring_guard_ignores_prose_and_test_only_mentions` —— 行注释、文档注释、
//!    字符串字面量、`#[cfg(test)]` 内联模块里的提及都不算。
//! 3. `the_definition_site_exemption_is_still_a_real_definition` —— 豁免的那个文件必须
//!    **还真的是定义处**；哪天 `AlwaysAllow` 被改名/搬走，这里先红，豁免不会变成「漏检」。
//!
//! # 真实文件上的反证
//!
//! 这里原先写着「在 `request.rs:603` 上跑过真实反例」。**那是错的**：`request.rs`
//! 实测 599 行、`AlwaysAllow`/`AlwaysDeny` 零命中，603 行不存在，那次运行从未发生。
//! 下面是真正在磁盘上跑过、可复现的探针（同一个守卫、同一个探针内容，两个位置只差
//! 插入点；每次 `git checkout --` 还原后 `touch`）：
//!
//! 1. 插到 `gateway/mod.rs` 里那个过去会被整段吞掉的窗口（`pub(crate) mod
//!    testing_support;` 之后）⇒ 报 `mod.rs:67`，`EXIT=101`；
//! 2. 阳性对照：同一探针插到该文件末尾 ⇒ 报 `mod.rs:800`，`EXIT=101`。
//!
//! 两处行号都等于插入位置，这才是「守卫能报红、且指的是真行」的证据；常驻版本是
//! `the_wiring_guard_reads_the_real_gateway_module_past_its_test_declarations`。

use std::fs;
use std::path::{Path, PathBuf};

/// 恒定放行的作者器。定义在 `packages/runtime/src/gateway/provenance.rs`，
/// 并经 `gateway/mod.rs` 的 `pub use` 对外可达，任何 crate 都拿得到。
const NEVER_WIRE_THESE: &[&str] = &["AlwaysAllow", "AlwaysDeny"];

/// 目录里没有任何「本该被扫」的源码，跳过（构建产物、依赖、其它工作树）。
const PRUNED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    ".worktrees",
    "dist",
    "build",
    ".next",
    "coverage",
];

/// 允许出现这两个名字的**唯一**文件：它们自己的定义点。
/// 导入语句已被剥掉，所以 `pub use` 重导出不会命中；剩下的定义必须显式豁免。
/// 豁免本身由 `the_definition_site_exemption_is_still_a_real_definition` 钉住。
const DEFINITION_SITE: &str = "packages/runtime/src/gateway/provenance.rs";

/// 完整性守卫：这些文件必须真的被扫到，否则「一条违规都没有」可能只是**没扫到**。
const MUST_VISIT: &[&str] = &[
    "packages/runtime/src/gateway/mod.rs",
    "packages/runtime/src/gateway/provenance.rs",
    "packages/runtime/src/gateway/owner_binding.rs",
    "packages/runtime/src/connection/types.rs",
    "packages/platform-api/src/context.rs",
    "src-tauri/src/platform/identity.rs",
];

pub(super) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR 至少要有两级祖先")
        .to_path_buf()
}

pub(super) fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            if !PRUNED_DIRS.contains(&name.as_str()) {
                collect_rs(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn attributes_before(src: &str, at: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut cursor = at;
    loop {
        let trimmed = src[..cursor].trim_end();
        if let Some(head) = trimmed.strip_suffix(']') {
            let head_start = head.rfind('[').unwrap_or(0);
            found.push(head[head_start..].to_string());
            cursor = head_start;
        } else if let Some(head) = trimmed.strip_suffix(')') {
            // 可见性修饰：`pub(crate) mod x;` / `pub(in a::b) mod x;`
            cursor = head.rfind('(').unwrap_or(0);
        } else if trimmed.ends_with("pub") {
            cursor = trimmed.len() - "pub".len();
        } else {
            return found;
        }
    }
}

/// 父模块是否用 `#[cfg(test)]` / `#[cfg(doctest)]` 声明了 `mod <stem>`。
fn declares_cfg_test_module(src: &str, stem: &str) -> bool {
    let needle = format!("mod {stem}");
    let mut cursor = 0usize;
    while let Some(rel) = src[cursor..].find(&needle) {
        let at = cursor + rel;
        let word_start = at == 0 || !is_ident_byte(src.as_bytes()[at - 1]);
        let cfg_test = attributes_before(src, at)
            .iter()
            .any(|attr| attr.contains("cfg(test") || attr.contains("cfg(doctest"));
        if word_start && cfg_test {
            return true;
        }
        cursor = at + needle.len();
    }
    false
}

pub(super) fn is_test_only(rel: &Path, root: &Path) -> bool {
    if rel
        .components()
        .any(|part| part.as_os_str() == std::ffi::OsStr::new("tests"))
    {
        return true;
    }
    let stem = rel
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if stem.ends_with("_test") || stem.ends_with("_tests") {
        return true;
    }
    let Some(parent) = rel.parent() else {
        return false;
    };
    ["mod.rs", "lib.rs", "main.rs"].iter().any(|anchor| {
        fs::read_to_string(root.join(parent).join(anchor))
            .is_ok_and(|src| declares_cfg_test_module(&src, stem))
    })
}

/// 剥掉注释**和字符串字面量的内容**（只留引号）。两者都不是接线：
/// 文档里、URL 里、日志文案里写着 `AlwaysAllow` 的生产文件不该被误判。
/// 普通字符串会正确处理 `\` 转义；原始字符串（`r#"…"#`）不做识别，
/// 会被当普通字符串处理，只可能漏掉「原始字符串里的内容」这一个方向。
///
/// 被剥掉的字符用等量换行补回去，这样**报出来的行号就是原文件的真实行号**——
/// 否则守卫红了，人拿着一个对不上的行号去改，改的是别的地方。
pub(super) fn strip_comments_and_literals(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i = (i + 2).min(chars.len());
        } else if c == '"' {
            i += 1;
            while i < chars.len() {
                let inner = chars[i];
                i += 1;
                if inner == '\\' && i < chars.len() {
                    i += 1;
                } else if inner == '"' {
                    break;
                }
            }
            out.push('"');
        } else {
            out.push(c);
            i += 1;
            continue;
        }
        push_newlines(&mut out, &chars[start..i]);
    }
    out
}

/// 被剥掉的那段里每有一个换行，就补一个换行回去，保持行号对齐。
fn push_newlines(out: &mut String, skipped: &[char]) {
    for c in skipped {
        if *c == '\n' {
            out.push('\n');
        }
    }
}

pub(super) fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (off, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + off);
                }
            }
            _ => {}
        }
    }
    None
}

/// `#[cfg(test…)]` 之后、**模块声明自己**的左花括号位置；不是内联块则返回 `None`。
///
/// 只认声明自身的形状：`mod x { … }` 是内联块；`mod x;` 是外部声明（模块体在别的文件里），
/// 它后面**根本没有花括号**。曾经的做法是「找到 `mod ` 之后，在剩余全文里找下一个 `{`」，
/// 于是 `#[cfg(test)] pub(crate) mod testing_support;` 会跟后面某个毫不相干的
/// `impl Foo {` 配对，把两者之间的整段**生产代码**当成测试块吞掉（实测
/// `packages/runtime/src/gateway/mod.rs` 7 处测试模块声明**全是**外部声明，
/// 起点 `mod.rs:65`，旧实现吞掉的区间在剥注释与字符串后的文本上是 **64..87**、
/// 520 字节）。守卫对那个窗口内的一切接线彻底失明，
/// 而守卫恰好就靠这个文件判定网关有没有被接上替身。
///
/// 因此：**先遇到 `;` 就是外部声明，直接放弃**；必须先遇到 `{` 才算内联块。
///
/// 这个盲区的范围已用「新旧判别器逐文件比对」量过：全仓 **54 个文件**被旧实现误吞过
/// （55 处区间，最大的是 `src-tauri/src/commands/sync/exec.rs` 的 7..1018，本轨的
/// `gateway/mod.rs` 只有 520 字节），而这些区间里真正含 `AlwaysAllow`/`AlwaysDeny` 的
/// **一处都没有**。所以它是**潜伏的绕过通道，不是眼下正在生效的漏洞**——但仍必须修：
/// 被破坏的是「剥除不得吞掉生产代码」这条**不变量**，与今天恰好有没有人违规无关。
pub(super) fn inline_module_brace(src: &str, after_attr: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = after_attr;
    // 属性与 `mod` 之间还可能有空白和别的属性（`#[cfg(test)] #[derive(..)] mod x {`）。
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if src[i..].starts_with("#[") {
            i += src[i..].find(']')? + 1;
            continue;
        }
        break;
    }
    // 可选的可见性修饰：`pub mod x;` / `pub(crate) mod x {` / `pub(in a::b) mod x {`。
    // 没有修饰的 `mod x {`（内联块最常见的写法）同样合法。
    if src[i..].starts_with("pub(") {
        i += src[i..].find(')')? + 1;
    } else if src[i..].starts_with("pub") {
        i += 3;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if !src[i..].starts_with("mod") {
        return None;
    }
    let after_mod = i + 3;
    if after_mod < bytes.len() && is_ident_byte(bytes[after_mod]) {
        return None; // `module` 之类，不是 `mod` 关键字
    }
    for (off, byte) in src[after_mod..].as_bytes().iter().enumerate() {
        match byte {
            b';' => return None, // `mod x;`：外部声明，不是内联块
            b'{' => return Some(after_mod + off),
            _ => {}
        }
    }
    None
}

/// 下一个「`#[cfg(test…)] mod … { … }`」内联模块的 (属性起点, 左花括号位置)。
pub(super) fn next_inline_test_block(src: &str, from: usize) -> Option<(usize, usize)> {
    let mut cursor = from;
    while let Some(rel) = src[cursor..].find("#[cfg(") {
        let attr_at = cursor + rel;
        let close = attr_at + src[attr_at..].find(']')?;
        let attr = &src[attr_at..=close];
        // `#[cfg(not(test))]` 是「非测试」，绝不能按测试块处理。
        let cfg_test = !attr.contains("not(")
            && (attr.contains("test)") || attr.contains("test,") || attr.contains("doctest"));
        if cfg_test {
            // 声明自己没有花括号（`mod x;`）时**不得**往后借一个 —— 见 inline_module_brace。
            if let Some(brace_at) = inline_module_brace(src, close + 1) {
                return Some((attr_at, brace_at));
            }
        }
        cursor = close + 1;
    }
    None
}

/// 剥掉 `#[cfg(test…)] mod … { … }` 内联模块。花括号配不上时**不吞**：
/// 宁可把内容留下来报一次假阳性，也不能整段吃掉生产代码。
pub(super) fn strip_inline_test_modules(src: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some((attr_at, brace_at)) = next_inline_test_block(src, cursor) {
        out.push_str(&src[cursor..attr_at]);
        match matching_brace(src, brace_at) {
            Some(end) => {
                push_newlines(&mut out, &src[attr_at..=end].chars().collect::<Vec<_>>());
                cursor = end + 1;
            }
            None => {
                cursor = attr_at;
                break;
            }
        }
    }
    out.push_str(&src[cursor..]);
    out
}

fn end_of_use_statement(src: &str, at: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (off, ch) in src[at..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ';' if depth == 0 => return Some(at + off + 1),
            _ => {}
        }
    }
    None
}

/// 剥掉 `use …;` / `pub use …;`。**导入不是使用**：`pub use` 重导出到别的 crate
/// 不构成把替身接进生产路径。
fn strip_use_statements(src: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;
    while let Some(rel) = src[cursor..].find("use ") {
        let at = cursor + rel;
        let line_start = src[..at].rfind('\n').map_or(0, |nl| nl + 1);
        let prefix = src[line_start..at].trim();
        let is_item = prefix.is_empty() || prefix == "pub" || prefix.starts_with("pub(");
        if !is_item {
            cursor = at + 4;
            continue;
        }
        match end_of_use_statement(src, at) {
            Some(end) => {
                out.push_str(&src[cursor..line_start]);
                push_newlines(&mut out, &src[line_start..end].chars().collect::<Vec<_>>());
                cursor = end;
            }
            None => break,
        }
    }
    out.push_str(&src[cursor..]);
    out
}

/// 判定用的「真正被执行的代码」：去注释 → 去 `#[cfg(test)]` 内联块 → 去导入。
fn executable_code(src: &str) -> String {
    strip_use_statements(&strip_inline_test_modules(&strip_comments_and_literals(
        src,
    )))
}

/// 在一段源码里找出「真的在用」这些名字的行号（剥除之后）。
pub(super) fn wired_names(src: &str) -> Vec<(usize, String)> {
    executable_code(src)
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            NEVER_WIRE_THESE.iter().any(|name| {
                line.split(|c: char| !is_ident_byte(c as u8))
                    .any(|w| w == *name)
            })
        })
        .map(|(idx, line)| (idx + 1, line.trim().to_string()))
        .collect()
}

#[test]
fn no_production_file_wires_an_authorizer_that_always_allows() {
    let root = repo_root();
    assert!(
        root.join("packages/runtime/Cargo.toml").is_file(),
        "仓库根识别错了：{}",
        root.display()
    );
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    files.sort();

    let mut violations: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .expect("扫描到的文件都在仓库根之下")
            .to_string_lossy()
            .into_owned();
        if is_test_only(Path::new(&rel), &root) || rel == DEFINITION_SITE {
            continue;
        }
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        scanned += 1;
        for (line, text) in wired_names(&src) {
            violations.push(format!("{rel}:{line}  {text}"));
        }
    }

    assert!(
        violations.is_empty(),
        "生产文件里出现了 {} / {} 的接线。生产组装必须显式传 \
         OwnerMatchAuthorizer::shared()（见 packages/runtime/src/gateway/owner_binding.rs \
         模块头的「接线要求」一节）；恒定放行的替身会让归属闸门静默失效。命中：\n{}",
        NEVER_WIRE_THESE[0],
        NEVER_WIRE_THESE[1],
        violations.join("\n")
    );

    // 覆盖面由 the_wiring_scan_actually_covers_the_gateway_and_the_host 钉住。
    assert!(
        scanned > 100,
        "只判定了 {scanned} 个生产文件，守卫的覆盖面小到没有意义"
    );
}

/// 完整性守卫的反面：证明「今天全绿」是**真的扫过了**，不是没扫到。
#[test]
fn the_wiring_scan_actually_covers_the_gateway_and_the_host() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    let visited: Vec<String> = files
        .iter()
        .map(|path| {
            path.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    for required in MUST_VISIT {
        assert!(
            visited.iter().any(|seen| seen == required),
            "{required} 没被扫描到"
        );
    }
    assert!(
        visited.len() > 500,
        "只扫到 {} 个 .rs，仓库根或剪枝规则有问题",
        visited.len()
    );

    // 住在生产目录里、实为单测的两个文件必须被判成测试，否则这条守卫早就红了。
    for test_module in [
        "packages/runtime/src/gateway/facade_support.rs",
        "packages/runtime/src/gateway/facade_tests.rs",
    ] {
        assert!(
            is_test_only(Path::new(test_module), &root),
            "{test_module} 应被判成测试代码"
        );
    }
}

/// 豁免不能腐化：被豁免的那个文件必须**真的还在定义**这两个类型。
#[test]
fn the_definition_site_exemption_is_still_a_real_definition() {
    let src = fs::read_to_string(repo_root().join(DEFINITION_SITE)).expect("定义点文件必须存在");
    let code = executable_code(&src);
    for name in NEVER_WIRE_THESE {
        assert!(
            code.contains(&format!("pub struct {name}")),
            "{DEFINITION_SITE} 不再定义 {name}，豁免应当撤掉"
        );
    }
}

/// 反证：这个守卫必须**能**抓住违规，否则它只是一段好看的代码。
#[test]
fn the_wiring_guard_catches_a_planted_production_wiring() {
    let planted = r#"
use datazen_runtime::gateway::AlwaysAllow;
/// 文档里提到 AlwaysAllow 不该被算作使用。
pub fn wire(port: Arc<dyn SessionPort>, store: Arc<dyn IdempotencyStore>) -> ExecutionGateway {
    ExecutionGateway::new(port, Arc::new(AlwaysAllow), store, clock)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn also_mentions_AlwaysDeny_in_a_cfg_test_block() {
        let _ = Arc::new(AlwaysDeny);
    }
}
"#;
    let hits = wired_names(planted);
    assert_eq!(
        hits.len(),
        1,
        "种下的违规必须被且仅被抓到一次，实际：{hits:?}"
    );
    assert!(
        hits[0].1.contains("Arc::new(AlwaysAllow)"),
        "抓到的是哪一行不对：{hits:?}"
    );

    let honest = r#"
use datazen_runtime::gateway::owner_binding::OwnerMatchAuthorizer;
pub fn wire() -> Arc<OwnerMatchAuthorizer> {
    OwnerMatchAuthorizer::shared()
}
"#;
    assert!(
        wired_names(honest).is_empty(),
        "正确的接线被误报了：{:?}",
        wired_names(honest)
    );
}

/// 反证之反证：注释、字符串与 `#[cfg(test)]` 内联块里的提及都**不该**被抓。
#[test]
fn the_wiring_guard_ignores_prose_and_test_only_mentions() {
    let benign = r#"
// AlwaysAllow 出现在行注释里
/// AlwaysDeny 出现在文档注释里
const DOC_URL: &str = "https://example.invalid/AlwaysAllow";
#[cfg(test)]
mod inner_tests {
    pub fn helper() -> Arc<AlwaysDeny> {
        Arc::new(AlwaysDeny)
    }
}
pub fn real_production_body() -> usize {
    42
}
"#;
    assert!(
        wired_names(benign).is_empty(),
        "只提及、未接线的写法被误报了：{:?}",
        wired_names(benign)
    );
}
