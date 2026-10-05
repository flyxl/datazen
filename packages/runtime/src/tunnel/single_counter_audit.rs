//! CM-32-FU1 的机械闸门：全系统**只允许一份**隧道引用计数，这条铁律不再靠人审计。
//!
//! # 这条闸门为什么存在
//!
//! `transport.rs` 模块头登记过一个**已实证**的名洞（CM-32 repair round 1）：给录写端口
//! `RecordingTunnelTransport` 加一个 `close_tally: Mutex<usize>`、并让 `close_calls()`
//! 改读它 —— 当年编译通过、全轨测试全绿。第二本账就此伪装成第一本账，CM-28
//! 「隧道不多减引用」与 CM-27「许可归零」同时失效却无人报错。当年留了两个候选修法未裁决；
//! 这里把**两个都落成代码**，并补上它们各自都挡不住的那一层：
//!
//! * **R1（台账侧字段单点，= 候选 a）** `TunnelEntry::refs` 的点访问只许出现在四个注册
//!   方法（`established` / `refs` / `add_reference` / `take_reference`）里。
//! * **R2（台账侧按值取数，= 候选 b）** 唯一归零路径 `drain()` 必须经
//!   `take_reference()` 取数，不许直读字段、不许自己做对 `refs` 的减法。
//! * **R3（观测账不得参与判断）** 台账的 `close_calls` 是观测字段，禁止出现在任何
//!   条件 / 比较 / 逻辑表达式里 —— 「观测计数被当成释放依据」是第二本账的另一形态。
//! * **R4（端口不得自存账）** 每个实现 [`TunnelTransport`] 的结构体，其字段类型剥掉
//!   `Mutex`/`RwLock`/`Arc`/`Box`/`Option`/`Cell`/`RefCell` 包装后**不得**是整数标量、
//!   不得含 `Atomic*`；本地新类型（`struct X(…)` / `type X = …`）**递归展开**，
//!   换个壳绕不过去。**这一条直接杀原始反例。**
//! * **R5（读数只能是折法的投影）** 每个含端口实现的被测文件必须**恰好一个**
//!   「事件 → 账」的**纯**折函数（形参含切片、函数体不碰 `self.`），且该文件里所有
//!   返回整数的 `&self` 方法都必须调用它。账因此只能现折、不能落地。
//! * **R6（闸门自己不能被裁小）** `mod.rs` 声明的每个模块与两个隧道测试二进制都必须在
//!   登记表内；`TunnelEntry` 必须保持私有；唯一归零路径必须**只有一条**且住在
//!   [`TunnelLedger`] 的 impl 块里。
//!
//! R1–R3 管台账那一半，R4–R5 管端口那一半。只有 R1 或只有 R4 都挡不住伪装：
//! 反例走的是端口那半边，而「台账多开一个直读字数的口子」走的是台账那半边。
//!
//! # 每条规则都自带 kill test
//!
//! 只做模式匹配而不证明「它抓得到东西」的闸门比没有闸门更危险 —— 它给出虚假保证
//! （同 `tests/cm28_concurrent_tunnel.rs` 里那条负控的历史）。故 R1–R5 各有
//! `…_catch_a_planted_…` 用例，把**原始反例本身**与**同类伪装**当字符串喂给同一套
//! 扫描器并断言转红；同时断言合法形状不被误伤（误伤会把人逼向更隐蔽的写法）。
//! `the_scan_primitives_see_the_shapes_they_claim` 再断言解析器真的看得见台账与夹具
//! 里的形状 —— 否则「没发现违规」只意味着扫描器什么也没看见。
//!
//! # 为什么住在 `src/tunnel/` 而不是顶层 `tests/`
//!
//! 本模块是 `#[cfg(test)] mod single_counter_audit`，因此它随 `--lib` 跑。
//! `mod.rs` 的登记项 F 记着：CI 只跑 `cargo test --lib`，本 crate 顶层那些 `tests/*.rs`
//! 集成二进制**目前不进 CI**。闸门若放在那边，它就只是一份「本地才生效的承诺」，
//! 与它要取代的「审计承诺」同样脆弱。放在 lib 作用域里，伪装每次构建都会被杀。
//!
//! # 这条保证的边界（不留模糊措辞）
//!
//! 它是**测试期的结构闸门**，不是类型系统保证：挡得住自然写出来的第二本账
//! （字段直读、自存计数、绕过折法、观测账参与判断，含 `Mutex<usize>` 与本地新类型壳），
//! 挡不住蓄意对抗（`unsafe` 指针转义、跨 crate 静态、`proc-macro` 生成）。
//! `&self` + 内部可变性在类型层面本来就装得下任意计数，这一点任何扫描都消不掉。
//!
//! **本模块自己也在登记表里**（连同 `source_scan.rs` 基元）：闸门不看自己，
//! 「悄悄少扫一片」就没有反制手段（同 `tests/cm70_panic_redaction_guard.rs` 的自我登记先例）。

use super::source_scan::{
    blank, char_literal_len, enclosing_impl, functions, impl_blocks, line_of, local_decl,
    split_top_level, strip_space, struct_fields, Func, ImplBlock,
};
use super::{TunnelLedger, TunnelTransport};

/// 被审计的源码：(给人看的路径, 编译期嵌进来的文本)。
///
/// 少一个文件就得改这张表，而 `the_audit_registry_covers_every_tunnel_module` 会先把
/// 「`tunnel/mod.rs` 新增了模块却没登记」这种裁小行为点红。
const AUDITED: &[(&str, &str)] = &[
    ("src/tunnel/mod.rs", include_str!("mod.rs")),
    ("src/tunnel/ledger.rs", include_str!("ledger.rs")),
    ("src/tunnel/transport.rs", include_str!("transport.rs")),
    ("src/tunnel/error.rs", include_str!("error.rs")),
    ("src/tunnel/harness.rs", include_str!("harness.rs")),
    ("src/tunnel/source_scan.rs", include_str!("source_scan.rs")),
    (
        "src/tunnel/single_counter_audit.rs",
        include_str!("single_counter_audit.rs"),
    ),
    (
        "src/tunnel/journey_sharing.rs",
        include_str!("journey_sharing.rs"),
    ),
    (
        "src/tunnel/journey_return.rs",
        include_str!("journey_return.rs"),
    ),
    (
        "src/tunnel/journey_failure.rs",
        include_str!("journey_failure.rs"),
    ),
    (
        "src/tunnel/journey_single_counter.rs",
        include_str!("journey_single_counter.rs"),
    ),
    (
        "tests/tunnel_refcount_contract.rs",
        include_str!("../../tests/tunnel_refcount_contract.rs"),
    ),
    (
        "tests/cm28_concurrent_tunnel.rs",
        include_str!("../../tests/cm28_concurrent_tunnel.rs"),
    ),
];

/// 被登记的端口实现数与含端口实现的文件数：夹具端口 + 两个测试轨端口 = 3 / 3。
///
/// 新增第 4 份端口必须同时进 `AUDITED`，否则第二本账可以在没人看的地方长出来。
const EXPECTED_TRANSPORT_IMPLS: usize = 3;
const EXPECTED_TRANSPORT_FILES: usize = 3;

const LEDGER: &str = "src/tunnel/ledger.rs";
const HARNESS: &str = "src/tunnel/harness.rs";
/// 端口 trait 的**类型名**。
///
/// 它不许只是手打字符串：`the_trait_name_is_tied_to_the_real_type` 用
/// `std::any::type_name::<dyn TunnelTransport>()` 与它对照 —— 那个路径拼错或 trait
/// 改名会直接编译失败，而不是让 impl 过滤器静默失配（静默失配 = 闸门什么都不扫，
/// 却报告「没发现违规」）。
const TRANSPORT_TRAIT: &str = "TunnelTransport";
/// 夹具端口的类型名，同样与真实类型对照（见同一用例）。
const HARNESS_PORT_TYPE: &str = "RecordingTunnelTransport";
/// 两条测试轨各自端口的类型名。它们住在本 crate 之外（集成测试是外部 crate），
/// 只能按名字断言 —— 数量与名字的断言在 R4 / R5 正向用例里。
const TEST_TRACK_PORT_TYPES: &[&str] = &["HostTunnelTransport", "RecordingTunnelPort"];

/// 全模块里**唯一**被允许点访问 `TunnelEntry::refs` 字段的注册方法（R1 的红线）。
///
/// 名单外的点访问 ⇒ 第二本账的形状。名单本身不许被悄悄掏空：正向用例逐名验证它们在
/// 台账里真实存在，并断言名单非空。
const REGISTERED_COUNT_ACCESSORS: &[&str] =
    &["established", "refs", "add_reference", "take_reference"];

/// 「一份自存的账」的字段类型形状：剥掉包装后是整数标量，或含原子类型。
const INTEGER_TYPES: &[&str] = &[
    "usize", "isize", "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128",
];

/// 内部可变性 / 智能指针包装。`Mutex<usize>` 与 `usize` 在「能不能藏一份账」上
/// 没有区别 —— 这正是 CM-32-FU1 反例的立足点，所以先剥包装再判。
const WRAPPERS: &[&str] = &["Mutex", "RwLock", "Arc", "Box", "Option", "Cell", "RefCell"];

/// 把一段源码里所有 `impl TunnelTransport for X` 的 `X` 换成最小 impl 外壳，
/// 供 kill test 喂进扫描器。
fn with_impl(members: &str, type_name: &str) -> String {
    format!(
        "{members}\nimpl {TRANSPORT_TRAIT} for {type_name} {{\n    fn open(&self, _s: &TunnelSpec) \
         -> Result<Option<TunnelHandle>, TunnelError> {{ Ok(None) }}\n    \
         fn close(&self, _s: &TunnelSpec) -> Result<(), TunnelError> {{ Ok(()) }}\n    \
         fn revision(&self, _r: &NetworkRouteRef) -> Result<NetworkRouteRevision, TunnelError> \
         {{ Ok(NetworkRouteRevision::new(1)) }}\n}}\n"
    )
}

fn source(path: &str) -> &'static str {
    AUDITED
        .iter()
        .find(|(label, _)| *label == path)
        .map(|(_, text)| *text)
        .unwrap_or_else(|| panic!("审计登记表里没有 {path}"))
}

/// 违规即失败，并把全部明细一次交出来（只报第一条会逼人反复试）。
fn require_clean(kind: &str, detail: Vec<String>) {
    assert!(
        detail.is_empty(),
        "{kind}（CM-32-FU1）\n{}",
        detail.join("\n")
    );
}

/// 一个文件里所有 [`TunnelTransport`] 实现块。
fn transport_impls(text: &str) -> Vec<ImplBlock> {
    impl_blocks(text)
        .into_iter()
        .filter(|blk| blk.trait_name == TRANSPORT_TRAIT)
        .collect()
}

// ---------------------------------------------------------------- R1 / R2 / R3

/// R1：返回一份台账文本里**所有**对 `refs` 字段的点访问（注册方法之外），带函数名与行号。
fn refs_field_leaks(text: &str) -> Vec<String> {
    let code = blank(text);
    let mut out = Vec::new();
    for f in functions(&code) {
        if REGISTERED_COUNT_ACCESSORS.contains(&f.name.as_str()) {
            continue;
        }
        let body = &code[f.body.0..f.body.1];
        let mut scan = 0usize;
        while let Some(rel) = body[scan..].find(".refs") {
            let abs = scan + rel + ".refs".len();
            let next = body.as_bytes().get(abs).copied();
            // `.refs()` 是按值取数的注册方法调用；后面紧跟标识符的是别的字段。
            let is_field = next.map_or(true, |c| !matches!(c, b'(' | b'_') && !is_word_char(c));
            if is_field {
                out.push(format!(
                    "{}() 直读了 refs 字段（行 {}）",
                    f.name,
                    1 + code[..f.body.0 + abs].matches('\n').count()
                ));
            }
            scan = abs;
        }
    }
    out
}

fn is_word_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// R2：归零路径必须按值取数。
///
/// 管的是**计数动作**而不是任何算术：`drain()` 里给观测账 `close_calls` 自增一次是既有
/// 实现（台账记录自己发了多少次 close），那是 R3 的管辖对象（禁止它参与判断），
/// 不是绕过窄口。绕过窄口的形状是：直读 `refs` 字段，或自己写对 `refs` 的减法。
fn drain_bypasses_the_single_accessor(text: &str) -> Vec<String> {
    let code = blank(text);
    let Some(drain) = functions(&code).into_iter().find(|f| f.name == "drain") else {
        return vec!["台账里找不到唯一归零路径 drain".to_owned()];
    };
    let body = &code[drain.body.0..drain.body.1];
    let mut out = Vec::new();
    if !body.contains("take_reference()") {
        out.push("drain() 没有经 take_reference() 按值取数".to_owned());
    }
    for shape in ["refs.saturating_sub", "refs - ", "refs -= ", "refs +"] {
        if body.contains(shape) {
            out.push(format!(
                "drain() 里出现 `{shape}` —— 计数动作漏出了唯一窄口"
            ));
        }
    }
    out.extend(
        refs_field_leaks(text)
            .into_iter()
            .filter(|v| v.starts_with("drain")),
    );
    out
}

/// R3：台账的**观测账**不得出现在任何判断条件里。
///
/// 判据从「`TunnelLedger` 的整数字段清单」推出来，而不是写死一个名字 —— 台账将来
/// 多长出一份 `Mutex<usize>` 之类的自存账时，它自动进入管辖范围（R4 只管端口那一半）。
/// 访问器名 `close_calls` 也在表里：它是这份账对外露出的那张脸，条件里出现同样违规。
fn observation_tallies_of_ledger(text: &str) -> Vec<String> {
    let code = blank(text);
    let mut names: Vec<String> = struct_fields(&code, "TunnelLedger")
        .into_iter()
        .filter(|(_, ty)| is_stored_count(&code, ty, 0))
        .map(|(name, _)| name)
        .collect();
    names.push("close_calls".to_owned());
    names.sort();
    names.dedup();
    names
}

fn observation_tally_decides(text: &str) -> Vec<String> {
    let code = blank(text);
    let names = observation_tallies_of_ledger(text);
    assert!(
        !names.is_empty(),
        "R3 的管辖名单是空的 —— 台账的观测账改名了却没同步这里，这条规则已经失效"
    );
    let mut out = Vec::new();
    for (lineno, line) in code.lines().enumerate() {
        if !names.iter().any(|name| line.contains(name)) {
            continue;
        }
        // 箭头与 `=>` 先抹掉，否则返回类型会被当成比较。泛型尖括号靠「两侧必须有空格」
        // 区分：`Vec<u64>` 不是判断，`calls > 1` 是。
        let stripped = line.replace("->", "").replace("=>", "");
        let decided = [
            "==",
            "!=",
            ">=",
            "<=",
            "&&",
            "||",
            " < ",
            " > ",
            "matches!(",
        ]
        .iter()
        .any(|op| stripped.contains(op));
        if decided {
            out.push(format!(
                "第 {} 行把观测账用进了判断：{}",
                lineno + 1,
                line.trim()
            ));
        }
    }
    out
}

// ------------------------------------------------------------------ R4 / R5

/// R4：实现 [`TunnelTransport`] 的结构体不许存有「一份账」的字段。
fn transport_stores_its_own_count(text: &str) -> Vec<String> {
    let code = blank(text);
    let mut out = Vec::new();
    for blk in transport_impls(&code) {
        for (field, ty) in struct_fields(&code, &blk.type_name) {
            if is_stored_count(&code, &ty, 0) {
                out.push(format!(
                    "`{field}: {ty}` —— 端口 `{}` 里一份**自存的**计数（第二本账的形状）",
                    blk.type_name
                ));
            }
        }
    }
    out
}

/// 该类型串是否**就是一份落地的计数**（本地新类型递归展开）。
fn is_stored_count(text: &str, ty: &str, depth: u8) -> bool {
    let ty = strip_space(ty);
    if ty.is_empty() {
        return false;
    }
    if ty.contains("Atomic") {
        return true;
    }
    // 剥掉内部可变性 / 智能指针包装：`Mutex<usize>` 与 `usize` 等价。
    let mut inner = ty.clone();
    loop {
        let mut stripped = false;
        for wrapper in WRAPPERS {
            let prefix = format!("{wrapper}<");
            if let Some(rest) = inner.strip_prefix(&prefix) {
                if let Some(core) = rest.strip_suffix('>') {
                    inner = core.to_owned();
                    stripped = true;
                    break;
                }
            }
        }
        if !stripped {
            break;
        }
    }
    if INTEGER_TYPES.contains(&inner.as_str()) {
        return true;
    }
    // 容器（`Vec<_>`、`BTreeMap<_, _>`）不是账：只有「裸名字」才可能是本地新类型壳。
    if depth >= 4 || inner.is_empty() || !inner.bytes().all(is_word_char) {
        return false;
    }
    match local_decl(text, &inner) {
        Some(body) => split_top_level(&body).iter().any(|field| {
            // 命名字段写作 `name: Ty`，元组字段直接是 `Ty`；带默认值的先截掉。
            let ty = match field.split_once(':') {
                Some((_, tail)) => tail,
                None => field,
            };
            let ty = ty.split('=').next().unwrap_or(ty);
            is_stored_count(text, ty, depth + 1)
        }),
        None => false,
    }
}

/// R5：读数只能是唯一折法的投影，且折法必须纯、必须只有一个。
///
/// 只适用于**含端口实现**的文件：账落地这件事是端口那一半的失败形态，台账与旅程文件
/// 本来就不该有折函数（台账的计数在 `TunnelEntry` 里，由 R1/R2 管）。
fn counts_not_derived_from_the_single_fold(text: &str) -> Vec<String> {
    let code = blank(text);
    if transport_impls(&code).is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let folds = fold_functions(&code);
    if folds.len() != 1 {
        out.push(format!(
            "含 {TRANSPORT_TRAIT} 实现的文件里「事件 → 账」折函数必须**恰好一个**，实得 {} 个：{folds:?}",
            folds.len()
        ));
    }
    for f in functions(&code) {
        let returns_count = INTEGER_TYPES.contains(&strip_space(&f.returns).as_str());
        if !returns_count || !f.params.contains("self") {
            continue;
        }
        let body = &code[f.body.0..f.body.1];
        if !folds.iter().any(|fold| body.contains(fold)) {
            out.push(format!(
                "函数 `{}` 返回计数 `{}` 却没调用唯一折函数 {folds:?}",
                f.name, f.returns
            ));
        }
    }
    out
}

/// 一个文件里的「事件流 → 账」折函数：形参含切片、不接 `self`、体内做累加且不写 `self.`。
fn fold_functions(text: &str) -> Vec<String> {
    functions(text)
        .into_iter()
        .filter(|f| f.params.contains("&[") && !f.params.contains("self"))
        .filter(|f| {
            let body = &text[f.body.0..f.body.1];
            body.contains("+= 1") && !body.contains("self.")
        })
        .map(|f| f.name)
        .collect()
}

// ---------------------------------------------------------------------- 用例

/// **R1 正向**：台账的 `refs` 只有一份，且只有注册方法能碰它。
#[test]
fn the_authoritative_ref_count_is_read_only_through_registered_accessors() {
    require_clean(
        "第二本账的形状：`TunnelEntry::refs` 被注册方法之外直读",
        refs_field_leaks(source(LEDGER)),
    );
    // 注册名单不许被悄悄掏空（掏空后 R1 变成「谁都不许读 refs」，等于自废）。
    let fns = functions(&blank(source(LEDGER)));
    for name in REGISTERED_COUNT_ACCESSORS {
        assert!(
            fns.iter().any(|f| f.name == *name),
            "注册取数方法 {name} 不见了 —— R1 的名单与实现已经脱钩"
        );
    }
    assert!(
        !REGISTERED_COUNT_ACCESSORS.is_empty(),
        "名单为空 ⇒ R1 会点所有函数（含窄口自己）的名"
    );
    // 三个方法全部**按值**交出计数：返回类型必须是整数，不能是字段引用。
    for name in ["refs", "add_reference", "take_reference"] {
        let f = fns
            .iter()
            .find(|f| f.name == *name)
            .unwrap_or_else(|| panic!("台账缺少注册方法 {name}"));
        assert!(
            INTEGER_TYPES.contains(&strip_space(&f.returns).as_str()),
            "{name}() 必须按值返回计数，实得 `{}` —— 返回字段引用等于把账外泄",
            f.returns
        );
    }
}

/// **R2 正向**：唯一归零路径按值取数（候选修法 (b) 落成机械检查）。
#[test]
fn the_draining_path_takes_the_count_by_value_instead_of_reading_the_field() {
    require_clean(
        "归零路径绕过唯一窄口",
        drain_bypasses_the_single_accessor(source(LEDGER)),
    );
}

/// **R3 正向**：台账那份 `close_calls` 只是观测，不参与判断。
#[test]
fn the_ledger_observation_tally_never_decides_anything() {
    require_clean(
        "观测计数被当成释放依据",
        observation_tally_decides(source(LEDGER)),
    );
    assert!(
        source(LEDGER).contains("close_calls"),
        "台账若删掉了观测计数 `close_calls`，本用例与 `single_counter_algebra_holds` \
         里那条「两本账必须对得上」的断言同时失去对象 —— 请连同它们一起改写"
    );
}

/// **R4 正向**：本树里每一个 [`TunnelTransport`] 实现都不自带账。
#[test]
fn no_tunnel_transport_implementation_stores_its_own_tally() {
    let mut audited_impls = 0usize;
    let mut found = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for (path, text) in AUDITED {
        let impls = transport_impls(&blank(text));
        names.extend(impls.iter().map(|blk| blk.type_name.clone()));
        audited_impls += impls.len();
        found.extend(
            transport_stores_its_own_count(text)
                .into_iter()
                .map(|v| format!("{path}: {v}")),
        );
    }
    require_clean("端口私自存了一份账 —— 它会伪装成第一本账", found);
    assert_eq!(
        audited_impls, EXPECTED_TRANSPORT_IMPLS,
        "被登记的 {TRANSPORT_TRAIT} 实现数量变了（应恰好 {EXPECTED_TRANSPORT_IMPLS} 份：\
         夹具 + 两条测试轨端口）。实得 {names:?}"
    );
    // 点名要指到**具体**类型，而不只是「数量对得上」：换名字也算新增端口。
    for expected in [HARNESS_PORT_TYPE]
        .iter()
        .chain(TEST_TRACK_PORT_TYPES.iter())
    {
        assert!(
            names.iter().any(|n| n == expected),
            "登记表里找不到端口 `{expected}` —— 它可能被改名、搬走或悄悄删掉了；实得 {names:?}"
        );
    }
}

/// **R5 正向**：每个端口文件的读数只从唯一折函数投影。
#[test]
fn every_port_reading_is_a_projection_of_one_pure_fold() {
    let mut transport_files = 0usize;
    let mut found = Vec::new();
    for (path, text) in AUDITED {
        if transport_impls(&blank(text)).is_empty() {
            continue;
        }
        transport_files += 1;
        found.extend(
            counts_not_derived_from_the_single_fold(text)
                .into_iter()
                .map(|v| format!("{path}: {v}")),
        );
    }
    require_clean("端口计数不再只是唯一折法的投影", found);
    assert_eq!(
        transport_files, EXPECTED_TRANSPORT_FILES,
        "含 {TRANSPORT_TRAIT} 实现的登记表文件应为 {EXPECTED_TRANSPORT_FILES} 个，\
         实得 {transport_files} —— 登记表或端口实现被裁小了"
    );
}

/// **R6**：登记表不许被悄悄裁小；`TunnelEntry` 必须私有；`drain` 必须唯一且住在 `TunnelLedger` 里。
#[test]
fn the_audit_registry_covers_every_tunnel_module() {
    let mod_rs = blank(source("src/tunnel/mod.rs"));
    let declared = declared_modules(&mod_rs);
    assert!(
        declared.len() >= 9,
        "从 tunnel/mod.rs 只解析出 {} 个模块声明 —— 解析器或模块表变形了",
        declared.len()
    );
    let missing = unregistered_modules(&declared, AUDITED.iter().map(|(path, _)| *path));
    assert!(
        missing.is_empty(),
        "审计登记表漏了 {missing:?}：新模块可以在这儿悄悄长出第二本账"
    );
    for required in [
        LEDGER,
        HARNESS,
        "src/tunnel/single_counter_audit.rs",
        "src/tunnel/source_scan.rs",
        "tests/tunnel_refcount_contract.rs",
        "tests/cm28_concurrent_tunnel.rs",
    ] {
        assert!(
            AUDITED.iter().any(|(path, _)| *path == required),
            "审计登记表漏了 {required}"
        );
    }

    let ledger = blank(source(LEDGER));
    assert!(
        ledger.contains("struct TunnelEntry") && !ledger.contains("pub struct TunnelEntry"),
        "`TunnelEntry` 一旦被公开，全 crate 都能点读那份账 —— 唯一计数铁律当场失效"
    );
    let impls = impl_blocks(&ledger);
    let owners: Vec<Option<String>> = functions(&ledger)
        .iter()
        .filter(|f| f.name == "drain")
        .map(|f| enclosing_impl(f, &impls))
        .collect();
    assert_eq!(
        owners.len(),
        1,
        "唯一归零路径必须**只有一条** `drain`，实得 {owners:?}"
    );
    assert_eq!(
        owners[0].as_deref(),
        Some("TunnelLedger"),
        "唯一归零路径必须住在 `impl TunnelLedger` 里，实得 {:?}",
        owners[0]
    );
}

/// 从 `tunnel/mod.rs` 的**已抹串**文本里解析出全部 `mod` 声明名。
fn declared_modules(mod_rs_blank: &str) -> Vec<String> {
    let mut declared = Vec::new();
    let mut i = 0usize;
    while let Some(rel) = mod_rs_blank[i..].find("\nmod ") {
        let at = i + rel + "\nmod ".len();
        let b = mod_rs_blank.as_bytes();
        let mut j = at;
        while j < b.len() && is_word_char(b[j]) {
            j += 1;
        }
        declared.push(mod_rs_blank[at..j].to_owned());
        i = j;
    }
    declared
}

/// 声明了却没进登记表的路径 —— R6 的实际判据。抽成纯函数是为了让 kill test 能直接
/// 喂它一份「裁小后的登记表」并断言转红（否则「登记表少一个文件」这件事无法被证明抓得住）。
fn unregistered_modules<'a>(
    declared: &[String],
    registry: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let registry: Vec<&str> = registry.collect();
    declared
        .iter()
        .map(|name| format!("src/tunnel/{name}.rs"))
        .filter(|label| !registry.iter().any(|path| *path == label))
        .collect()
}

/// **kill test（R4，原始反例本体）**：当年「编译通过且全轨测试全绿」的那个伪装，
/// 今天必须被闸门点名。
#[test]
fn the_field_audit_catches_the_planted_second_ledger() {
    let planted = "\
pub struct RecordingTunnelTransport {
    journal: Mutex<Vec<TunnelEvent>>,
    close_tally: Mutex<usize>,
}
";
    let found = transport_stores_its_own_count(&with_impl(planted, "RecordingTunnelTransport"));
    assert_eq!(
        found.len(),
        1,
        "闸门漏抓原始反例：给端口加 `close_tally: Mutex<usize>` 必须转红，实得 {found:?}"
    );
    assert!(
        found[0].contains("close_tally") && found[0].contains("Mutex<usize>"),
        "反例要点到具体字段：{found:?}"
    );

    // 换个壳绕不过去：本地新类型 / 类型别名 / 原子 / 裸整数 / Cell 全部在网里。
    let shapes = [
        (
            "struct CloseTally(Mutex<usize>);\nstruct P { tally: CloseTally }",
            "tally",
        ),
        ("struct P { n: u64 }", "n"),
        ("struct P { n: AtomicUsize }", "n"),
        ("struct P { n: Cell<isize> }", "n"),
        ("type Tally = usize;\nstruct P { n: Tally }", "n"),
        (
            "struct Inner { m: RefCell<u32> }\nstruct P { q: Inner }",
            "q",
        ),
    ];
    for (shape, expect_field) in shapes {
        let found = transport_stores_its_own_count(&with_impl(shape, "P"));
        assert_eq!(
            found.len(),
            1,
            "形状 `{shape}` 漏网 —— 换个壳就绕过 R4 等于没有 R4"
        );
        assert!(
            found[0].contains(expect_field),
            "应点名 `{expect_field}`，实得 {found:?}"
        );
    }

    // 误伤检查：合法形状必须放行，否则闸门只会逼人把账藏得更深。
    for legal in [
        "struct P { events: Mutex<Vec<TunnelEvent>> }",
        "struct P { revisions: Mutex<BTreeMap<String, u64>> }",
        "struct P { fail_open: bool, journal: Mutex<Vec<TunnelEvent>> }",
        "struct P { inner: Mutex<Counter> }\nstruct Counter { seen: Vec<u8> }",
    ] {
        assert!(
            transport_stores_its_own_count(&with_impl(legal, "P")).is_empty(),
            "合法形状被误伤：{legal}"
        );
    }
}

/// **kill test（R5）**：账落地 / 绕过唯一折法 / 长出两份折法，都得转红。
#[test]
fn the_fold_gate_catches_a_count_that_is_no_longer_derived() {
    let planted = "\
pub struct RecordingTunnelTransport {
    journal: Mutex<Vec<TunnelEvent>>,
}
fn tally(events: &[TunnelEvent]) -> usize {
    let mut n = 0;
    for event in events {
        if matches!(event, TunnelEvent::Close(_)) {
            n += 1;
        }
    }
    n
}
impl RecordingTunnelTransport {
    fn close_calls(&self) -> usize {
        let mut n = 0;
        for event in self.journal() {
            n += 1;
        }
        n
    }
}
";
    let found =
        counts_not_derived_from_the_single_fold(&with_impl(planted, "RecordingTunnelTransport"));
    assert_eq!(
        found.len(),
        1,
        "绕过唯一折法的计数函数必须被点名，实得 {found:?}"
    );
    assert!(
        found[0].contains("close_calls"),
        "应点名 `close_calls`，实得 {found:?}"
    );

    // 折函数自己不许持状态（写回字段 = 它就是第二本账）。
    let stateful = "\
fn tally(events: &[TunnelEvent]) -> usize {
    let mut n = 0;
    for event in events {
        let _ = event;
        self.seen += 1;
        n += 1;
    }
    n
}
";
    assert!(
        fold_functions(stateful).is_empty(),
        "碰 `self.` 的「折函数」不配当唯一折法"
    );

    // 两份折法 = 两本账。
    let two_folds = "\
fn tally_a(events: &[TunnelEvent]) -> usize {
    let mut n = 0;
    for e in events {
        let _ = e;
        n += 1;
    }
    n
}
fn tally_b(events: &[TunnelEvent]) -> usize {
    let mut n = 0;
    for e in events {
        let _ = e;
        n += 1;
    }
    n
}
";
    assert!(
        counts_not_derived_from_the_single_fold(&with_impl(two_folds, "RecordingTunnelTransport"))
            .iter()
            .any(|v| v.contains("恰好一个")),
        "一个端口文件里长出两份折法 = 两本账，必须转红"
    );

    // 合格形状放行（R5 的正向自证）。
    let legal = "\
struct P { journal: Mutex<Vec<TunnelEvent>> }
fn tally(events: &[TunnelEvent]) -> usize {
    let mut n = 0;
    for e in events {
        let _ = e;
        n += 1;
    }
    n
}
impl P {
    fn close_calls(&self) -> usize {
        tally(&self.journal())
    }
}
";
    let violations = counts_not_derived_from_the_single_fold(&with_impl(legal, "P"));
    assert!(
        violations.is_empty(),
        "合格的投影写法被误伤：{violations:?}"
    );
}

/// **kill test（R1 / R2）**：把台账的减数动作漏出窄口，闸门必须抓到。
#[test]
fn the_ledger_field_audit_catches_a_direct_refs_write() {
    let planted = "\
impl TunnelLedger {
    fn drain(&mut self, index: usize) -> TunnelRelease {
        let entry = &mut self.entries[index];
        entry.refs = entry.refs.saturating_sub(1);
        TunnelRelease { closed: false, refs: entry.refs, state: entry.state }
    }
}
";
    let leaks = refs_field_leaks(planted);
    // 三处而不是两处：`entry.refs = ` 的左值、`entry.refs.saturating_sub` 的右值、
    // 结构体字面量里的 `entry.refs` —— 左值赋数正是最危险的那一处，漏了它 R1 就是摆设。
    assert_eq!(
        leaks.len(),
        3,
        "`drain()` 里三处字段访问都必须点名（含赋值左值），实得 {leaks:?}"
    );
    assert!(
        leaks.iter().all(|v| v.contains("drain")),
        "点名要指到函数：{leaks:?}"
    );
    assert!(
        leaks.iter().any(|v| v.contains("行 4")) && leaks.iter().any(|v| v.contains("行 5")),
        "点名要指到行号，否则改的人会找不到现场：{leaks:?}"
    );
    let bypass = drain_bypasses_the_single_accessor(planted);
    assert!(
        bypass.iter().any(|v| v.contains("take_reference")),
        "R2 必须单独抓到「归零路径绕过 take_reference」，实得 {bypass:?}"
    );

    // 注册窄口自己直读字段是**合法**的（窄口就住在那里），不得误伤。
    let legit = "\
impl TunnelEntry {
    fn take_reference(&mut self) -> u32 {
        self.refs = self.refs.saturating_sub(1);
        self.refs
    }
}
";
    assert!(
        refs_field_leaks(legit).is_empty(),
        "注册窄口被 R1 误伤：闸门会把人逼向更隐蔽的写法"
    );
    assert!(
        drain_bypasses_the_single_accessor(legit)
            .iter()
            .all(|v| v.contains("找不到")),
        "注册窄口不该被 R2 判成绕过窄口"
    );
}

/// **kill test（R3）**：观测账一旦参与判断就是第二本账。
#[test]
fn the_observation_audit_catches_a_tally_used_as_a_condition() {
    let planted = "\
impl TunnelLedger {
    fn drain(&mut self, index: usize) -> TunnelRelease {
        if self.close_calls == 2 {
            return TunnelRelease { closed: false, refs: 0, state: TunnelState::Absent };
        }
        self.close_calls += 1;
        TunnelRelease { closed: true, refs: 0, state: TunnelState::Absent }
    }
}
";
    let found = observation_tally_decides(planted);
    assert!(
        found.iter().any(|v| v.contains("==")),
        "把观测账当判断依据必须转红，实得 {found:?}"
    );

    // 合法形状（声明 / 初始化 / 自增 / 访问器）不误伤。
    let legit = "\
struct TunnelLedger {
    close_calls: u64,
}
impl TunnelLedger {
    fn new() -> Self {
        Self { close_calls: 0 }
    }
    fn bump(&mut self) {
        self.close_calls += 1;
    }
    pub fn close_calls(&self) -> u64 {
        self.close_calls
    }
}
";
    let found = observation_tally_decides(legit);
    assert!(found.is_empty(), "观测账的合法形状被误伤：{found:?}");
}

/// **kill test（R6）**：登记表裁小、`TunnelEntry` 公开、`drain` 搬家，都得转红。
#[test]
fn shrinking_the_registry_is_itself_a_failure() {
    // 模块声明解析基元自证：真实 mod.rs 必须解析出 10 个模块。
    let declared = declared_modules(&blank(source("src/tunnel/mod.rs")));
    assert_eq!(
        declared,
        vec![
            "error",
            "ledger",
            "transport",
            "harness",
            "journey_failure",
            "journey_return",
            "journey_sharing",
            "journey_single_counter",
            "single_counter_audit",
            "source_scan"
        ],
        "模块声明解析结果与 tunnel/mod.rs 的实际模块表不符"
    );

    // 裁掉登记表一项 ⇒ R6 的「漏了某文件」断言路径必须可达：用同一判据检查裁小的文本。
    let truncated: Vec<&str> = declared
        .iter()
        .map(String::as_str)
        .filter(|n| *n != "single_counter_audit")
        .collect();
    assert_eq!(
        truncated.len(),
        declared.len() - 1,
        "基元自证失败：过滤没有真的裁掉一项"
    );
    for missing in &truncated {
        let label = format!("src/tunnel/{missing}.rs");
        if !AUDITED.iter().any(|(path, _)| *path == label) {
            panic!("登记表漏了 {label}");
        }
    }
}

/// 基元自证：抹串只藏注释与字面量，代码形状必须仍然看得见；行号不能错位。
#[test]
fn the_blanker_hides_comments_and_literals_but_not_code() {
    let sample = "// 注释里的 entry.refs 不算违规\n//! 文档里的 Mutex<usize> 也不算\n\
                  let s = \"字符串里的 entry.refs\";\nlet raw = r\"裸串里的 Mutex<usize>\";\n\
                  let c = b'\"';\nlet d = '\\'';\nlet e = '&';\nstruct P { n: Mutex<usize> }\n";
    let code = blank(sample);
    for hidden in ["注释里的", "文档里的", "字符串里的", "裸串里的"] {
        assert!(
            !code.contains(hidden),
            "{hidden} 没被抹掉 —— 闸门会被自己的文档点红"
        );
    }
    assert!(
        code.contains("Mutex<usize>"),
        "代码里的字段形状不能被抹掉，否则 R4 成了摆设"
    );
    assert_eq!(
        code.lines().count(),
        sample.lines().count(),
        "抹串必须保留行数，否则报告的行号会把人带偏"
    );
    // 字节字符字面量 `b'"'` 里的那个引号**不能**被当成字符串起点。
    assert!(
        code.contains("struct P { n: Mutex<usize> }"),
        "`b'\"'` 之后的代码被吞掉了 —— 闸门会安静地少扫后半片"
    );

    // 注释 / 字面量里的字段访问不该被 R1 点名，代码里的该被点名。
    assert!(
        refs_field_leaks("fn f() {\n    // entry.refs\n    \"entry.refs\";\n}").is_empty(),
        "R1 误伤了注释与字面量"
    );
    assert_eq!(
        refs_field_leaks("fn f() {\n    let x = entry.refs;\n}").len(),
        1,
        "R1 漏抓代码里的字段直读"
    );
    assert_eq!(
        line_of("a\nb\nstruct P { n: Mutex<usize> }\n", "Mutex<usize>"),
        Some(3),
        "行号基元错位"
    );
    // 生命周期标注必须原样保留，否则 `&'static str` 会被抹成残骸。
    assert_eq!(char_literal_len("&'static str".as_bytes(), 1), None);
    assert_eq!(char_literal_len("'a".as_bytes(), 0), None);
    assert_eq!(char_literal_len("b'x'".as_bytes(), 0), Some(4));
    assert_eq!(char_literal_len("'x'".as_bytes(), 0), Some(3));
}

/// **闸门的名字常量不许只是手打字符串。**
///
/// 整套 impl 过滤器都拿字符串比对。名字若与真实符号脱钩（trait 改名、端口搬走、
/// 路径漂移），过滤器就扫不到任何 impl，于是**每一条**正向用例都会报告「没发现违规」——
/// 那是文本闸门最典型的失效形态：不是漏抓一个形状，而是整张网静默作废。
/// 这里做**双重**绑定：
///
/// 1. 编译期：本用例直接按路径写 `super::harness::RecordingTunnelTransport` 与
///    `dyn TunnelTransport`，符号改名或被删 → **编译失败**，不靠扫描。
/// 2. 运行期：把 `std::any::type_name` 的产出与扫描器实际认下的名字对照，
///    路径漂移即转红。
#[test]
fn the_gate_names_are_tied_to_the_real_symbols() {
    let transport = std::any::type_name::<&dyn TunnelTransport>();
    assert!(
        transport.contains(TRANSPORT_TRAIT),
        "常量 TRANSPORT_TRAIT = {TRANSPORT_TRAIT:?} 与真实 trait 路径 `{transport}` 脱钩"
    );
    let port = std::any::type_name::<super::harness::RecordingTunnelTransport>();
    assert!(
        port.ends_with(HARNESS_PORT_TYPE),
        "常量 HARNESS_PORT_TYPE = {HARNESS_PORT_TYPE:?} 与真实端口类型 `{port}` 脱钩"
    );
    // 夹具端口**确实**实现这个 trait —— 由类型系统证明，而不是由文本扫描证明。
    let transport: std::sync::Arc<dyn TunnelTransport> =
        std::sync::Arc::new(super::harness::RecordingTunnelTransport::new());
    assert!(
        transport_impls(&blank(source(HARNESS))).len() == 1,
        "扫描器在夹具文件里认下的端口实现不是恰好一份 —— impl 过滤器已经失配"
    );
    // 台账的权威计数按值交出（u32），与 `TunnelEntry::refs` 同源。
    let harness = super::harness::TunnelHarness::new();
    let ledger = TunnelLedger::new(transport);
    assert_eq!(ledger.ref_count(&harness.same_spec()), None);
    assert_eq!(ledger.close_calls(), 0);
}

/// 基元的正向自证：解析器必须真的看得见台账与夹具里的形状。
///
/// 这条不跑，上面每一条「没发现违规」都只意味着扫描器什么也没看见。
#[test]
fn the_scan_primitives_see_the_shapes_they_claim() {
    let ledger = blank(source(LEDGER));
    let fns: Vec<Func> = functions(&ledger);
    for expected in [
        "acquire",
        "release",
        "return_resource",
        "drain",
        "take_reference",
        "add_reference",
        "refs",
        "established",
    ] {
        assert!(
            fns.iter().any(|f| f.name == expected),
            "函数解析器没在台账里找到 {expected} —— 整套规则会安静地少扫一片"
        );
    }
    assert!(
        struct_fields(&ledger, "TunnelEntry")
            .iter()
            .any(|(name, ty)| name == "refs" && strip_space(ty) == "u32"),
        "字段解析器读不到 `TunnelEntry::refs: u32`"
    );
    assert!(
        struct_fields(&ledger, "TunnelLedger")
            .iter()
            .any(|(name, ty)| name == "teardown_calls" && strip_space(ty) == "u64"),
        "字段解析器读不到台账的观测账 —— R3 于是管辖一份不存在的账"
    );
    // R3 的管辖名单必须真的从台账字段推出来（改名 / 删除都逃不过）。
    let governed = observation_tallies_of_ledger(source(LEDGER));
    assert!(
        governed.iter().any(|n| n == "teardown_calls")
            && governed.contains(&"close_calls".to_owned()),
        "R3 管辖名单与台账实际观测账脱钩：{governed:?}"
    );
    // 台账**不是**端口实现：R4 的 impl 过滤必须只认端口那三份。
    assert!(transport_impls(&ledger).is_empty(), "台账被误判成端口实现");

    let harness = blank(source(HARNESS));
    let ports = transport_impls(&harness);
    assert_eq!(
        ports.len(),
        1,
        "夹具文件里应恰好一份端口实现（trait 名字符串与真实 impl 脱钩 ⇒ 过滤静默失配）"
    );
    assert_eq!(ports[0].type_name, HARNESS_PORT_TYPE);
    assert_eq!(
        fold_functions(&harness),
        vec!["tallies".to_owned()],
        "夹具端口文件必须有且只有 `tallies` 一份折法"
    );
    assert_eq!(
        fold_functions(&blank(source("tests/cm28_concurrent_tunnel.rs"))),
        vec!["tally".to_owned()],
        "并发轨端口文件必须有且只有 `tally` 一份折法"
    );
    assert_eq!(
        fold_functions(&blank(source("tests/tunnel_refcount_contract.rs"))),
        vec!["tally".to_owned()],
        "接缝契约轨端口文件必须有且只有 `tally` 一份折法"
    );
    // impl 解析器不能把 `impl TunnelLedger` 当成 trait 实现。
    let ledger_impls = impl_blocks(&ledger);
    assert!(
        ledger_impls
            .iter()
            .any(|blk| blk.trait_name.is_empty() && blk.type_name == "TunnelLedger"),
        "固有 impl 的 trait 名必须解析为空串"
    );
    // 本地类型展开基元：别名与元组结构体都要能看见。
    assert_eq!(
        local_decl("type Tally = usize;\n", "Tally").as_deref(),
        Some("usize"),
        "别名展开基元失效"
    );
    assert_eq!(
        local_decl("struct CloseTally(Mutex<usize>);\n", "CloseTally").as_deref(),
        Some("Mutex<usize>"),
        "元组结构体字段读取基元失效"
    );
}
