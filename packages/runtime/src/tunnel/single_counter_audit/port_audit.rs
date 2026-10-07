//! 唯一计数铁律闸门的**端口那一半**：R4（端口不得自存账）与 R5（读数只能是折法的投影）。
//!
//! 拆成独立文件是为了把 `mod.rs` 拉回规模线以内
//! （同 `kill_tests.rs` / `gate_self_audit.rs` 的先例），**不是**为了让这两条规则少做：
//! 下面两条用例仍是 `#[cfg(test)]` 用例，随 `--lib` 一道跑；本文件自己也在闸门登记表
//! （[`AUDITED`]）里，R4 / R5 的扫描面覆盖它，`R6` 另有 `the_audit_registry_covers_every_tunnel_module`
//! 断言这份登记必须如实存在 —— 拆文件若忘了登记，那条用例会转红。
//!
//! **契约随实现同住**：R4 / R5 的判据、「端口读数」的划定方式（[`port_readings`]）、
//! 以及「收窄不许把管辖面收成 0」那两条自证全在下面，读这两条规则不需要再翻
//! `mod.rs` 的模块文档。通用扫描基元（含 [`struct_fields`]）住在
//! `src/tunnel/source_scan.rs`，这里只经 `super::*` 取用。

use super::*;

/// 「一份自存的账」的字段类型形状：剥掉包装后是整数标量，或含原子类型。
const INTEGER_TYPES: &[&str] = &[
    "usize", "isize", "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128",
];

/// 内部可变性 / 智能指针包装。`Mutex<usize>` 与 `usize` 在「能不能藏一份账」上
/// 没有区别 —— 这正是那个反例的立足点，所以先剥包装再判。
const WRAPPERS: &[&str] = &["Mutex", "RwLock", "Arc", "Box", "Option", "Cell", "RefCell"];

// ------------------------------------------------------------------ R4 / R5

/// R4：实现 [`TunnelTransport`] 的结构体不许存有「一份账」的字段。
pub(super) fn transport_stores_its_own_count(text: &str) -> Vec<String> {
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
///
/// R7（台账侧的镜像账判据）与 R3（观测账点名集合）复用它，所以它 `pub(super)`：
/// 那两条规则住在 `mod.rs`，共享这一份「剥包装 + 递归展开」的判据，不许各写一份。
pub(super) fn is_stored_count(text: &str, ty: &str, depth: u8) -> bool {
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
///
/// 「读数」由 [`port_readings`] 划定管辖面 —— **取数路径闭合**，不是返回类型。
pub(super) fn counts_not_derived_from_the_single_fold(text: &str) -> Vec<String> {
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
    let impls = impl_blocks(&code);
    for blk in transport_impls(&code) {
        for f in port_readings(&code, &impls, &blk.type_name) {
            let body = &code[f.body.0..f.body.1];
            if !folds.iter().any(|fold| body.contains(fold)) {
                out.push(format!(
                    "端口 `{}` 的读数函数 `{}` 返回计数 `{}` 却没调用唯一折函数 {folds:?}",
                    blk.type_name, f.name, f.returns
                ));
            }
        }
    }
    out
}

/// R5 的**管辖面**：一份文件里真正算「端口读数」的整数返回方法。
///
/// 判据是**取数路径闭合**，不是返回类型。开放世界里 `-> u64` 既可能是一份账
/// （`opened()`），也可能只是个时间戳（`TestClock::now_nanos`）；只看返回类型就是把
/// 这两者一视同仁 —— 这正是 R5 误报 `now_nanos` 的根因。同理 `ScriptedTransport`
/// 是物理接缝替身（`impl PhysicalTransport`），它的 `opened/closed` 读的是物理事件，
/// 不是隧道账，不归 R5 管。
///
/// 一个方法算「端口读数」，当且仅当两条同时成立：
///
/// 1. 它挂在**实现了 [`TunnelTransport`] 的那个类型**上（[`enclosing_impl`]，trait
///    实现解析成 `{trait} for {type}` 后 `rsplit(" for ")` 取回类型名）；
/// 2. 它**直接或经 `self.<method>()` 传递地**读到该类型**自己的字段**。
///
/// 第 2 条必须传递：合格写法就是 `fn opened(&self) -> usize { tally(&self.lock()).opened }`
/// —— 字段只出现在 `lock()` 里。字面匹配 `self.<字段>` 会让 `cm28_concurrent_tunnel.rs`
/// 与 `tunnel_refcount_contract.rs` 这两份**参考实现本身**掉出管辖面，R5 立刻变成
/// 「什么都不扫却报告没发现违规」的摆设 —— 比误报更安静，也更糟。
///
/// 返回类型那一条不能省：`bool` 是整数条件之外的答案，`fn open_fails(&self) -> bool`
/// 读端口自己的字段完全合法（见 `tunnel_arch_support/mod.rs`）。
fn port_readings(code: &str, impls: &[ImplBlock], port: &str) -> Vec<Func> {
    let fields: Vec<String> = struct_fields(code, port)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let own: Vec<Func> = functions(code)
        .into_iter()
        .filter(|f| f.params.contains("self") && owner_type(f, impls) == port)
        .collect();
    // 种子：体内直接读自己字段的方法。
    let mut on_path: Vec<String> = own
        .iter()
        .filter(|f| fields.iter().any(|field| touches(code, f, field)))
        .map(|f| f.name.clone())
        .collect();
    // 闭包：经 `self.<method>` 走到种子上的方法，同样在这条取数路径上。
    loop {
        let before = on_path.len();
        for f in &own {
            if on_path.contains(&f.name) {
                continue;
            }
            if on_path.iter().any(|name| touches(code, f, name)) {
                on_path.push(f.name.clone());
            }
        }
        if on_path.len() == before {
            break;
        }
    }
    own.into_iter()
        .filter(|f| on_path.contains(&f.name))
        .filter(|f| INTEGER_TYPES.contains(&strip_space(&f.returns).as_str()))
        .collect()
}

/// 一个 `fn` 挂在哪个类型上（`impl Foo` → `Foo`，`impl Tr for Foo` → `Foo`）。
///
/// 没有外层 impl（自由函数）返回空串，于是永远不会被判成任何端口的读数。
fn owner_type(f: &Func, impls: &[ImplBlock]) -> String {
    enclosing_impl(f, impls)
        .map(|owner| owner.rsplit(" for ").next().unwrap_or_default().to_owned())
        .unwrap_or_default()
}

/// 函数体里有没有 `self.<member>` —— 字段访问与方法调用一视同仁（`self.events` 与
/// `self.events()` 都算），且必须**完整**匹配成员名：`self.events` 不算命中
/// `events_len`，否则相邻字段名会互相污染。
fn touches(code: &str, f: &Func, member: &str) -> bool {
    let body = &code[f.body.0..f.body.1];
    let needle = format!("self.{member}");
    body.match_indices(&needle)
        .any(|(at, _)| match body.as_bytes().get(at + needle.len()) {
            Some(next) => !(next.is_ascii_alphanumeric() || *next == b'_'),
            None => true,
        })
}

/// 折函数的形参是不是「一段事件的借用」—— 共享切片 / 独占切片 / `Vec`，四种合理写法都认。
///
/// 只认 `&[` 会把完全合法的折法打成 `实得 0 个：[]`，随后 R5 反过来抱怨端口
/// 「没调用唯一折函数 []」——**诊断与事实相反**：两个都指向一个根本没坏的签名。
/// （`&[` 不匹配 `&mut [`，也不匹配 `&Vec<`，所以四种都要写出来。）
fn is_fold_param(params: &str) -> bool {
    ["&[", "&mut [", "&Vec<", "&mut Vec<"]
        .iter()
        .any(|p| params.contains(p))
}

/// 一个文件里的「事件流 → 账」折函数：形参收一段事件的借用、不接 `self`、体内做累加且不写 `self.`。
///
/// `pub(super)` 是因为 R6 的自证用例（`the_scan_primitives_see_the_shapes_they_claim`）
/// 与两条 kill test 都要在真实夹具上断言「这个文件里恰好一份折法」—— 判据搬家，
/// 证词不许跟着搬家。
pub(super) fn fold_functions(text: &str) -> Vec<String> {
    functions(text)
        .into_iter()
        .filter(|f| is_fold_param(&f.params) && !f.params.contains("self"))
        .filter(|f| {
            let body = &text[f.body.0..f.body.1];
            body.contains("+= 1") && !body.contains("self.")
        })
        .map(|f| f.name)
        .collect()
}

// ---------------------------------------------------------------------- 用例

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
         夹具 + 三条测试轨端口）。实得 {names:?}"
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
        let code = blank(text);
        let ports = transport_impls(&code);
        if ports.is_empty() {
            continue;
        }
        transport_files += 1;
        let impls = impl_blocks(&code);
        for blk in &ports {
            // 收窄判据不许把管辖面收成 0：收窄是为了**去掉误报**，不是为了放过真违规。
            // 管辖面一旦为空，R5 就什么都不扫却报告「没发现违规」—— 那比误报危险得多。
            let judged = port_readings(&code, &impls, &blk.type_name);
            assert!(
                !judged.is_empty(),
                "{path}: 收窄后 R5 管辖面为空 —— 端口 `{}` 的整数读数一个都没被判到，\
                 它完全可以绕过唯一折法私藏第二本账",
                blk.type_name
            );
        }
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

    // ---- 收窄判据的两条自证（都种在扫描器输入里，不依赖外部变异） ----
    //
    // **有牙**：端口自己的字段里长出一份整数账、有方法直读它 ⇒ 必须被点名。
    // 这正是「在 `RecordingTunnelPort` 上加 `local_tunnels: AtomicU64` + `fn cheat(&self)
    // -> usize`」那个变异的形状，钉在树里，免得日后有人把收窄顺手当放宽。
    let cheater = "\
struct P { events: Mutex<Vec<TunnelEvent>>, local_tunnels: AtomicU64 }
fn tally(events: &[TunnelEvent]) -> usize {
    let mut n = 0;
    for event in events {
        let _ = event;
        n += 1;
    }
    n
}
impl P {
    fn opened(&self) -> usize { tally(&self.events) }
    fn cheat(&self) -> usize { self.local_tunnels.load(Ordering::Relaxed) }
}
";
    let caught = counts_not_derived_from_the_single_fold(&with_impl(cheater, "P"));
    assert!(
        caught.iter().any(|v| v.contains("cheat")),
        "端口在自己字段里私藏一份计数，R5 却没点名 `cheat` —— 收窄把闸门的牙拔了：{caught:?}"
    );

    // **反面对照**：同一份文件里不属于这个端口的整数读数**不算**违规。
    // `now_nanos` 是时间戳（挂在 `TestClock` 上）、`ScriptedTransport` 是物理接缝替身
    // （挂在另一个类型上）—— 开放世界里 `-> u64` / `-> usize` 只说明类型，不说明它是一份账。
    let neighbours = "\
struct P { events: Mutex<Vec<TunnelEvent>> }
struct TestClock { nanos: u64 }
fn tally(events: &[TunnelEvent]) -> usize {
    let mut n = 0;
    for event in events {
        let _ = event;
        n += 1;
    }
    n
}
impl P {
    fn opened(&self) -> usize { tally(&self.events) }
}
impl MonotonicSource for TestClock {
    fn now_nanos(&self) -> u64 { self.nanos }
}
";
    let clean = with_impl(neighbours, "P");
    let scoped = port_readings(&clean, &impl_blocks(&clean), "P");
    assert_eq!(
        scoped.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["opened"],
        "R5 管辖面应只剩端口自己的读数；`now_nanos`（另一个类型）不在其中"
    );
    require_clean(
        "收窄判据误伤了不属于该端口的整数读数",
        counts_not_derived_from_the_single_fold(&clean),
    );
}
