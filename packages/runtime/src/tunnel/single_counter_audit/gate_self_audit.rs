//! CM-32-FU1 闸门的 **R6：闸门查自己**。
//!
//! 前面那些规则都在查台账与端口；这一份查**闸门本身**：登记表有没有被裁小、
//! 名字常量有没有和真实符号脱钩、扫描基元是不是真的看得见它声称看得见的形状。
//! 闸门不看自己，「悄悄少扫一片」就没有反制手段 —— 文本闸门最典型的失效形态不是
//! 漏抓一个形状，而是整张网静默作废后仍然报告「没发现违规」。
//!
//! 拆成独立文件是为了让 `mod.rs` 停在单文件规模线以内（见 AGENTS.md「单文件规模与模块拆分」），
//! 不是为了让这条规则可以少做：`#[cfg(test)]` 随 `--lib` 一道跑，且本文件自己也在
//! 闸门登记表里（`AUDITED`），扫描面覆盖它。

use super::*;

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
        GATE,
        "src/tunnel/single_counter_audit/kill_tests.rs",
        "src/tunnel/single_counter_audit/gate_self_audit.rs",
        "src/tunnel/single_counter_audit/port_audit.rs",
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
pub(super) fn declared_modules(mod_rs_blank: &str) -> Vec<String> {
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
pub(super) fn unregistered_modules<'a>(
    declared: &[String],
    registry: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let registry: Vec<&str> = registry.collect();
    declared
        .iter()
        .map(|name| {
            let flat = format!("src/tunnel/{name}.rs");
            let nested = format!("src/tunnel/{name}/mod.rs");
            (flat, nested)
        })
        // 平铺与目录两种模块形态都承认，但**至少一个**必须在登记表里 ——
        // 否则「把闸门拆成目录」这件事本身就成了绕过登记表检查的路。
        .filter(|(flat, nested)| !registry.iter().any(|path| *path == flat || *path == nested))
        .map(|(flat, _)| flat)
        .collect()
}

/// **R6 补**：磁盘上每一份实现 [`TunnelTransport`] 的文件都必须在登记表里。
///
/// 上一版只对测试轨硬编了两条 `include_str!`，而 `EXPECTED_TRANSPORT_IMPLS` /
/// `EXPECTED_TRANSPORT_FILES` **正是对着这份硬编名单数出来的** —— 名单外的第 4 份
/// 端口贡献 0、断言照样过。实测反例：新建 `tests/tunnel_new_probe.rs`，里面一个端口
/// 自存 `close_tally: Mutex<usize>`，`--lib` 报 `461 passed; 0 failed` 无一转红。
/// 那是 M11（新生产模块逃逸）那个洞平移了一层目录，而**历史假绿恰在测试轨**。
///
/// 因此这里不再信硬编名单，而是在**运行时**枚举 `src/` 与 `tests/` 两棵子树，
/// 对每个文件跑与 R4 / R5 **同一个** [`transport_impls`] 判据（口径不许分叉）。
///
/// 枚举本身必须先自证有效，否则「`read_dir` 失败 ⇒ 列表为空 ⇒ 全部放行」就是一个
/// 比原洞更安静的新洞：这里因此钉住两条 —— 列表非空、且必须含夹具文件。
#[test]
fn every_transport_implementing_file_on_disk_is_registered() {
    let on_disk = port_files_on_disk();
    let registered: Vec<&str> = AUDITED.iter().map(|(path, _)| *path).collect();
    let unregistered: Vec<&String> = on_disk
        .iter()
        .filter(|path| !registered.contains(&path.as_str()))
        .collect();
    require_clean(
        "磁盘上实现了 TunnelTransport 却没进审计登记表 —— \
         这份端口不会被 R4 / R5 扫到，它完全可以私藏第二本账",
        unregistered.iter().map(|p| p.to_string()).collect(),
    );
    assert!(
        on_disk.contains(&HARNESS.to_owned()),
        "枚举结果里连夹具文件 {HARNESS} 都没有 —— 枚举本身已经失效，\
         这条规则正在空转（实得 {on_disk:?}）"
    );
    assert!(
        on_disk.len() >= 3,
        "磁盘上只认出 {on_disk:?} —— 递归枚举没走到子目录（tests/ 下就有 common/、\
         gateway_contract/ 之类的子目录）"
    );
}

/// 磁盘上真实实现了 [`TunnelTransport`] 的文件（相对 crate 根，正斜杠分隔）。
fn port_files_on_disk() -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for sub in ["src", "tests"] {
        collect_port_files(&root.join(sub), root, &mut out);
    }
    out.sort();
    out
}

/// 递归收集子树里含端口实现的 `.rs` 文件。读不动的文件**静默跳过**——但
/// [`every_transport_implementing_file_on_disk_is_registered`] 的两条自证断言会先把它顶红。
fn collect_port_files(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_port_files(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !transport_impls(&blank(&text)).is_empty() {
                out.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
}

/// **闸门的名字常量不许只是手打字符串。**
///
/// 整套 impl 过滤器都拿字符串比对。名字若与真实符号脱钩（trait 改名、端口搬走、
/// 路径漂移），过滤器就扫不到任何 impl，于是**每一条**正向用例都会报告「没发现违规」——
/// 那是文本闸门最典型的失效形态：不是漏抓一个形状，而是整张网静默作废。
/// 这里做**双重**绑定：
///
/// 1. 编译期：本用例直接按路径写 `super::super::harness::RecordingTunnelTransport` 与
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
    let port = std::any::type_name::<super::super::harness::RecordingTunnelTransport>();
    assert!(
        port.ends_with(HARNESS_PORT_TYPE),
        "常量 HARNESS_PORT_TYPE = {HARNESS_PORT_TYPE:?} 与真实端口类型 `{port}` 脱钩"
    );
    // 夹具端口**确实**实现这个 trait —— 由类型系统证明，而不是由文本扫描证明。
    let transport: std::sync::Arc<dyn TunnelTransport> =
        std::sync::Arc::new(super::super::harness::RecordingTunnelTransport::new());
    assert!(
        transport_impls(&blank(source(HARNESS))).len() == 1,
        "扫描器在夹具文件里认下的端口实现不是恰好一份 —— impl 过滤器已经失配"
    );
    // 台账的权威计数按值交出（u32），与 `TunnelEntry::refs` 同源。
    let harness = super::super::harness::TunnelHarness::new();
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
