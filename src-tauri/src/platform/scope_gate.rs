//! 宿主侧的**组织作用域形态中立闸门**：宿主**生产代码**里不得直读桌面的
//! 安装级组织常量。
//!
//! # 这条闸门为什么存在
//!
//! 桌面的组织是**安装级常量**（`LOCAL_ORGANIZATION_ID`，见
//! [`super::identity::LOCAL_ORGANIZATION_ID`]）。桌面形态下直读它没有错——
//! 桌面上只有一个组织，读到的和走正规入口读到的**逐字相同**。
//!
//! 但在 web / 多租户服务端形态下它是**越组织读写**：服务端会拿「桌面的组织」
//! 去查「服务端的多租户存储」。这不是性能问题——按 CM-05，跨组织访问一律按
//! 「**不存在**」处理，而这里是「**用错组织查**」：查不到只是运气好，查得到
//! 就是真的越界了。
//!
//! 所以宿主里凡是能拿到**请求上下文**或**宿主状态**的地方，一律走
//! `OrganizationScope::from_context(&ctx)` / `AppState::organization_scope()`；
//! 只有**桌面专有**的后台路径才允许直读常量，且必须逐条登记在下面的白名单里，
//! 并写明「为什么这里是桌面专有路径」。
//!
//! # 规则（G1）
//!
//! 宿主 `src-tauri/src/` 下**生产代码**里的每一次
//! `local_organization_scope()` / `desktop_organization_scope()` 直读，都必须：
//!
//! 1. 在所在行**紧邻上方的注释块**里带上 `scope-gate: <id>` 标记；
//! 2. `<id>` 出现在本文件的 [`WHITELIST`] 里，且带一句「为什么这里是桌面专有路径」。
//!
//! 白名单里的 id 也必须**都被用上**：登记了却没有对应站点，等于白名单在骗自己。
//!
//! # 每条规则自带 kill test 与自证
//!
//! 只做匹配而不证明「它抓得到东西」的闸门比没有闸门更危险（它给出虚假保证）。
//! 故本文件有四类用例：
//!
//! * **kill test**：把**植入的违规直读**（没有标记的直读）喂给同一套判据，断言转红；
//! * **不误伤**：带合法标记的直读、以及测试区里的直读，断言不转红；
//! * **自证**：断言扫描器**确实看得见**白名单里的那些站点——否则「没发现违规」
//!   只意味着它什么也没看见；
//! * **等价性**：断言宿主正规入口解析出的组织**仍然是** `LOCAL_ORGANIZATION_ID`
//!   （桌面行为逐字不变）。
//!
//! # 闸门的边界（不留模糊措辞）
//!
//! * 它是**测试期的机械闸门**，不是类型系统保证：它查的是**谁在直读常量**，
//!   管不了运行时行为。
//! * 它只扫宿主 crate 的 `src/`。**共享包（`packages/**`）不在范围内**——
//!   那边另有 `scripts/check-platform-crate-boundaries.mjs` 管边界，
//!   而共享包**本来就不该**出现桌面常量。
//! * 它**不引入任何进程级全局状态**：全部是纯函数 + 一个 `const` 白名单。
//! * 蓄意对抗（把函数名拆成 `concat!`、用 `#[cfg]` 藏起来）挡不住。

use std::path::{Path, PathBuf};

/// 被闸门盯着的两个直读入口的名字。
///
/// **刻意写成字符串常量**而不是路径里的标识符：闸门扫的是**代码层**
/// （注释与字符串字面量已抹掉），本文件自己若在代码里写出这两个名字，
/// 就会被自己的规则点红。
const LOCAL_SCOPE_NAME: &str = concat!("local_", "organization_scope");
const DESKTOP_SCOPE_NAME: &str = concat!("desktop_", "organization_scope");

/// 站点标记的前缀：直读所在行**紧邻上方**的注释块里必须带它。
const MARKER: &str = concat!("scope-", "gate:");

/// 显式白名单：`(标记 id, 为什么这里是桌面专有路径)`。
///
/// **每一条都必须写明理由**——「以后再想」不是理由。
const WHITELIST: &[(&str, &str)] = &[
    (
        "desktop-composition-root",
        "桌面形态的**组装根**：`AppState` 的组织作用域在启动期确定一次。\
         这里是「本形态是哪个组织」的**唯一**答案所在——别的形态（多租户服务端）\
         把这一行换成「按请求解析组织」即可，请求期的所有调用点不用动。\
         它必须直读常量：组装期没有请求上下文，也没有宿主状态可问（宿主状态正是\
         由这一行组装出来的）。",
    ),
    (
        "desktop-background-timer",
        "桌面 monitor 的**定时任务**不在请求周期内：它由宿主自己的时钟驱动，\
         跑的是桌面本地的那批 widget。而且 `MonitorEngine` 在 `AppState` **之前**\
         就被构造（`AppState` 持有 monitor engine），此刻没有宿主状态可问。\
         因此这里保留桌面的安装级常量，并逐条登记在本白名单里。",
    ),
];

/// 一处直读。
#[derive(Debug, Clone)]
struct DirectRead {
    /// 相对 `src-tauri/src` 的路径（正斜杠分隔）。
    file: String,
    /// 1 基行号。
    line: usize,
    /// 该行**紧邻上方**的注释块里带的标记 id。
    marker: Option<String>,
}

impl DirectRead {
    fn describe(&self) -> String {
        format!(
            "{}:{}  {}",
            self.file,
            self.line,
            self.marker
                .as_deref()
                .map(|id| format!("[scope-gate: {id}]"))
                .unwrap_or_else(|| "[无标记]".to_string())
        )
    }
}

// ─────────────────────────── 判据 ───────────────────────────

/// 宿主 crate 里被扫的子树（相对 crate 根）。
const SRC: &str = "src";

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
fn cfg_test_items(code: &str) -> Vec<CfgTestItem> {
    let bytes = code.as_bytes();
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
            let attr = &trimmed[..end];
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
    let _ = bytes;
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

/// 抹掉注释与字符串 / 字符字面量，**保留下标与换行**。
///
/// 判据只认**代码**：注释里的散文不是契约，字符串字面量里的名字也不是调用点。
fn blank(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        // 行注释
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
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    continue;
                }
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    out.push(b' ');
                    out.push(b' ');
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
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    if bytes.get(i + 1) == Some(&b'\n') {
                        out.push(b' ');
                        out.push(b'\n');
                    } else {
                        out.push(b' ');
                        out.push(b' ');
                    }
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
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

/// 代码层里所有**直读**的字节下标（不含函数**定义**处）。
fn direct_read_offsets(code: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for name in [LOCAL_SCOPE_NAME, DESKTOP_SCOPE_NAME] {
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(name) {
            let at = from + rel;
            from = at + name.len();
            if at > 0 && is_ident_byte(code.as_bytes()[at - 1]) {
                continue;
            }
            let after = &code[at + name.len()..];
            if !after.trim_start().starts_with('(') {
                continue;
            }
            // 定义处（`fn <名字>(`）不是「直读」。
            let before = code[..at].trim_end();
            let last_word = before
                .rsplit(|c: char| c.is_whitespace())
                .next()
                .unwrap_or_default();
            if last_word == "fn" {
                continue;
            }
            out.push(at);
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// 一个文件里**全部**的直读（含测试区）。
fn all_direct_reads(rel: &str, source: &str) -> Vec<DirectRead> {
    let code = blank(source);
    direct_read_offsets(&code)
        .into_iter()
        .map(|at| {
            // 行号取自**抹串后**的代码层：抹串保留了换行，行号因此一致；
            // 而抹串后的文本全是 ASCII，按字节切片不会切在字符中间。
            let line = 1 + code[..at].matches('\n').count();
            DirectRead {
                file: rel.to_string(),
                line,
                marker: marker_above(source, line),
            }
        })
        .collect()
}

/// 一个文件里**生产区**的直读（排除 `#[cfg(test)]` 项体与测试专用文件）。
fn production_direct_reads(rel: &str, source: &str, test_only_file: bool) -> Vec<DirectRead> {
    if test_only_file {
        return Vec::new();
    }
    let code = blank(source);
    let regions: Vec<(usize, usize)> = cfg_test_items(&code)
        .into_iter()
        .filter_map(|item| item.body)
        .collect();
    direct_read_offsets(&code)
        .into_iter()
        .filter(|at| !regions.iter().any(|(s, e)| *at >= *s && *at <= *e))
        .map(|at| {
            let line = 1 + code[..at].matches('\n').count();
            DirectRead {
                file: rel.to_string(),
                line,
                marker: marker_above(source, line),
            }
        })
        .collect()
}

/// 直读所在行**紧邻上方**的注释块里的标记 id。
///
/// 从直读行的**上一行**往上走，只收连续的 `//` 注释行——注释块与直读之间
/// 隔了代码，就不算「紧邻上方」。
fn marker_above(source: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    // 直读行（1 基）的上一行的 0 基下标。
    let mut idx = line.checked_sub(2)?;
    let mut block: Vec<&str> = Vec::new();
    while let Some(text) = lines.get(idx) {
        let trimmed = text.trim();
        if !trimmed.starts_with("//") {
            break;
        }
        block.push(trimmed);
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    for comment in block {
        let Some((_, rest)) = comment.split_once(MARKER) else {
            continue;
        };
        let id = rest.split_whitespace().next().unwrap_or_default();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

/// 扫描宿主 `src/`：**生产代码**里的全部直读。
fn scan_production_reads() -> Vec<DirectRead> {
    let files = source_files_on_disk();
    // 第一遍：收集被 `#[cfg(test)] mod …;` 包含的外部测试文件。
    let mut included: Vec<String> = Vec::new();
    for rel in &files {
        let source = read_source(rel);
        let code = blank(&source);
        for item in cfg_test_items(&code) {
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
    // 第二遍：生产区的直读。
    let mut out = Vec::new();
    for rel in &files {
        let source = read_source(rel);
        out.extend(production_direct_reads(
            rel,
            &source,
            is_test_only_path(rel, &included),
        ));
    }
    out
}

/// 白名单里的标记 id。
fn whitelist_ids() -> Vec<&'static str> {
    WHITELIST.iter().map(|(id, _)| *id).collect()
}

// ─────────────────────────── 用例 ───────────────────────────

/// **kill test**：植入一处**没有标记**的直读，判据必须转红。
///
/// 判据与扫描用的是同一套代码（[`production_direct_reads`] +
/// [`marker_above`]），所以这条用例证明的是「闸门真的抓得到」，
/// 而不是「另写了一个恰好也能抓到的检查」。
#[test]
fn the_gate_catches_a_planted_unmarked_direct_read() {
    let planted = "fn f() {\n    let scope = crate::platform::local_organization_scope();\n}\n";
    let reads = production_direct_reads("planted.rs", planted, false);
    assert_eq!(
        reads.len(),
        1,
        "闸门没抓到植入的直读 —— 规则形同虚设（实得 {reads:?}）"
    );
    assert!(
        reads[0].marker.is_none(),
        "植入的直读本该没有标记（实得 {:?}）",
        reads[0].marker
    );
    assert!(
        !whitelist_ids().contains(&reads[0].marker.as_deref().unwrap_or("")),
        "植入的直读没有标记，因此不可能落在白名单里"
    );
}

/// **不误伤**：带合法标记的直读、以及**测试区**里的直读，都不转红。
#[test]
fn the_gate_does_not_flag_a_marked_read_or_a_read_in_a_test_region() {
    let marked = "fn f() {\n    // scope-gate: desktop-composition-root\n    let s = crate::platform::local_organization_scope();\n}\n";
    let reads = production_direct_reads("marked.rs", marked, false);
    assert_eq!(reads.len(), 1);
    assert_eq!(
        reads[0].marker.as_deref(),
        Some("desktop-composition-root"),
        "带标记的直读必须能取到标记"
    );

    // 测试区里的直读不算生产直读。
    let in_test = "fn f() {}\n#[cfg(test)]\nmod tests {\n    fn g() {\n        let s = crate::platform::local_organization_scope();\n    }\n}\n";
    assert!(
        production_direct_reads("in_test.rs", in_test, false).is_empty(),
        "测试区里的直读被当成生产直读了 —— 判据必须先分清测试区"
    );
    assert!(
        all_direct_reads("in_test.rs", in_test).len() == 1,
        "扫描器必须**看得见**测试区里的直读，否则上面的「不转红」只是因为它瞎"
    );

    // 测试专用文件里的直读也不算生产直读。
    assert!(
        production_direct_reads("tests/whatever.rs", marked, true).is_empty(),
        "测试专用文件里的直读被当成生产直读了"
    );
}

/// **自证**：扫描器确实看得见白名单里的那些站点。
///
/// 没有这一条，「没发现违规」只意味着扫描器什么也没看见。
#[test]
fn the_scanner_actually_sees_the_whitelisted_sites() {
    let reads = scan_production_reads();
    let ids = whitelist_ids();
    for id in &ids {
        assert!(
            reads.iter().any(|r| r.marker.as_deref() == Some(*id)),
            "扫描器没在生产代码里看到标记为 {id} 的站点 —— \
             闸门会安静地少扫一片（实得 {}）",
            reads
                .iter()
                .map(DirectRead::describe)
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    // 扫描面本身不能是空的：至少要看得见白名单里的那些站点。
    assert!(
        reads.len() >= ids.len(),
        "生产直读只有 {} 处，比白名单还少 —— 扫描多半是瞎的",
        reads.len()
    );
    // 白名单里点名的两个文件必须真的被扫到。
    for required in ["bootstrap/app_state.rs", "monitor/engine.rs"] {
        assert!(
            reads.iter().any(|r| r.file == required),
            "闸门没覆盖 {required}（实得 {}）",
            reads
                .iter()
                .map(DirectRead::describe)
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

/// **主规则**：生产代码里的每一处直读都必须落在显式白名单内。
#[test]
fn every_production_direct_read_is_on_the_allow_list() {
    let reads = scan_production_reads();
    let ids = whitelist_ids();
    let mut bad: Vec<String> = Vec::new();
    for read in &reads {
        match read.marker.as_deref() {
            None => bad.push(format!(
                "{} —— 直读桌面的安装级组织常量却没有 `scope-gate:` 标记。\
                 请改用 `AppState::organization_scope()`（或 `OrganizationScope::from_context`），\
                 确有桌面专有理由的，写清理由并登记进白名单。",
                read.describe()
            )),
            Some(id) if !ids.contains(&id) => bad.push(format!(
                "{} —— 标记 {id} 不在白名单里（白名单：{:?}）",
                read.describe(),
                ids
            )),
            Some(_) => {}
        }
    }
    assert!(
        bad.is_empty(),
        "宿主生产代码里有未登记的组织作用域直读：\n{}",
        bad.join("\n")
    );
}

/// **白名单不许养闲条目**：登记了却没有站点，等于白名单在骗自己。
#[test]
fn every_allow_list_entry_is_actually_used() {
    let reads = scan_production_reads();
    let mut unused: Vec<String> = Vec::new();
    for (id, reason) in WHITELIST {
        assert!(
            !reason.trim().is_empty(),
            "白名单条目 {id} 没写「为什么这里是桌面专有路径」"
        );
        if !reads.iter().any(|r| r.marker.as_deref() == Some(*id)) {
            unused.push((*id).to_string());
        }
    }
    assert!(
        unused.is_empty(),
        "白名单里有登记了却没有对应站点的条目：{unused:?} —— \
         站点被改掉了就把条目删掉，别让白名单变成许愿池"
    );
}

/// **等价性**：宿主正规入口解析出的组织**仍然是** `LOCAL_ORGANIZATION_ID`。
///
/// 这是「桌面行为逐字不变」的实测证据：把直读换成走正规入口之后，
/// 桌面上解析出来的组织**逐字**没变。
#[tokio::test]
async fn the_host_scope_entry_still_resolves_to_the_local_organization() {
    let test = crate::testing::app_state::TestAppState::new().await;
    let scope = test.state.organization_scope().unwrap();
    assert_eq!(
        scope.organization_id().as_str(),
        super::LOCAL_ORGANIZATION_ID,
        "宿主正规入口解析出的组织变了 —— 桌面的行为必须逐字不变"
    );
    assert_eq!(
        scope.organization_id(),
        super::local_organization_scope().organization_id(),
        "正规入口与桌面的安装级常量必须解析出同一个组织"
    );
}
