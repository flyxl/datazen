//! 本模块是 `single_counter_audit` 的**子模块**（`use super::*` 直接看得见父模块的私有
//! 规则函数）—— 拆文件只为守 AGENTS.md 的单文件规模纪律，不改变任何判据。
//!
//! # 这里放的是「闸门的闸门」
//!
//! 只做模式匹配而不证明「它抓得到东西」的闸门比没有闸门更危险：它给出**虚假保证**。
//! 本 crate 里有一条现成的历史（`tests/cm28_concurrent_tunnel.rs` 里那条负控：每轮新建
//! 台账导致 `close_calls()` 恒为 1，把 `+= 1` 改成 `= 1` 全仓 698 条测试仍然全绿）。
//! 所以 R1–R7 每条规则在这里都有一条把**植入变异**当字符串喂给同一套扫描器的用例，
//! 并同时断言**合法形状不被误伤** —— 误伤会把人逼向更隐蔽的写法，比漏抓更难发现。
//!
//! 原始反例（CM-32 repair round 1 实证过「编译通过且全轨测试全绿」的那次改动）由
//! [`the_field_audit_catches_the_planted_second_ledger`] 逐字复现。
//!
//! 变异**文本**写在这里而不是去改真实夹具：真实夹具的源码是扫描对象，往它身上种变异
//! 需要另一棵工作树；作为字符串喂给同一套扫描器，判据与现场完全等价，而且**每次 CI
//! 都会重跑**，不依赖谁记得去做实验。

use super::gate_self_audit::{declared_modules, unregistered_modules};
use super::*;

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
    let found = transport_stores_its_own_count(&with_impl(planted, HARNESS_PORT_TYPE));
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
    let found = counts_not_derived_from_the_single_fold(&with_impl(planted, HARNESS_PORT_TYPE));
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
        counts_not_derived_from_the_single_fold(&with_impl(two_folds, HARNESS_PORT_TYPE))
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
        if self.teardown_calls == 2 {
            return TunnelRelease { closed: false, refs: 0, state: TunnelState::Absent };
        }
        self.teardown_calls += 1;
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
    teardown_calls: u64,
}
impl TunnelLedger {
    fn new() -> Self {
        Self { teardown_calls: 0 }
    }
    fn bump(&mut self) {
        self.teardown_calls += 1;
    }
    pub fn close_calls(&self) -> u64 {
        self.teardown_calls
    }
}
";
    let found = observation_tally_decides(legit);
    assert!(found.is_empty(), "观测账的合法形状被误伤：{found:?}");
}

/// **kill test（R7）**：实测过的漏网形状 —— `refs` 照旧留着，旁边挂一份镜像账。
///
/// 这条钉子是**先发现漏洞再补规则**留下的：R1 / R2 只盯 `refs` 这个名字，于是
/// 「新加一个 `shadow_refs: u32` 并让注册窄口同步维护它」在只有一、二号规则时
/// **编译通过且全轨隧道测试全绿**（镜像账与真账逐笔相同，任何值断言都看不出来）。
/// R7 从**声明**层面钉死，与名字无关。
#[test]
fn the_declaration_audit_catches_a_renamed_mirror_counter() {
    let planted = "\
struct TunnelEntry {
    spec: TunnelSpec,
    state: TunnelState,
    refs: u32,
    shadow_refs: u32,
    dependents: IndexSet<LeaseId>,
}
struct TunnelLedger {
    transport: Arc<dyn TunnelTransport>,
    entries: Vec<TunnelEntry>,
    teardown_calls: u64,
    held: Mutex<usize>,
}
";
    let found = ledger_declares_a_second_counter(planted);
    assert_eq!(
        found.len(),
        2,
        "两条藏账路径都必须点名（entry 里的镜像账、台账里多出来的一份自存账），实得 {found:?}"
    );
    assert!(
        found[0].contains("shadow_refs") && found[1].contains("held"),
        "点名要指到字段：{found:?}"
    );
    assert!(
        ledger_declares_a_second_counter(source(LEDGER)).is_empty(),
        "真实台账被误伤：{:?}",
        ledger_declares_a_second_counter(source(LEDGER))
    );

    // 元组壳同样在网里。
    let boxed = "\
struct Mirror(Mutex<usize>);
struct TunnelEntry {
    refs: u32,
    mirror: Mirror,
}
";
    assert!(
        ledger_declares_a_second_counter(boxed)
            .iter()
            .any(|v| v.contains("mirror")),
        "本地新类型壳的镜像账漏网"
    );
}

/// **kill test（R2 补充）**：把释放结果里的数改取自观测账。
#[test]
fn the_value_source_audit_catches_drain_reading_the_observation_tally() {
    let planted = "\
impl TunnelLedger {
    fn drain(&mut self, index: usize) -> TunnelRelease {
        let remaining = entry.take_reference();
        TunnelRelease { closed: false, refs: self.teardown_calls as u32, state: entry.state }
    }
}
";
    let found = drain_returns_a_count_from_the_wrong_bank(planted);
    assert_eq!(found.len(), 1, "`refs` 取自观测账必须点名，实得 {found:?}");
    assert!(found[0].contains("teardown_calls"), "{found:?}");

    // 合法来源（窄口返回值 / 注册读方法 / 字面量 0）不误伤。
    let legit = "\
impl TunnelLedger {
    fn drain(&mut self, index: usize) -> TunnelRelease {
        if entry.state == TunnelState::Closing {
            return TunnelRelease { closed: false, refs: entry.refs(), state: entry.state };
        }
        let remaining = entry.take_reference();
        TunnelRelease { closed: remaining == 0, refs: remaining, state: entry.state }
    }
}
";
    let violations = drain_returns_a_count_from_the_wrong_bank(legit);
    assert!(
        violations.is_empty(),
        "合法的取值来源被误伤：{violations:?}"
    );
}

/// **kill test（R6）**：把登记表裁小这件事本身必须转红；同名 `drain` 必须数得出来。
#[test]
fn shrinking_the_gate_itself_is_a_failure() {
    let declared = declared_modules(&blank(source("src/tunnel/mod.rs")));
    assert!(
        unregistered_modules(&declared, AUDITED.iter().map(|(path, _)| *path)).is_empty(),
        "真实登记表不完整"
    );
    let shrunk: Vec<&str> = AUDITED
        .iter()
        .map(|(path, _)| *path)
        .filter(|path| *path != HARNESS)
        .collect();
    let found = unregistered_modules(&declared, shrunk.iter().copied());
    assert_eq!(
        found,
        vec![HARNESS.to_owned()],
        "把夹具端口从登记表里裁掉必须被点名 —— 否则第二本账可以在没人看的地方长出来"
    );

    // 「唯一归零路径只许一条」这一半：两条同名 `drain` 必须都数出来，
    // 且各自归到正确的 impl 块。
    let two_drains = "\
impl TunnelLedger {
    fn drain(&mut self, index: usize) -> u32 {
        0
    }
}
impl TunnelEntry {
    fn drain(&mut self) -> u32 {
        self.refs()
    }
}
";
    let code = blank(two_drains);
    let impls = impl_blocks(&code);
    let owners: Vec<Option<String>> = functions(&code)
        .iter()
        .filter(|f| f.name == "drain")
        .map(|f| enclosing_impl(f, &impls))
        .collect();
    assert_eq!(
        owners.len(),
        2,
        "解析器必须看得见两条同名 `drain`，实得 {owners:?}"
    );
    assert_eq!(owners[0].as_deref(), Some("TunnelLedger"));
    assert_eq!(owners[1].as_deref(), Some("TunnelEntry"));
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
        code.contains("struct P { n: Mutex<usize> }"),
        "`b'\\\"'` 之后的代码被吞掉了 —— 闸门会安静地少扫后半片"
    );
    assert_eq!(
        code.lines().count(),
        sample.lines().count(),
        "抹串必须保留行数，否则报告的行号会把人带偏"
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
    // `shadow_refs:` 不能被读成 `refs:`（否则 R2 补充会点到错位置）。
    assert!(
        offsets_of("struct E {\n    shadow_refs: u32,\n}", "refs:").is_empty(),
        "取值点扫描把 `shadow_refs:` 误认成 `refs:`"
    );
    assert_eq!(
        offsets_of("TunnelRelease { refs: remaining }", "refs:").len(),
        1,
        "取值点扫描漏掉了真的 `refs:`"
    );
    assert_eq!(
        line_of("a\nb\nstruct P { n: Mutex<usize> }\n", "Mutex<usize>"),
        Some(3),
        "行号基元错位"
    );
    assert_eq!(char_literal_len("&'static str".as_bytes(), 1), None);
    assert_eq!(char_literal_len("'a".as_bytes(), 0), None);
    assert_eq!(char_literal_len("b'x'".as_bytes(), 0), Some(4));
    assert_eq!(char_literal_len("'x'".as_bytes(), 0), Some(3));
}
