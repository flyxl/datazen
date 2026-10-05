//! §4 锁定不变量：冻结 DTO 只消费不改写、端口错误一律是 `RuntimeError`、
//! 模块路径固定、单文件 ≤800 行、生产路径无 `unwrap/expect/panic!/unsafe/#[allow]/sleep`。

use super::{err, runtime_err};
use crate::gateway_fixtures as fx;
use datazen_runtime::connection::{
    Counter, ExecutionId, ExecutionReceipt, ExecutionState, RuntimeError, SessionHandle,
    SessionView, StreamId,
};
use datazen_runtime::registry::SessionPort;

// ───────────────── H §4 不变量 ─────────────────

/// 端口接缝上流动的必须是 `connection` 里那份冻结 DTO，网关不得自造同形类型。
///
/// 特别地：`registry` 目前只转出了 `SessionPort`（§10 要求的 `registry::SessionView` /
/// `SessionHandle` 转出在基线 `060053afb` 里并不存在），因此这里按「消费定义处」锁定：
/// `SessionPort` 的入参/出参、`ExecutionRecord` 的句柄，全部是同一个 `connection::SessionHandle`。
#[tokio::test(start_paused = true)]
async fn the_port_seam_speaks_the_frozen_connection_dtos() {
    let h = fx::ready_harness();
    let port: &dyn SessionPort = h.port.as_ref();
    let handle: SessionHandle = fx::session_handle();
    let view: SessionView = port.session_view(&handle).await.expect("会话投影可读");
    assert_eq!(view.handle, handle);
    assert_eq!(view.handle.db_session_id.as_str(), fx::SESSION);

    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    let record = super::record(&h, &id).await;
    let from_record: SessionHandle = record.handle().clone();
    assert_eq!(
        from_record, handle,
        "执行记录持有的必须是同一个冻结句柄类型"
    );
    assert_eq!(from_record.runtime_epoch, Counter::new(1));
}

#[test]
fn frozen_dtos_keep_their_json_shape() {
    let view = fx::ready_view();
    let json = serde_json::to_value(&view).expect("会话视图可序列化");
    let keys: Vec<&String> = json.as_object().expect("会话视图是对象").keys().collect();
    let expected = [
        "activeExecutionId",
        "attachmentState",
        "configRevision",
        "connectionId",
        "contextRevision",
        "expiresAt",
        "handle",
        "initialTarget",
        "observedContext",
        "owner",
        "state",
    ];
    assert_eq!(keys.len(), expected.len(), "会话视图字段集变了：{keys:?}");
    for key in expected {
        assert!(keys.iter().any(|k| k.as_str() == key), "缺少字段 {key}");
    }

    let receipt = ExecutionReceipt {
        execution_id: ExecutionId::new("exe_contract_json"),
        stream_id: StreamId::new("stream-contract"),
        state: ExecutionState::Queued,
    };
    let receipt_keys: Vec<String> = serde_json::to_value(&receipt)
        .expect("回执可序列化")
        .as_object()
        .expect("回执是对象")
        .keys()
        .cloned()
        .collect();
    // serde_json::Map 默认是 BTreeMap（没开 preserve_order），回来的是字典序。
    // 这里锁的是**键集合**——多一个字段、少一个字段都要变红。
    let mut sorted = receipt_keys.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["executionId", "state", "streamId"]);
}

#[tokio::test(start_paused = true)]
async fn port_failures_arrive_as_runtime_errors() {
    let h = fx::ready_harness();
    h.port
        .set_view(Err(RuntimeError::UnknownSession(fx::SESSION.to_string())));
    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);

    // D-05：端口只给 RuntimeError，网关不得换皮成自有错误类型。
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::UnknownSession(fx::SESSION.to_string())
    );
    assert_eq!(error.to_persistable_json()["kind"], "runtime");
}

/// 去掉注释与字符串字面量，只留结构字符，用于配平花括号。
fn code_only(line: &str) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return String::new();
    }
    let mut out = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            _ => out.push(ch),
        }
    }
    out
}

/// 剔除 `#[cfg(test)] mod x { … }` 整块，只留下生产代码行；返回（原始行号，代码行）。
fn production_lines(source: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let code: Vec<String> = lines.iter().map(|line| code_only(line)).collect();
    let mut keep = vec![true; lines.len()];
    let mut index = 0;
    while index < lines.len() {
        if code[index].trim() != "#[cfg(test)]" {
            index += 1;
            continue;
        }
        let mut head = index + 1;
        while head < lines.len() && code[head].trim().is_empty() {
            head += 1;
        }
        let is_block = head < lines.len()
            && (code[head].trim_start().starts_with("mod ")
                || code[head].trim_start().starts_with("pub mod "))
            && code[head].trim_end().ends_with('{');
        if !is_block {
            index += 1;
            continue;
        }
        let mut depth = 0_i32;
        let mut cursor = head;
        loop {
            depth += code[cursor].matches('{').count() as i32;
            depth -= code[cursor].matches('}').count() as i32;
            if depth <= 0 || cursor + 1 >= lines.len() {
                break;
            }
            cursor += 1;
        }
        for slot in keep.iter_mut().take(cursor + 1).skip(index) {
            *slot = false;
        }
        index = cursor + 1;
    }
    let mut kept: Vec<(usize, String)> = Vec::new();
    for (index, (line, keep)) in lines.iter().zip(keep.iter()).enumerate() {
        if *keep {
            kept.push((index + 1, line.to_string()));
        }
    }
    kept
}

#[test]
fn gateway_sources_obey_the_locked_invariants() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("src/gateway");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("src/gateway 可读")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    let mut expected: Vec<String> = [
        // 测试专用：只编进 `#[cfg(test)]`，不碰生产路径扫描。
        "cancel_event_tests.rs",
        "event_store_tests.rs",
        "facade_support.rs",
        "facade_tests.rs",
        // 负例文件只编进 `#[cfg(doctest)]`，同样不进任何生产二进制。
        "request_cm06_negatives.rs",
        "retention_tests.rs",
        "testing_support.rs",
        "token_tests.rs",
        // 生产文件。
        "cancel.rs",
        "events.rs",
        "idempotency.rs",
        "mod.rs",
        "owner_binding.rs",
        "provenance.rs",
        "request.rs",
        "retention.rs",
        "timing.rs",
        "token.rs",
    ]
    .iter()
    .map(|name| name.to_string())
    .collect();
    expected.sort();
    assert_eq!(
        names, expected,
        "src/gateway 的文件集变了，扫描范围必须同步"
    );

    let test_only = [
        "cancel_event_tests.rs",
        "event_store_tests.rs",
        "facade_support.rs",
        "facade_tests.rs",
        "request_cm06_negatives.rs",
        "retention_tests.rs",
        "testing_support.rs",
        "token_tests.rs",
    ];
    for name in &names {
        let source = std::fs::read_to_string(dir.join(name)).expect("源码可读");
        assert!(
            source.lines().count() <= 800,
            "{name} 超过 800 行：{}",
            source.lines().count()
        );
        if test_only.contains(&name.as_str()) {
            continue;
        }
        for (line_no, line) in production_lines(&source) {
            for token in [
                ".unwrap(", ".expect(", "panic!(", "unsafe ", "#[allow", "sleep(",
            ] {
                assert!(
                    !line.contains(token),
                    "{name}:{line_no} 生产路径出现 {token}：{line}"
                );
            }
        }
    }

    // ⚠ 已登记、未修：**全仓**的 800 行红线只覆盖 `src/gateway` 与本文件下面的 `tests/`。
    //   * 结论：全仓有 **80 个** `.rs` 文件 ≥ 800 行，最大的
    //     `src-tauri/src/commands/schema_diff.rs` 有 3250 行。没有任何门禁盯着它们。
    //   * 为什么未修：把这 80 个补进下面的清单会**当场变红**（它们现在就在违反），
    //     把阈值改成「只对新文件生效」又等于给存量开口子——那是放宽规则，不是修规则。
    //     真修只能逐个按职责拆分，是独立的大范围改动，不该搭在本轨道上顺手做。
    //   * 怎么复现：`git ls-files '*.rs' | xargs wc -l | awk '$1>=800 && $2!="total"' | wc -l`
    //     换一个口径（`python3` 按 `splitlines()` 数）得到同一个数。必须先 `git ls-files`：
    //     直接 `find` / `read_dir` 会把 gitignored 的 `src-tauri/src/driver_init.rs`
    //     和 `target/` 下的构建产物一并数进来，得到的不是同一个问题。
    //   * 影响范围：这 80 个文件现在只受「评审时看一眼」约束，规模继续膨胀不会让任何
    //     测试变红。`AGENTS.md:256` 的原文是「**推荐**单文件不超过 800 行，严禁出现
    //     超大单文件」——「推荐」是建议性措辞。真正硬的只有本文件里两处
    //     `assert!(… <= 800 …)`：生产那处在本段上方（`:228`），测试那处在本段下方
    //     ——**下方那处故意不给行号**：这段注释就插在它前面，本段每增删一行都会把它顶下去，
    //     给它写死行号等于给自己埋一个必然过期的数字（连着栽了两次：320 → 336 → 338）。
    //     要定位就跑 `grep -n '<= 800' packages/runtime/tests/gateway_contract/invariants.rs`。
    //     写在这里是为了台账随合并删除后，这个缺口不会跟着消失。

    // 测试专用模块必须显式标 #[cfg(test)]，否则它们会进生产二进制。
    let mod_rs = std::fs::read_to_string(dir.join("mod.rs")).expect("mod.rs 可读");
    for (module, public) in [
        ("testing_support", "pub(crate) "),
        ("facade_support", "pub(crate) "),
        ("facade_tests", ""),
        ("cancel_event_tests", ""),
        ("event_store_tests", ""),
        ("retention_tests", ""),
        ("token_tests", ""),
    ] {
        let needle = format!("{public}mod {module};");
        let declared = mod_rs
            .lines()
            .position(|line| line.trim_end() == needle)
            .unwrap_or_else(|| panic!("mod.rs 必须声明 {needle}"));
        let previous = mod_rs
            .lines()
            .take(declared)
            .filter(|line| !line.trim().is_empty())
            .last()
            .unwrap_or_default()
            .trim()
            .to_string();
        assert_eq!(previous, "#[cfg(test)]", "{needle} 必须紧跟 #[cfg(test)]");
    }

    // 本轨自己的测试文件同样受 800 行约束。
    // ⚠ 已登记、未纳入：`tests/cm70_no_disk.rs` 实测 807 行，且此前**没有任何** 800 行
    //   门禁覆盖它。它是 CM-70 的分节注册表，补进下面的清单会立刻变红；拆分它属独立
    //   改动，本轮不擅自做。写在这里而不是只写进台账，是为了台账随合并删除后该缺口不消失。
    let mut test_files = vec![
        "tests/gateway_contract.rs".to_string(),
        "tests/gateway_fixtures/mod.rs".to_string(),
        "tests/cm70_idempotency_replay.rs".to_string(),
        "tests/owner_binding.rs".to_string(),
    ];
    for entry in std::fs::read_dir(root.join("tests/cm70")).expect("CM-70 分节用例目录可读")
    {
        let name = entry
            .expect("目录项可读")
            .file_name()
            .to_string_lossy()
            .to_string();
        if name.ends_with(".rs") {
            test_files.push(format!("tests/cm70/{name}"));
        }
    }
    let sections =
        std::fs::read_dir(root.join("tests/gateway_contract")).expect("分节用例目录可读");
    for entry in sections.filter_map(|entry| entry.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".rs") {
            test_files.push(format!("tests/gateway_contract/{name}"));
        }
    }
    // 本轨新增的测试目录同理：整目录纳入，不逐个硬编码文件名。
    let owner_binding =
        std::fs::read_dir(root.join("tests/owner_binding")).expect("owner_binding 用例目录可读");
    let mut owner_files: Vec<String> = owner_binding
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".rs"))
        .map(|name| format!("tests/owner_binding/{name}"))
        .collect();
    owner_files.sort();
    assert!(
        !owner_files.is_empty(),
        "tests/owner_binding/ 目录为空：这条扫描若悄悄失效，800 行门禁会漏掉本轨的测试文件"
    );
    test_files.extend(owner_files);
    for test in &test_files {
        let source = std::fs::read_to_string(root.join(test)).expect("测试源码可读");
        assert!(source.lines().count() <= 800, "{test} 超过 800 行");
    }
}
