//! 共享内核的**形态中立**闸门：内核源码里不得出现形态名（桌面 / Tauri / GUI / Web）。
//!
//! # 这条闸门为什么存在
//!
//! 内核（`datazen-runtime`）是 web 与桌面**共用**的那一半。形态名一旦写进内核，
//! 就等于把「只有桌面才成立」的假设焊进共享代码：桌面形态存在时看不出问题，
//! 换成 web 形态才发现那一半根本用不了。
//!
//! 本 crate 曾留着三处这样的焊点，全部是**桌面形态独占**的名字：
//!
//! * `route.rs` 的四个稳定字面码与租约前缀（`desktopRouteUnknown` 等）；
//! * `artifact` 端口的 `ExportSink::DesktopDialogHandle`（共享**端口**上的形态名）；
//! * `budget` 的额度档位（`DESKTOP_*` / `desktop_default()`）与连接夹具的同名字段。
//!
//! 它们当时都「能编译、全测绿」——形态名不构成编译错误，只构成**共享包不该有的
//! 假设**。靠人记着下次别写，是这类漏洞唯一会长出来的方式。
//!
//! # 规则（F1）
//!
//! 内核源码里的**字符串字面量**与**标识符**都不得含形态名。判据是**分词后逐词比**，
//! 不是子串比 —— 否则 `webhook` / `webdriver` 这类协议复合词会被误伤；误伤会逼人
//! 把合法写法藏起来，闸门反而变成漏洞。
//!
//! # 每条规则自带 kill test 与自证
//!
//! 只做匹配而不证明「它抓得到东西」的闸门比没有闸门更危险（它给出虚假保证）。
//! 故本文件有三类用例：
//!
//! * **kill test**：把**原始反例**（带形态名的字面量与标识符）当字符串喂给同一套
//!   判据，断言转红；
//! * **不误伤**：形态名出现在**注释**里、或落在**非形态词**（`webhook`）里时，
//!   断言不转红；
//! * **自证**：`the_scanner_actually_sees_the_stable_codes` 断言扫描器真的把内核里
//!   已知的稳定码读了出来 —— 否则「没发现违规」只意味着扫描器什么也没看见。
//!
//! # 闸门的边界（不留模糊措辞）
//!
//! * 它是**测试期的机械闸门**，不是类型系统保证：它查的是**名字**，管不了运行时行为。
//! * 它只认**拉丁字母**的形态名。中文散文里的「桌面」不会被它点红 —— 散文不是契约，
//!   契约是**字面量**与**标识符**。
//! * 它只扫内核 crate 的 `src/`。`tests/` **不扫**：那里有对**宿主 crate** 的合法引用
//!   （例如按路径读 `src-tauri/…/identity.rs` 验证宿主接线），那种路径里的形态名是
//!   真实存在的目录名，不是内核的形态假设。
//! * 蓄意对抗（把形态名拆成 `concat!`、用 `#[cfg]` 藏起来）挡不住。

use super::source_scan::{blank, literals};

/// 内核 crate 里被扫的子树。
const SRC: &str = "src";

/// 形态词表。
///
/// **刻意不写成完整字面量**：闸门扫的就是源码里的字面量，若这里直接写出完整的
/// 形态名，本文件会先被**自己的**规则点红。拆开拼，闸门才敢扫自己。
fn forbidden_words() -> Vec<String> {
    vec![
        ["desk", "top"].concat(),
        ["tau", "ri"].concat(),
        ["g", "ui"].concat(),
        ["w", "eb"].concat(),
    ]
}

/// 协议复合词：camelCase 切词后**相邻两段合并**成这些词时，整体不参与形态判定。
///
/// 「WebSocket」按 camelCase 切成 `web` + `socket`；`web` 单看是形态名，
/// 但 `WebSocket` 是**协议名**（WebSocket 隧道是隧道机制的一种物理开法，
/// 与「哪种形态」无关）。合并机制让这类词整体豁免，同时不影响真的形态名
/// ——`desktopTunnel` 切出 `desktop` + `tunnel`，合并不是已知复合词，照常判定。
///
/// 合并**只在同一个 chunk 内**发生（即 camelCase 相邻段），跨空格的两个词
/// 不合并：`web socket`（带空格）与 `WebSocket` 不是同一个词形。
const COMPOUND_WORDS: &[&str] = &["websocket", "webdriver", "webhook", "webassembly"];

/// 判据：把一段文本切成词。
///
/// 切法：先按**非字母数字**切，再按 camelCase 边界（小写/数字 → 大写）切，
/// 最后把相邻 camelCase 段合并成 [`COMPOUND_WORDS`] 里的协议复合词。
/// 纯 ASCII 输入，字节下标与字符下标一致。
fn words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for chunk in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if chunk.is_empty() {
            continue;
        }
        let bytes = chunk.as_bytes();
        let mut start = 0usize;
        let mut segments: Vec<&str> = Vec::new();
        for i in 1..bytes.len() {
            let prev = bytes[i - 1];
            let cur = bytes[i];
            if (prev.is_ascii_lowercase() || prev.is_ascii_digit()) && cur.is_ascii_uppercase() {
                segments.push(&chunk[start..i]);
                start = i;
            }
        }
        segments.push(&chunk[start..]);
        for segment in segments {
            let lower = segment.to_ascii_lowercase();
            if let Some(last) = out.last_mut() {
                let combined = format!("{last}{lower}");
                if COMPOUND_WORDS.contains(&combined.as_str()) {
                    *last = combined;
                    continue;
                }
            }
            out.push(lower);
        }
    }
    out
}

/// 文本里命中的形态词（去重后按出现序）。
fn form_words(text: &str, forbidden: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in words(text) {
        if forbidden.contains(&word) && !out.contains(&word) {
            out.push(word);
        }
    }
    out
}

/// 一段源码里的违规：`(位置说明, 形态词)`。
///
/// 扫两层：
///
/// * **字面量层** —— 用 [`literals`] 收集（注释里的字面量不算，注释不是契约）；
/// * **标识符层** —— 用 [`blank`] 抹掉注释与字面量，剩下的就是代码，其标识符要查。
fn violations(source: &str, forbidden: &[String]) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for literal in literals(source) {
        let hit = form_words(&literal.text, forbidden);
        if !hit.is_empty() {
            out.push((format!("字面量 {:?}", literal.text), hit));
        }
    }
    for word in words(&blank(source)) {
        if forbidden.contains(&word) {
            out.push((format!("标识符 {word}"), vec![word.clone()]));
        }
    }
    out
}

/// 被扫的共享 crate 根：本 crate、`datazen-kernel` 与 `modules`（DDD 迁移 D3 起
/// 机制内核的落位处）。
///
/// 机制内核（job / budget / sql_guard / schema_cache / driver_registry）从本 crate
/// 分出去以后，「形态中立」的理由一字未变——它同时服务多种形态。若闸门只扫本 crate，
/// 拆分就会安静地把那一片移出覆盖范围，而闸门仍然全绿（少扫等于没扫）。
/// D3 起 budget / sql_guard / driver_registry 的唯一定义在 `datazen-modules`，
/// 那一片必须跟着搬进扫描范围。
fn scanned_crates() -> Vec<(&'static str, std::path::PathBuf)> {
    // D3 迁移后本文件住在 modules；机制（tunnel/budget/…）也在这里，
    // 用例编排与集成测试仍在 datazen-runtime，job/schema_cache 仍在 kernel。
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
    let kernel = here.join("../packages/kernel");
    let runtime = here.join("../packages/runtime");
    vec![("modules", here), ("kernel", kernel), ("runtime", runtime)]
}

/// `modules` crate 里**豁免**的子树：`src/ai`。
///
/// AI Provider 适配（D1.5 并入）合法地处理 `web_search` 这类**工具名**与外部
/// URL——那是协议词汇，不是「哪种形态」的假设；豁免必须**只窄到这一棵子树**，
/// 其余模块目录（shared/connection/execution/…）照扫。
const MODULES_EXEMPT: &[&str] = &["src/ai"];

/// 两个共享 crate 的 `src/` 下全部 `.rs` 源码，形如 `runtime:src/job/mod.rs`。
fn source_files_on_disk() -> Vec<String> {
    let mut out = Vec::new();
    for (label, root) in scanned_crates() {
        assert!(
            root.is_dir(),
            "闸门找不到 {label} crate 的目录 {root:?} —— 少扫一片比不扫更危险：它还会报绿"
        );
        let mut files = Vec::new();
        let exempt: Vec<String> = if label == "modules" {
            let mut e: Vec<String> = MODULES_EXEMPT.iter().map(|s| s.to_string()).collect();
            // modules/tests 是跨功能契约测试，合法引用宿主路径。
            e.push("src/tests".to_string());
            e
        } else {
            Vec::new()
        };
        collect_rs_with_exemptions(&root.join(SRC), &root, &mut files, &exempt);
        assert!(
            !files.is_empty(),
            "{label} crate 的 src/ 下一个源码文件都没枚举到 —— 递归枚举断了"
        );
        for rel in files {
            out.push(format!("{label}:{rel}"));
        }
    }
    out.sort();
    out
}

fn collect_rs_with_exemptions(
    dir: &std::path::Path,
    root: &std::path::Path,
    out: &mut Vec<String>,
    exempt: &[String],
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if exempt.iter().any(|e| rel.starts_with(e.as_str())) {
                continue;
            }
            collect_rs_with_exemptions(&path, root, out, exempt);
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

/// 读一个源码文件的内容（相对 crate 根）。
fn read_source(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("读不到 {rel}：{err}（闸门必须真的看见源码）"))
}

/// 读被扫 crate 里的一处源码：`label` 与 [`scanned_crates`] 的标签一致。
fn read_scanned(label: &str, rel: &str) -> String {
    let (_, root) = scanned_crates()
        .into_iter()
        .find(|(known, _)| *known == label)
        .unwrap_or_else(|| panic!("未登记的 crate 标签 {label}"));
    let path = root.join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("读不到 {label}:{rel}：{err}（闸门必须真的看见源码）"))
}

/// 拼出一个**带形态名**的码。
///
/// 拆开拼的理由与 [`forbidden_words`] 相同：闸门不能在自己的源码里留下完整的
/// 形态名字面量 —— 那正是这条规则要抓的形状，不能由闸门自己提供反例。
fn planted_form_code() -> String {
    ["desk", "top"].concat()
}

/// 拼出**另一个**形态词。同样拆开拼，理由同上。
///
/// 函数名与调用处的变量名**都不能含形态词**——闸门扫的是标识符层，
/// 叫 `planted_web_word` 会被自己点红。
fn planted_other_form() -> String {
    ["w", "eb"].concat()
}

#[test]
fn the_kernel_has_no_form_name_in_its_code_or_literals() {
    let forbidden = forbidden_words();
    let mut bad: Vec<String> = Vec::new();
    for rel in source_files_on_disk() {
        let source = read_scanned_file(&rel);
        for (where_, hit) in violations(&source, &forbidden) {
            bad.push(format!("{rel}: {where_} → {hit:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "共享内核里出现了形态名：\n{}\n\
         形态专名只属于宿主；内核要同时服务多种形态，\
         任何把形态写进名字的地方都会让另一半形态用不了。",
        bad.join("\n")
    );
}

#[test]
fn the_gate_catches_a_planted_form_name_in_a_stable_code() {
    let forbidden = forbidden_words();
    // 原始反例一：字面量里带形态名。
    let planted_literal = format!(
        "fn f() {{ let reason = \"{}TunnelFailed\"; }}",
        planted_form_code()
    );
    assert!(
        !violations(&planted_literal, &forbidden).is_empty(),
        "闸门没抓到植入的字面量形态名 —— 规则形同虚设"
    );
    // 原始反例二：标识符里带形态名。
    let planted_ident = format!("fn f() {{ let {} = 1; }}", planted_form_code());
    assert!(
        !violations(&planted_ident, &forbidden).is_empty(),
        "闸门没抓到植入的标识符形态名 —— 规则形同虚设"
    );
}

#[test]
fn the_gate_does_not_flag_a_form_word_that_is_not_a_form_name() {
    let forbidden = forbidden_words();
    // 注释里的形态名不是契约（散文可以谈形态）。
    let in_comment = format!("// 这一段在讲{}的处理\nfn f() {{}}\n", planted_form_code());
    assert!(
        violations(&in_comment, &forbidden).is_empty(),
        "闸门把注释里的形态名也点红了 —— 判据必须是**契约**（字面量与标识符），不是散文"
    );
    // `webhook` / `webdriver` / `WebSocket` 这类协议复合词不是形态名。
    //
    // `WebSocket` 是**回归用例**：camelCase 切词把它切成 `web` + `socket`，
    // `web` 单看在形态词表里 —— 隧道机制里「WebSocket 隧道」是物理开法之一，
    // 与形态无关。判据若不做复合词合并，搬隧道进共享层的那天这道闸门就会转红，
    // 且报错原因（「形态名 web」）与真因（协议名）完全对不上。
    for legit in [
        "webhook",
        "webdriver",
        "WebSocket",
        "WebSocketTunnel",
        "Starting WebSocket tunnel",
    ] {
        let src = format!("fn f() {{ let reason = \"{legit}\"; }}");
        assert!(
            violations(&src, &forbidden).is_empty(),
            "闸门误伤了合法复合词 {legit} —— 误伤会逼人把合法写法藏起来"
        );
    }
    // 复合词豁免**不得**放宽到真的形态名上。
    //
    // 这些字面量**也必须拆开拼**：本文件自己被闸门扫，写出完整的形态名
    // 就会被自己的规则点红（连负控用例也不例外）。变量名同理。
    let form = planted_form_code();
    let other = planted_other_form();
    let mut capital = planted_form_code();
    capital.replace_range(0..1, &capital[..1].to_ascii_uppercase());
    for real in [
        format!("{form}Tunnel"),
        format!("{capital}Tunnel"),
        [other.as_str(), capital.as_str(), "Tunnel"].concat(),
        [other.as_str(), "_", form.as_str(), "_tunnel"].concat(),
    ] {
        let src = format!("fn f() {{ let reason = \"{real}\"; }}");
        assert!(
            !violations(&src, &forbidden).is_empty(),
            "闸门漏掉了真形态名 {real} —— 复合词豁免放宽过头了"
        );
    }
}

#[test]
fn the_scanner_actually_sees_the_stable_codes() {
    let forbidden = forbidden_words();
    // 扫描器必须真的把内核里的稳定码读出来：否则「没发现违规」只意味着它什么也没看见。
    let route = read_source("src/execution/runtime/tunnel/route.rs");
    let found: Vec<String> = literals(&route)
        .into_iter()
        .map(|l| l.text)
        .filter(|t| t.starts_with("tunnel") || t.starts_with("route"))
        .collect();
    for expected in [
        "routeUnknown",
        "tunnelReferenceNotHeld",
        "tunnelEndpointUnavailable",
        "tunnelTeardownUnconfirmed",
    ] {
        assert!(
            found.iter().any(|f| f == expected),
            "扫描器没在 route.rs 里看到稳定码 {expected} —— 闸门会安静地少扫一片（实得 {found:?}）"
        );
    }
    // 稳定码必须**无形态名**（正向断言：闸门的实际目标）。
    for code in &found {
        assert!(
            form_words(code, &forbidden).is_empty(),
            "稳定码 {code} 里带形态名 {:?} —— 形态名不进共享内核的对外文本",
            form_words(code, &forbidden)
        );
    }
}

/// 按 `label:rel` 读源码。
fn read_scanned_file(labeled: &str) -> String {
    let (label, rel) = labeled
        .split_once(':')
        .unwrap_or_else(|| panic!("扫描清单里的条目缺 crate 标签：{labeled}"));
    read_scanned(label, rel)
}

#[test]
fn the_scan_covers_the_whole_kernel_crate() {
    // 「没发现问题」必须建立在「确实看了全部文件」之上。
    let files = source_files_on_disk();
    assert!(
        files.len() >= 60,
        "只枚举到 {} 个源码文件 —— 递归枚举没走到子目录，闸门在少扫",
        files.len()
    );
    for required in [
        // 连接资源运行时机制（D3 起在 modules）
        "modules:src/execution/runtime/tunnel/route.rs",
        "modules:src/execution/runtime/tunnel/form_neutrality.rs",
        "modules:src/execution/runtime/artifact/mod.rs",
        "modules:src/execution/runtime/connection/testing/fixtures.rs",
        // 机制内核：留在 kernel 的 job/schema_cache 与本 crate 的机制都必须被扫
        "kernel:src/job/host.rs",
        "kernel:src/schema_cache/schema_cache.rs",
        "modules:src/execution/runtime/budget/quotas.rs",
        "modules:src/execution/domain/sql_guard/mod.rs",
        "modules:src/execution/runtime/driver_registry/registry.rs",
        "modules:src/shared/id.rs",
        // 用例编排随机制迁入本 crate（过渡期整块），继续被扫
        "modules:src/execution/runtime/application/mod.rs",
        // runtime 留守面（native_tool + 集成测试夹具）也必须被扫
        "runtime:src/native_tool/handler.rs",
        // 豁免必须真的生效：src/ai 不得出现在清单里
        // （若豁免失效，ai 子树的文件会出现在 files 里并被形态词点红）。
    ] {
        assert!(
            !files.iter().any(|f| f.starts_with("modules:src/ai/")),
            "modules:src/ai 应被豁免却出现在扫描清单里"
        );
        assert!(files.iter().any(|f| f == required), "闸门没覆盖 {required}");
    }
    // 闸门自己也在被扫范围内：它自己的**字面量**里若有完整的形态名，
    // 上面那条会红。这里单独点一遍，是为了让失败信息直接指到本文件
    // （注释里谈形态是允许的，故只看字面量）。
    let self_source = read_source("src/execution/runtime/tunnel/form_neutrality.rs");
    for literal in literals(&self_source) {
        assert!(
            !literal.text.contains(&planted_form_code()),
            "闸门文件里写出了完整的形态名字面量 {:?} —— 它会被自己的规则点红",
            literal.text
        );
    }
}
