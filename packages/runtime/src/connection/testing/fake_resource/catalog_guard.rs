//! 注入点目录的三源对撞：文档表格 ↔ `FaultKind` 的真实编号/阶段。
//!
//! 为什么不能用「手写一份 F 编号数组」当守门测试：那只证明了代码**自己**的顺序自洽，
//! 文档与代码同时错成同一个值时它照样全绿。本模块因此在**测试运行时**读取三个真源：
//!
//! | 侧 | 真源 | 解析出什么 |
//! |---|---|---|
//! | 文档一 | 注入点目录表格 | 每行的 `F<n>` / 阶段 / 注入类型（单元格里的反引号标识） |
//! | 文档二 | 「阶段 × 故障 × 期望可观察结果」表格 | 每行开头的 `F<n>` 与期望结果里点名的变体 |
//! | 代码 | `script.rs` 自身的源码 | 每个 `FaultKind` 变体的 `/// F<n>：`、`/// 目录归属：<阶段>` 与 `catalog_id()` 的 `match` 臂 |
//!
//! 三条腿都要成立：
//!
//! - **命名腿（第一张表）**：文档行里反引号包住的标识，只要**恰好等于**某个 `FaultKind` 变体名，
//!   就必须落在该行自己的编号上（`RollbackFailed` 在 F8 行 → `catalog_id` 必须是 `F8`）。
//! - **阶段腿（第一张表）**：变体自报的 `目录归属：<阶段>` 必须等于该编号所在行的「阶段」单元格
//!   （在第一个全角 `（` 处截断），并且两侧的编号集合都恰好是 F1–F12。
//! - **可观察腿（第二张表）**：可观察结果是**独立演进的另一张表**。它每行开头的 `F<n>` 加上期望
//!   结果里点名的变体，构成第三真源；只改第一张表 + 代码（漏改第二张表）在这里立刻打红。
//!
//! 读不到、解析不出、形状不对，一律 `panic!` 并带上 `file:line`：
//! **绝不静默跳过** —— 一个会「跳过」的守门测试等于没有守门测试。
//!
//! 已知上限（设计的必然，不是缺陷）：把 **两张表格与代码三处同时**改成同一个错误值，
//! 本测试仍会绿 —— 任何「文档 ↔ 代码」对撞都躲不开这一点，除非再引入文档之外的第四真源。
//! 本模块的职责是把「单侧/双侧漏改」全部打红，并用非空断言保证解析没退化成空循环。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// 注入点目录表格的编号全集。缺行、多行、编号重复都会打红。
const EXPECTED_IDS: [&str; 12] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

/// 命名腿至少要命中这么多个变体：注入点目录用变体名点名的那几行。
const MIN_NAMED_HITS: usize = 4;

/// 可观察腿至少要命中这么多个变体：可观察结果表点名的四处。
/// （F2 的 `ResourceBusy`、F10 的 `Clean` 属于 `ProviderError`/`ResetOutcome`，不是 `FaultKind`，
/// 按设计被忽略。）
const MIN_OBSERVABLE_HITS: usize = 4;

/// 守门测试的入口：解析三侧并逐条对撞。
pub(crate) fn assert_catalogue_matches_doc() {
    let spec = Spec::read();
    let code = CodeCatalogue::read();
    assert_non_vacuous(&spec, &code);
    assert_named_variants_match_their_row(&spec, &code);
    assert_stage_and_id_set_match(&spec, &code);
    assert_observable_rows_name_the_same_numbers(&spec, &code);
}

/// 两侧解析出来的东西都非空，且每个期望编号都**恰好**有一个文档行。
fn assert_non_vacuous(spec: &Spec, code: &CodeCatalogue) {
    let first_line = spec.rows.first().map_or(1, |row| row.line);
    assert!(
        spec.rows.len() == EXPECTED_IDS.len(),
        "{}:{}: 注入点目录解析出 {} 行，期望 {} 行 —— 解析多半已经走样",
        spec.path.display(),
        first_line,
        spec.rows.len(),
        EXPECTED_IDS.len(),
    );
    assert!(
        !code.variants.is_empty(),
        "{}:{}: `FaultKind` 一个变体都没解析出来 —— 括号扫描失配",
        code.path.display(),
        code.enum_line,
    );
    for id in EXPECTED_IDS {
        let found: Vec<&DocRow> = spec.rows.iter().filter(|row| row.id == id).collect();
        assert_eq!(
            found.len(),
            1,
            "{}:{}: 编号 {id} 应当出现恰好一行，实际 {} 行",
            spec.path.display(),
            first_line,
            found.len(),
        );
    }
}

/// 命名腿：文档行反引号里的标识 == 变体名 ⇒ `catalog_id` 必须等于该行的编号。
///
/// 命中数本身就是非空证据，所以一并断言下限 —— 文档不再用变体名点名时这里必须打红，
/// 而不是「一个都没命中所以通过」。
fn assert_named_variants_match_their_row(spec: &Spec, code: &CodeCatalogue) {
    let mut hits: Vec<String> = Vec::new();
    for row in &spec.rows {
        for token in backticked_tokens(&row.injection) {
            let Some(variant) = code.variants.iter().find(|it| it.name == token) else {
                continue;
            };
            assert_eq!(
                variant.catalog_id,
                row.id,
                "{}:{} 的 `{token}` 属于 {}，但 `FaultKind::{}::catalog_id()` 返回 {}（{}:{}）",
                spec.path.display(),
                row.line,
                row.id,
                variant.name,
                variant.catalog_id,
                code.path.display(),
                variant.id_line,
            );
            hits.push(format!("{}→{}", variant.name, variant.catalog_id));
        }
    }
    assert!(
        hits.len() >= MIN_NAMED_HITS,
        "{}:{}: 命名腿只命中 {} 个变体（≥{} 才说明注入点目录仍用变体名点名）：{:?}；\\
         若这是有意的文档改写，请同步更新本模块的 MIN_NAMED_HITS 并说明理由",
        spec.path.display(),
        spec.rows[0].line,
        hits.len(),
        MIN_NAMED_HITS,
        hits,
    );
}

/// 阶段腿：变体的 `目录归属` == 该编号所在行的阶段；两侧编号集合都 == F1–F12。
fn assert_stage_and_id_set_match(spec: &Spec, code: &CodeCatalogue) {
    let mut code_ids: BTreeSet<&str> = BTreeSet::new();
    for variant in &code.variants {
        assert_eq!(
            variant.doc_id,
            variant.catalog_id,
            "{}:{}: `FaultKind::{}` 的注释编号是 {}，`catalog_id()` 却是 {}（{}:{}）",
            code.path.display(),
            variant.line,
            variant.name,
            variant.doc_id,
            variant.catalog_id,
            code.path.display(),
            variant.id_line,
        );
        let row = spec
            .rows
            .iter()
            .find(|row| row.id == variant.doc_id)
            .unwrap_or_else(|| {
                panic!(
                    "{}:{}: 代码自报编号 {}，但注入点目录里没有这一行",
                    code.path.display(),
                    variant.line,
                    variant.doc_id
                )
            });
        assert_eq!(
            variant.stage,
            row.stage,
            "{}:{}: `FaultKind::{}` 自报「目录归属：{}」，而 {} 行（文档第 {} 行）的阶段是「{}」",
            code.path.display(),
            variant.line,
            variant.name,
            variant.stage,
            row.id,
            row.line,
            row.stage,
        );
        code_ids.insert(variant.catalog_id.as_str());
    }
    let doc_ids: BTreeSet<&str> = spec.rows.iter().map(|row| row.id.as_str()).collect();
    let expected: BTreeSet<&str> = EXPECTED_IDS.iter().copied().collect();
    assert_eq!(
        doc_ids,
        expected,
        "{}: 注入点目录的编号集合与代码里的期望不一致",
        spec.path.display(),
    );
    assert_eq!(
        code_ids,
        expected,
        "{}: `FaultKind::catalog_id()` 覆盖的编号集合与 F1–F12 不一致",
        code.path.display(),
    );
}

/// 可观察腿（第三真源）：可观察结果是**另一张表**，与注入点目录独立演进。
///
/// 只改注入点目录 + 代码而漏改可观察结果表的改法，在这里必红 —— 这是「双侧同错」唯一的现实缺口。
fn assert_observable_rows_name_the_same_numbers(spec: &Spec, code: &CodeCatalogue) {
    let mut hits: Vec<String> = Vec::new();
    for row in &spec.observables {
        // 两列分别扫：跨列拼接可能把一个未闭合的反引号和另一个单元格的反引号配成一对。
        let tokens = backticked_tokens(&row.injection)
            .into_iter()
            .chain(backticked_tokens(&row.expectation));
        for token in tokens {
            let Some(variant) = code.variants.iter().find(|it| it.name == token) else {
                continue;
            };
            assert_eq!(
                variant.catalog_id, row.id,
                "{}:{} 的期望结果点名 `{}`，该行为 {}，但 `FaultKind::{}::catalog_id()` 返回 {}（{}:{}）",
                spec.path.display(),
                row.line,
                token,
                row.id,
                variant.name,
                variant.catalog_id,
                code.path.display(),
                variant.id_line,
            );
            hits.push(format!("{}→{}", variant.name, variant.catalog_id));
        }
    }
    assert!(
        hits.len() >= MIN_OBSERVABLE_HITS,
        "{}:{}: 可观察腿只命中 {} 个变体（≥{} 才说明可观察结果表仍用变体名点名）：{:?}；\\
         若这是有意的文档改写，请同步更新本模块的 MIN_OBSERVABLE_HITS 并说明理由",
        spec.path.display(),
        spec.observables[0].line,
        hits.len(),
        MIN_OBSERVABLE_HITS,
        hits,
    );
}

// ---------------------------------------------------------------- 文档侧解析

/// 注入点目录的一行：`| F<n> | 阶段 | 注入类型 |`。
struct DocRow {
    id: String,
    /// 阶段单元格截到第一个全角 `（` 之前的部分（`描述（\`describeResource\`）` → `描述`）。
    stage: String,
    injection: String,
    line: usize,
}

/// 可观察结果表的一行：`| F<n> <描述> | 期望宿主可观察结果 | 关联 CM |`。
struct ObsRow {
    /// 注入单元格开头的 `F<n>`。
    id: String,
    /// 变体名散落在**两个**单元格里（`F11 \`CloseUnconfirmed\``、`F8 rollback 失败` +
    /// 期望列里的 `` `RollbackFailed` ``），所以两列都要扫。
    injection: String,
    expectation: String,
    line: usize,
}

struct Spec {
    path: PathBuf,
    rows: Vec<DocRow>,
    observables: Vec<ObsRow>,
}

impl Spec {
    fn read() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/architecture/platform/fake-runtime-fixtures.md");
        let text = fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!(
                "{}: 读不到注入点目录真源（{err}）—— 守门测试不得跳过",
                path.display()
            )
        });
        let rows = parse_section_41(&text, &path);
        let observables = parse_section_42(&text, &path);
        Self {
            path,
            rows,
            observables,
        }
    }
}

/// 可观察结果表的注入单元格形如 ``F8 rollback 失败`` ⇒ `F8`；形如 ``F10 driver `Clean` 但句柄非空``
/// 里的反引号要留给 `backticked_tokens`，所以这里只切**第一个空格**之前的部分。
fn parse_section_42(text: &str, path: &Path) -> Vec<ObsRow> {
    let lines: Vec<&str> = text.lines().collect();
    let heading_line = lines
        .iter()
        .position(|line| line.trim_start().starts_with("### 4.2"))
        .unwrap_or_else(|| {
            panic!("{}: 找不到 `### 4.2` 小节标题", path.display());
        });
    let table_line = (heading_line + 1..lines.len())
        .find(|index| lines[*index].trim_start().starts_with("| 注入"))
        .unwrap_or_else(|| {
            panic!(
                "{}:{}: `### 4.2` 小节里找不到以 `| 注入` 开头的表头",
                path.display(),
                heading_line + 1,
            )
        });
    let separator = table_line + 1;
    assert!(
        lines[separator].trim().starts_with("| ---"),
        "{}:{}: 可观察结果表表头下面应当是 `| --- | --- | --- |` 分隔行，实际是 `{}`",
        path.display(),
        separator + 1,
        lines[separator].trim(),
    );
    let mut rows: Vec<ObsRow> = Vec::new();
    for (offset, line) in lines[separator + 1..].iter().enumerate() {
        let data = line.trim();
        if !data.starts_with('|') {
            break;
        }
        let number = separator + 2 + offset;
        let cells: Vec<&str> = data
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        assert_eq!(
            cells.len(),
            3,
            "{}:{}: 可观察结果表数据行应当是 3 个单元格，实际 {} 个：`{}`",
            path.display(),
            number,
            cells.len(),
            data,
        );
        let id = cells[0]
            .split(char::is_whitespace)
            .next()
            .unwrap_or(cells[0]);
        assert!(
            is_f_id(id),
            "{}:{}: 可观察结果表行的注入列应当以 F<n> 开头，实际 `{}`",
            path.display(),
            number,
            cells[0],
        );
        rows.push(ObsRow {
            id: id.to_owned(),
            injection: cells[0].to_owned(),
            expectation: cells[1].to_owned(),
            line: number,
        });
    }
    assert!(
        !rows.is_empty(),
        "{}:{}: 可观察结果表一行都没解析出来",
        path.display(),
        separator + 2,
    );
    rows
}

/// 定位 `### 4.1` 小节里的表格并逐行解析。任何形状不对的地方都带 `file:line` 打红。
fn parse_section_41(text: &str, path: &Path) -> Vec<DocRow> {
    let lines: Vec<&str> = text.lines().collect();
    let header_line = lines
        .iter()
        .position(|line| line.trim_start().starts_with("### 4.1"))
        .unwrap_or_else(|| {
            panic!("{}: 找不到 `### 4.1` 小节标题", path.display());
        });
    let mut table_line: Option<usize> = None;
    // 从标题的**下一行**开始：小节首行本身就是 `#` 开头，不能用它来判断小节结束。
    for (offset, line) in lines[header_line + 1..].iter().enumerate() {
        if line.trim_start().starts_with('#') {
            break;
        }
        if line.trim_start().starts_with("| 编号") {
            table_line = Some(header_line + 1 + offset);
            break;
        }
    }
    let Some(table_line) = table_line else {
        panic!(
            "{}:{}: `### 4.1` 小节里找不到以 `| 编号` 开头的表头",
            path.display(),
            header_line + 1,
        );
    };
    let separator = table_line + 1;
    assert!(
        lines[separator].trim().starts_with("| ---"),
        "{}:{}: 注入点目录表头下面应当是 `| --- | --- | --- |` 分隔行，实际是 `{}`",
        path.display(),
        separator + 1,
        lines[separator].trim(),
    );
    let mut rows: Vec<DocRow> = Vec::new();
    let mut cursor = separator + 1;
    while cursor < lines.len() {
        let line = lines[cursor];
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || trimmed.starts_with('#') {
            break;
        }
        let number = cursor + 1;
        let cells: Vec<String> = trimmed
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_owned())
            .collect();
        assert_eq!(
            cells.len(),
            3,
            "{}:{}: 注入点目录数据行应当是 3 个单元格，实际 {} 个：`{}`",
            path.display(),
            number,
            cells.len(),
            trimmed,
        );
        assert!(
            is_f_id(&cells[0]),
            "{}:{}: 注入点目录行的编号列应当形如 F12，实际 `{}`",
            path.display(),
            number,
            cells[0],
        );
        rows.push(DocRow {
            id: cells[0].clone(),
            stage: normalize_stage(&cells[1]),
            injection: cells[2].clone(),
            line: number,
        });
        cursor += 1;
    }
    assert!(
        !rows.is_empty(),
        "{}: 注入点目录一行都没解析出来",
        path.display()
    );
    rows
}

fn is_f_id(cell: &str) -> bool {
    let Some(digits) = cell.strip_prefix('F') else {
        return false;
    };
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// `描述（\`describeResource\`）` → `描述`。全角 `（` 之后是注解，不参与阶段归属。
fn normalize_stage(cell: &str) -> String {
    cell.split('（').next().unwrap_or(cell).trim().to_owned()
}

/// 取出单元格里的反引号标识：`失败 / \`UnsupportedPlan\`` → `["UnsupportedPlan"]`。
fn backticked_tokens(cell: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = cell;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else {
            break;
        };
        tokens.push(after[..close].trim().to_owned());
        rest = &after[close + 1..];
    }
    tokens
}

// ---------------------------------------------------------------- 代码侧解析

/// 一个 `FaultKind` 变体：源码行号、注释编号、注释阶段、`catalog_id()` 的真值。
struct CodeVariant {
    name: String,
    line: usize,
    doc_id: String,
    stage: String,
    catalog_id: String,
    id_line: usize,
}

struct CodeCatalogue {
    path: PathBuf,
    enum_line: usize,
    variants: Vec<CodeVariant>,
}

impl CodeCatalogue {
    fn read() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/connection/testing/fake_resource/script.rs");
        let text = fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!(
                "{}: 读不到 `FaultKind` 源码（{err}）—— 守门测试不得跳过",
                path.display()
            )
        });
        let lines: Vec<&str> = text.lines().collect();
        let enum_line = lines
            .iter()
            .position(|line| line.trim_start().starts_with("pub enum FaultKind"))
            .unwrap_or_else(|| {
                panic!("{}: 找不到 `pub enum FaultKind`", path.display());
            });
        let ids = parse_catalog_ids(&lines, &path, enum_line);
        let variants = parse_variants(&lines, &path, enum_line, &ids);
        Self {
            path,
            enum_line: enum_line + 1,
            variants,
        }
    }
}

/// 变体名 → (`catalog_id()` 里的编号字面量, 臂所在行号)。
type IdMap = BTreeMap<String, (String, usize)>;

/// 扫 `pub enum FaultKind { … }` 的函数体：按行分类，收集每个变体的两行属性。
fn parse_variants(lines: &[&str], path: &Path, enum_line: usize, ids: &IdMap) -> Vec<CodeVariant> {
    let mut variants: Vec<CodeVariant> = Vec::new();
    let mut doc_id: Option<String> = None;
    let mut stage: Option<String> = None;
    for (offset, line) in lines[enum_line + 1..].iter().enumerate() {
        let number = enum_line + offset + 2; // 1-based
        let trimmed = line.trim();
        if trimmed == "}" {
            break;
        }
        if let Some(comment) = trimmed.strip_prefix("///") {
            let comment = comment.trim();
            if let Some(rest) = comment.strip_prefix("目录归属：") {
                stage = Some(rest.trim().to_owned());
            } else if doc_id.is_none() {
                doc_id = parse_doc_id(comment);
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("#[") {
            continue;
        }
        let Some(name) = leading_type_name(trimmed) else {
            continue;
        };
        let found_id = doc_id.take().unwrap_or_else(|| {
            panic!(
                "{}:{}: `FaultKind::{}` 缺少首行 `/// F<n>：…` 注释",
                path.display(),
                number,
                name
            )
        });
        let found_stage = stage.take().unwrap_or_else(|| {
            panic!(
                "{}:{}: `FaultKind::{}` 缺少 `/// 目录归属：<阶段>` 注释",
                path.display(),
                number,
                name
            )
        });
        let (catalog_id, id_line) = ids.get(&name).cloned().unwrap_or_else(|| {
            panic!(
                "{}:{}: `FaultKind::{}` 在 `catalog_id()` 里没有对应的 `match` 臂",
                path.display(),
                number,
                name
            )
        });
        variants.push(CodeVariant {
            name,
            line: number,
            doc_id: found_id,
            stage: found_stage,
            catalog_id,
            id_line,
        });
    }
    assert!(
        !variants.is_empty(),
        "{}:{}: `FaultKind` 体扫描到 0 个变体",
        path.display(),
        enum_line + 1,
    );
    variants
}

/// `/// F8：回滚失败` → `Some("F8")`；其它注释行 → `None`。
fn parse_doc_id(comment: &str) -> Option<String> {
    let after_f = comment.trim_start().strip_prefix('F')?;
    let digits: String = after_f.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let rest = after_f[digits.len()..].trim_start();
    // 允许 `F8：…` 与 `F8: …` 两种分隔符。
    if !(rest.starts_with('：') || rest.starts_with(':')) {
        return None;
    }
    Some(format!("F{digits}"))
}

/// 变体声明行的名字：`RequiresReplacement { reason: String },` → `RequiresReplacement`。
fn leading_type_name(trimmed: &str) -> Option<String> {
    if !trimmed.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let name: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// 扫 `catalog_id()` 的函数体：变体名 → (`"F<n>"` 字面量, 臂所在行号)。
fn parse_catalog_ids(lines: &[&str], path: &Path, from_line: usize) -> IdMap {
    let anchor = lines
        .iter()
        .position(|line| line.contains("pub const fn catalog_id(&self)"))
        .unwrap_or_else(|| {
            panic!(
                "{}: 找不到 `pub const fn catalog_id(&self)`",
                path.display()
            );
        });
    assert!(
        anchor > from_line,
        "{}:{}: `catalog_id()` 应当定义在 `FaultKind` 之后",
        path.display(),
        anchor + 1,
    );
    let mut ids: IdMap = BTreeMap::new();
    for (offset, line) in lines[anchor..].iter().enumerate() {
        let number = anchor + offset + 1;
        let trimmed = line.trim();
        if trimmed.starts_with('}') {
            break;
        }
        let Some(rest) = trimmed.split("FaultKind::").nth(1) else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let Some(open) = rest.find('"') else {
            continue;
        };
        let Some(close) = rest[open + 1..].find('"') else {
            panic!(
                "{}:{}: `catalog_id()` 的 `{name}` 臂里字符串字面量没闭合",
                path.display(),
                number,
            );
        };
        let literal = &rest[open + 1..open + 1 + close];
        assert!(
            is_f_id(literal),
            "{}:{}: `FaultKind::{name}` 的编号字面量应当形如 F12，实际 `\"{literal}\"`",
            path.display(),
            number,
        );
        assert!(
            ids.insert(name.clone(), (literal.to_owned(), number))
                .is_none(),
            "{}:{}: `FaultKind::{name}` 在 `catalog_id()` 里出现了两条臂",
            path.display(),
            number,
        );
    }
    assert!(
        !ids.is_empty(),
        "{}: `catalog_id()` 一个 `match` 臂都没解析出来",
        path.display(),
    );
    ids
}
