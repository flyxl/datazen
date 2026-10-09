//! 唯一计数铁律的机械闸门：全系统**只允许一份**隧道引用计数，这条铁律不再靠人审计。
//!
//! # 这条闸门为什么存在
//!
//! `transport.rs` 模块头登记过一个**已实证**的名洞：给录写端口
//! `RecordingTunnelTransport` 加一个 `close_tally: Mutex<usize>`、并让 `close_calls()`
//! 改读它 —— 当年编译通过、全轨测试全绿。第二本账就此伪装成第一本账，「隧道不多减引用」
//! 与「许可归零」同时失效却无人报错。当年留了两个候选修法未裁决；
//! 这里把**两个都落成代码**，并补上它们各自都挡不住的那一层：
//!
//! * **R1（台账侧字段单点，= 候选 a）** `TunnelEntry::refs` 的点访问只许出现在四个注册
//!   方法（`established` / `refs` / `add_reference` / `take_reference`）里。
//! * **R2（台账侧按值取数，= 候选 b）** 唯一归零路径 `drain()` 必须经
//!   `take_reference()` 取数，不许直读字段、不许自己做对 `refs` 的减法。
//! * **R3（观测账不得参与判断）** 台账那份观测账（字段 `teardown_calls`、对外访问器
//!   `close_calls()`）禁止出现在任何条件 / 比较 / 逻辑表达式里 ——
//!   「观测计数被当成释放依据」是第二本账的另一形态。
//! * **R4（端口不得自存账）/ R5（读数只能是折法的投影）** 端口那一半的两条：R4 禁止
//!   实现 [`TunnelTransport`] 的结构体自带一份账（剥包装后是整数标量、或含 `Atomic*`；
//!   本地新类型递归展开，换个壳绕不过去），R5 要求所有端口读数都只能是唯一**纯**折函数
//!   的投影。**这一半连同它的判据、夹具与自证受单文件规模线约束，住在同目录
//!   [`port_audit`]（`port_audit.rs`）** —— 契约说明与实现同处一份文件，读它不必回来翻这里。
//! * **R6（闸门自己不能被裁小）** `mod.rs` 声明的每个模块与三份带端口的测试替身都必须在
//!   登记表内；`TunnelEntry` 必须保持私有；唯一归零路径必须**只有一条**且住在
//!   [`TunnelLedger`] 的 impl 块里。
//! * **R7（台账的声明里只许有一份计数）** `TunnelEntry` 的整数字段必须**恰好一个**且
//!   名为 `refs`，`TunnelLedger` 的整数字段**至多一个**（那份观测账）。R1 只盯 `refs`
//!   这个名字，因此「留着 `refs`、旁边挂一份逐笔相同的镜像账」能整条绕过 R1 / R2 ——
//!   这条是补那个洞的，判据**与名字无关**。
//! * **R2+（释放结果的取值来源）** `drain()` 交给调用方的那个数只许来自
//!   `take_reference()` 的返回值、注册读方法或字面量 0；改从观测账取即转红。
//!
//! R1–R3 + R7 + R2+ 管台账那一半，R4–R5 管端口那一半，R6 管闸门自己。只有其中任一条
//! 都挡不住伪装：原始反例走端口那半边（R4 / R5 各抓一次），「台账多开一个直读字数的
//! 口子」走 R1 / R2，「换名字挂一份镜像账」只有 R7 看得见，「把观测账读进条件」
//! 只有 R3 / R2+ 看得见，「把闸门裁小」只有 R6 看得见。
//!
//! # 每条规则都自带 kill test
//!
//! 只做模式匹配而不证明「它抓得到东西」的闸门比没有闸门更危险 —— 它给出虚假保证
//! （同 `tests/cm28_concurrent_tunnel.rs` 里那条负控的历史）。故每条规则在子模块
//! [`kill_tests`] 里各有一条 `…_catch_a_planted_…` 用例，把**原始反例本身**与
//! **同类伪装**当字符串喂给同一套扫描器并断言转红；同时断言合法形状不被误伤
//! （误伤会把人逼向更隐蔽的写法）。`the_scan_primitives_see_the_shapes_they_claim`
//! 再断言解析器真的看得见台账与夹具里的形状 —— 否则「没发现违规」只意味着
//! 扫描器什么也没看见。
//!
//! # 为什么住在 `src/tunnel/` 而不是顶层 `tests/`
//!
//! 本模块是 `#[cfg(test)] mod single_counter_audit`，因此它随 `--lib` 跑。
//! `mod.rs` 的登记项 F 记着：CI 只跑 `cargo test --lib`，本 crate 顶层那些 `tests/*.rs`
//! 集成二进制**目前不进 CI**。闸门若放在那边，它就只是一份「本地才生效的承诺」，
//! 与它要取代的「审计承诺」同样脆弱。放在 lib 作用域里，伪装每次构建都会被杀 ——
//! 这也意味着闸门能杀掉**写在集成测试里**的伪装（`include_str!` 把那两个文件也扫了），
//! 尽管那些文件本身不进 CI。
//!
//! # 这条保证的边界（不留模糊措辞）
//!
//! 它是**测试期的结构闸门**，不是类型系统保证：挡得住自然写出来的第二本账
//! （字段直读、自存计数、绕过折法、换名字挂镜像账、观测账参与判断，含 `Mutex<usize>`
//! 与本地新类型壳），挡不住蓄意对抗（`unsafe` 指针转义、跨 crate 静态、
//! `proc-macro` 生成）。`&self` + 内部可变性在类型层面本来就装得下任意计数，
//! 这一点任何扫描都消不掉。
//!
//! **闸门自己也在登记表里**（`mod.rs` / `kill_tests.rs` / `gate_self_audit.rs` /
//! `port_audit.rs` / `source_scan.rs` 五份）：
//! 闸门不看自己，「悄悄少扫一片」就没有反制手段
//! （同 `tests/cm70_panic_redaction_guard.rs` 的自我登记先例）。
//! 另有两份子模块按职责拆开 —— `gate_self_audit.rs` 是「R6 查闸门自己」，
//! `port_audit.rs` 是「R4 / R5 查端口那一半」：都同单文件规模线，
//! 拆分是为了让每个文件各自好读，不是为了让哪条规则少做 —— 拆出来的那份照样进
//! 登记表（少登记就会被 `the_audit_registry_covers_every_tunnel_module` 点红）。

mod gate_self_audit;
mod kill_tests;
mod port_audit;

use super::source_scan::{
    blank, char_literal_len, enclosing_impl, functions, impl_blocks, line_of, local_decl,
    split_top_level, strip_space, struct_fields, Func, ImplBlock,
};
use super::{TunnelLedger, TunnelTransport};
// 端口那一半（`port_audit.rs`）与本文件的规则互为证词：R7 复用它的字段形状判据，
// 而 `kill_tests` / `gate_self_audit` 要在真实夹具上断言「恰好一份折法」，
// 所以这几个名字必须留在本文件的命名空间里 —— 判据搬家，证词不许跟着搬家。
use port_audit::{
    counts_not_derived_from_the_single_fold, fold_functions, is_stored_count,
    transport_stores_its_own_count,
};

/// 被审计的源码：(给人看的路径, 编译期嵌进来的文本)。
///
/// 少一个文件就得改这张表，而 `the_audit_registry_covers_every_tunnel_module` 会先把
/// 「`tunnel/mod.rs` 新增了模块却没登记」这种裁小行为点红。
const AUDITED: &[(&str, &str)] = &[
    ("src/tunnel/mod.rs", include_str!("../mod.rs")),
    ("src/tunnel/ledger.rs", include_str!("../ledger.rs")),
    ("src/tunnel/transport.rs", include_str!("../transport.rs")),
    ("src/tunnel/error.rs", include_str!("../error.rs")),
    ("src/tunnel/harness.rs", include_str!("../harness.rs")),
    (
        "src/tunnel/source_scan.rs",
        include_str!("../source_scan.rs"),
    ),
    (
        "src/tunnel/single_counter_audit/mod.rs",
        include_str!("mod.rs"),
    ),
    (
        "src/tunnel/single_counter_audit/kill_tests.rs",
        include_str!("kill_tests.rs"),
    ),
    (
        "src/tunnel/single_counter_audit/port_audit.rs",
        include_str!("port_audit.rs"),
    ),
    (
        "src/tunnel/single_counter_audit/gate_self_audit.rs",
        include_str!("gate_self_audit.rs"),
    ),
    (
        "src/tunnel/journey_sharing.rs",
        include_str!("../journey_sharing.rs"),
    ),
    (
        "src/tunnel/journey_return.rs",
        include_str!("../journey_return.rs"),
    ),
    (
        "src/tunnel/journey_failure.rs",
        include_str!("../journey_failure.rs"),
    ),
    (
        "src/tunnel/journey_single_counter.rs",
        include_str!("../journey_single_counter.rs"),
    ),
    (
        "tests/tunnel_refcount_contract.rs",
        include_str!("../../../tests/tunnel_refcount_contract.rs"),
    ),
    (
        "tests/cm28_concurrent_tunnel.rs",
        include_str!("../../../tests/cm28_concurrent_tunnel.rs"),
    ),
    (
        "tests/tunnel_arch_support/mod.rs",
        include_str!("../../../tests/tunnel_arch_support/mod.rs"),
    ),
];

/// 被登记的端口实现数与含端口实现的文件数：夹具端口 + 三个测试轨端口 = 4 / 4。
///
/// 三个测试轨端口 = `tunnel_refcount_contract.rs` 的 `HostTunnelTransport`、
/// `cm28_concurrent_tunnel.rs` 的 `RecordingTunnelPort`、架构轨共用替身
/// `tunnel_arch_support/mod.rs` 的 `RecordingTunnelPort`（后两个同名、不同文件，
/// 所以「实现数」与「文件数」才都从 3 变成 4 —— 只看名字会把它们当成一份）。
///
/// 新增第 5 份端口必须同时进 `AUDITED`，否则第二本账可以在没人看的地方长出来。
const EXPECTED_TRANSPORT_IMPLS: usize = 4;
const EXPECTED_TRANSPORT_FILES: usize = 4;

const LEDGER: &str = "src/tunnel/ledger.rs";
const HARNESS: &str = "src/tunnel/harness.rs";
/// 台账那份**观测**账的字段名与对外访问器名（R3 的已知点名集合）。
///
/// 两者**刻意不同名**：字段是 `teardown_calls`，访问器是 `close_calls()`。同名会让
/// 按行扫描分不清「声明字段」与「把它读进判断条件」，一个合法访问器就会被误判成违规。
const LEDGER_TALLY_FIELD: &str = "teardown_calls";
const LEDGER_TALLY_ACCESSOR: &str = "close_calls";
/// 闸门本体（目录模块）在登记表里的标签。
const GATE: &str = "src/tunnel/single_counter_audit/mod.rs";
/// 端口 trait 的**类型名**。
///
/// 它不许只是手打字符串：`the_gate_names_are_tied_to_the_real_symbols` 同时用
/// `std::any::type_name` 对照并把夹具端口真的装进 `Arc<dyn TunnelTransport>` ——
/// 名字与真实符号脱钩会**编译失败**或转红，而不是让 impl 过滤器静默失配
/// （静默失配 = 闸门什么都不扫，却报告「没发现违规」）。
const TRANSPORT_TRAIT: &str = "TunnelTransport";
/// 夹具端口的类型名，同样与真实类型对照（见同一用例）。
const HARNESS_PORT_TYPE: &str = "RecordingTunnelTransport";
/// 两条测试轨各自端口的类型名。它们住在本 crate 之外（集成测试是外部 crate），
/// 只能按名字断言 —— 数量与名字的断言在 [`port_audit`] 的 R4 / R5 正向用例里。
const TEST_TRACK_PORT_TYPES: &[&str] = &["HostTunnelTransport", "RecordingTunnelPort"];

/// 全模块里**唯一**被允许点访问 `TunnelEntry::refs` 字段的注册方法（R1 的红线）。
///
/// 名单外的点访问 ⇒ 第二本账的形状。名单本身不许被悄悄掏空——这一条**由 R1 自己
/// 兜住**，不另设断言：名单一旦为空，窄口自己（`refs` / `add_reference` /
/// `take_reference`）就落进「名单外」，`require_clean` 在正向用例开头就红，
/// 且报的是这三条函数名，比一句「名单为空」有用得多。
const REGISTERED_COUNT_ACCESSORS: &[&str] =
    &["established", "refs", "add_reference", "take_reference"];

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
    assert!(detail.is_empty(), "{kind}\n{}", detail.join("\n"));
}

/// 一个文件里所有 [`TunnelTransport`] 实现块。
fn transport_impls(text: &str) -> Vec<ImplBlock> {
    impl_blocks(text)
        .into_iter()
        .filter(|blk| blk.trait_name == TRANSPORT_TRAIT)
        .collect()
}

// -------------------------------------------------------------------- R7 / R2+

/// **R7**：台账的**声明**里只许有一份引用计数。
///
/// R1 只盯 `refs` 这个名字，于是「`refs` 老老实实留着、旁边再挂一个 `shadow_refs: u32`
/// 并让注册窄口同步维护它」可以整条绕过 R1 / R2 —— 那本镜像账与真账逐笔相同，
/// 任何**值**断言都看不出差别（实测：变异后 `cargo test --lib -- tunnel` 全绿，
/// 见那支漏洞探针）。所以这条从**声明**层面钉死，与名字无关：
///
/// * `TunnelEntry` 里「一份落地的计数」形状的字段**必须恰好一个**，且就是 `refs`；
/// * `TunnelLedger` 里这样的字段**至多一个**（那份观测账，另有 R3 禁止它参与判断）。
///
/// 判据复用 R4 的 [`is_stored_count`]，因此 `Mutex<usize>` / `Cell<isize>` /
/// 本地新类型壳（`struct Mirror(Mutex<usize>)`）同样在网里。
fn ledger_declares_a_second_counter(text: &str) -> Vec<String> {
    let code = blank(text);
    let mut out = Vec::new();

    let entry_counts: Vec<String> = struct_fields(&code, "TunnelEntry")
        .into_iter()
        .filter(|(_, ty)| is_stored_count(&code, ty, 0))
        .map(|(name, _)| name)
        .collect();
    if entry_counts != ["refs".to_owned()] {
        out.push(format!(
            "`TunnelEntry` 里「一份落地的计数」形状的字段必须**恰好一个**且名为 `refs`，实得 {entry_counts:?}"
        ));
    }

    let ledger_counts: Vec<String> = struct_fields(&code, "TunnelLedger")
        .into_iter()
        .filter(|(_, ty)| is_stored_count(&code, ty, 0))
        .map(|(name, _)| name)
        .collect();
    if ledger_counts.len() > 1 {
        out.push(format!(
            "`TunnelLedger` 多出自存计数：{ledger_counts:?} —— 只许有一份**观测**账（R3 禁止它参与判断）"
        ));
    }
    out
}

/// **R2 的补充**：`drain()` 交给调用方的那个数，来源只能是窄口返回值 / 注册读方法 / 字面量 0。
///
/// 堵的是「`refs` 字段照旧由窄口管，但**释放结果里的数**改从观测账取」这条路：
/// 那条路上 `refs` 从未被第二本账写坏，R1 / R2 / R7 全都点不到它的名。
fn drain_returns_a_count_from_the_wrong_bank(text: &str) -> Vec<String> {
    let code = blank(text);
    let Some(drain) = functions(&code).into_iter().find(|f| f.name == "drain") else {
        return Vec::new();
    };
    let body = &code[drain.body.0..drain.body.1];
    let mut out = Vec::new();
    // 不要求「顶层」：`TunnelRelease { … refs: X … }` 这个结构体字面量本身就把深度抬到 1，
    // 按顶层筛会把**全部**取值点筛掉，于是这条规则空转（kill test 正是抓在这里）。
    for at in offsets_of(body, "refs:") {
        let tail = body[at..].trim_start();
        let value = tail.split(',').next().unwrap_or(tail).trim();
        let legal = ["remaining", "0", "entry.refs()"];
        if !legal.contains(&value) {
            out.push(format!(
                "drain() 把释放结果的 `refs` 取自 `{value}` —— 只能来自 take_reference() 的返回值、注册读方法或字面量 0"
            ));
        }
    }
    out
}

/// `needle` 在 `hay` 里每次出现之后紧跟的下标（左邻必须是词边界：
/// 否则 `shadow_refs:` 会被当成 `refs:`）。
fn offsets_of(hay: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let b = hay.as_bytes();
    let mut i = 0usize;
    while let Some(rel) = hay[i..].find(needle) {
        let at = i + rel;
        if at == 0 || !is_word_char(b[at - 1]) {
            out.push(at + needle.len());
        }
        i = at + needle.len();
    }
    out
}

/// **R7 正向**：台账的声明里只有一份引用计数，且释放结果里的数来自窄口。
#[test]
fn the_ledger_declares_exactly_one_reference_counter() {
    require_clean(
        "台账的声明里藏了第二份计数（换名字也逃不过）",
        ledger_declares_a_second_counter(source(LEDGER)),
    );
    require_clean(
        "释放决策返回的数取自第二本账",
        drain_returns_a_count_from_the_wrong_bank(source(LEDGER)),
    );
    // 自证：判据在真实台账上**确实看得见** `refs`，不是靠「什么都匹配不到」通过的。
    let entry_fields: Vec<String> = struct_fields(&blank(source(LEDGER)), "TunnelEntry")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(
        entry_fields.contains(&"refs".to_owned()),
        "字段解析器在 TunnelEntry 里只读到 {entry_fields:?} —— R7 就是空转"
    );
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
            let is_field = next.is_none_or(|c| !matches!(c, b'(' | b'_') && !is_word_char(c));
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
/// 另加**已知点名集合**兜底：植入变异常常只给一个 `impl` 块、没有 `struct TunnelLedger`
/// 声明，只靠字段清单会让 R3 对 `if self.teardown_calls == 2` 视而不见（kill test 正是
/// 抓在这里）。已知集合的完整性由 R7 背书：R7 规定台账的整数字段**至多一个**。
/// 访问器名也在集合里：它是这份账对外露出的那张脸，读进条件同样违规。
fn observation_tallies_of_ledger(text: &str) -> Vec<String> {
    let code = blank(text);
    let mut names: Vec<String> = struct_fields(&code, "TunnelLedger")
        .into_iter()
        .filter(|(_, ty)| is_stored_count(&code, ty, 0))
        .map(|(name, _)| name)
        .collect();
    names.push(LEDGER_TALLY_FIELD.to_owned());
    names.push(LEDGER_TALLY_ACCESSOR.to_owned());
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

/// R1 的返回形状判据：返回**引用**才算外泄账，按值（含新值类型）都不算。
///
/// 判据刻意不是「必须是 u32」：把计数包进 `Refs(pub u32)` 才是更安全的写法，
/// 调用方拿不到可变引用。硬编码整数类型会把更安全的写法判成违规，还给出
/// 与事实相反的诊断（实测 `Refs(u32)` 是按值返回，却被报成「返回字段引用等于把账外泄」）。
pub(super) fn leaks_by_return_type(returns: &str) -> bool {
    strip_space(returns).starts_with('&')
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
    // 名单为空不必另设断言，理由见 `REGISTERED_COUNT_ACCESSORS` 的文档。
    //
    // 三个方法全部**按值**交出计数：不许返回引用。
    //
    // 判据是「不是引用类型」，**不是**「必须是 u32」：把计数包进新值类型
    // `Refs(pub u32)` 才是更安全的写法（调用方无法顺手改里面的数），
    // 硬编码整数类型会把更安全的写法判成违规，还给出与事实相反的诊断。
    for name in ["refs", "add_reference", "take_reference"] {
        let f = fns
            .iter()
            .find(|f| f.name == *name)
            .unwrap_or_else(|| panic!("台账缺少注册方法 {name}"));
        assert!(
            !leaks_by_return_type(&f.returns),
            "{name}() 必须按值交出计数，实得 `{}` —— 返回字段引用等于把账外泄",
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
