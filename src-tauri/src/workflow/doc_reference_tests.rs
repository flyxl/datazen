//! `docs/architecture/platform/workflow-resource-model.md` 的**引用新鲜度**守卫。
//!
//! 长期文档里的「行号 + 事实断言」最容易悄悄过期：代码一改，行号漂了；
//! 断言没跟着改，读者按文档去读代码就会读到别的东西。仓库里没有校验 Markdown
//! 引用的脚本（见 AGENTS.md「文档维护纪律」），只能靠人工/代理检查——这组用例
//! 把其中最容易漂的一部分钉住。
//!
//! 三件事：
//!
//! 1. 文档里的**相对链接**指到的文件必须真的存在（`docs/` 内可达）。
//! 2. 文档点名的**行号**必须还指着它声称的那一行内容。
//! 3. 文档里的**驱动数量事实**（`qualify_sql_target` 的实现方）必须与代码一致。
//!
//! 行号会随任何代码改动漂移——那正是本组用例存在的理由：**代码动了，文档必须
//! 跟着动**，否则这里先红。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 仓库根（`src-tauri/` 的上一级）。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri 必须在仓库根下")
        .to_path_buf()
}

/// 被守的长期文档。
const DOC_RELATIVE: &str = "docs/architecture/platform/workflow-resource-model.md";

fn doc_path() -> PathBuf {
    repo_root().join(DOC_RELATIVE)
}

fn doc_text() -> String {
    std::fs::read_to_string(doc_path())
        .unwrap_or_else(|error| panic!("{} 应当可读：{error}", DOC_RELATIVE))
}

/// 文档里的相对链接必须指向真实存在的文件。
#[test]
fn the_document_links_reach_real_files() {
    let doc = doc_text();
    let doc_dir = doc_path().parent().expect("文档必有所在目录").to_path_buf();

    let mut broken: Vec<String> = Vec::new();
    let mut checked = 0usize;

    // Markdown 行内链接：[text](target)
    for line in doc.lines() {
        let mut rest = line;
        while let Some(open) = rest.find("](") {
            rest = &rest[open + 2..];
            let Some(close) = rest.find(')') else { break };
            let target = &rest[..close];
            rest = &rest[close + 1..];

            let target = target.split_whitespace().next().unwrap_or("");
            if target.is_empty()
                || target.starts_with('#')
                || target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            // 只看相对路径；锚点（`file.md#section`）不参与可达性判断。
            let path_part = target.split('#').next().unwrap_or("");
            if path_part.is_empty() {
                continue;
            }
            checked += 1;
            let resolved = doc_dir.join(path_part);
            if !resolved.exists() {
                broken.push(format!("{target}（解析为 {}）", resolved.display()));
            }
        }
    }

    assert!(
        checked > 20,
        "取样器应当取到文档里的大部分相对链接，实际只取到 {checked} 个"
    );
    assert!(
        broken.is_empty(),
        "文档里的相对链接必须指向真实存在的文件：\n{}",
        broken.join("\n")
    );
}

/// 文档点名的行号必须还指着它声称的那一行。
///
/// 每一项是 `(仓库内相对路径, 1-based 行号, 该行必须包含的片段)`。
const LINE_REFERENCES: &[(&str, usize, &str)] = &[
    // §2.3：version 写入点（本轨自己顶掉的 / 逐条核对过仍正确的）。
    ("src-tauri/src/workflow/registry.rs", 226, "version: None,"),
    (
        "packages/feature-core/src/workflow/registry.rs",
        437,
        "version: None,",
    ),
    // §2.4：目标解析的两行代码块。
    (
        "packages/feature-core/src/workflow/executor.rs",
        130,
        "let workflow_connection = workflow.connection.as_deref().or(connection_id);",
    ),
    (
        "packages/feature-core/src/workflow/executor.rs",
        131,
        "let workflow_database = workflow.database.as_deref();",
    ),
    // §2.4：目标定位策略注释的现居所（原 `command_runtime.rs:115-119` 已失效）。
    // 文档指的是 59-62 这**四行注释块**，逐行钉住它的首尾与关键 token。
    (
        "src-tauri/src/commands/driver_command/execute.rs",
        59,
        "F7: pass envelope targeting into the command input",
    ),
    (
        "src-tauri/src/commands/driver_command/execute.rs",
        60,
        "qualify_sql_target",
    ),
    (
        "src-tauri/src/commands/driver_command/execute.rs",
        62,
        "never switches the session's database",
    ),
    // §2.5：调度器不包裹 Job（原 `scheduler.rs:176` 越界）。
    (
        "src-tauri/src/workflow/scheduler.rs",
        44,
        "crate::commands::workflow_execute_impl(",
    ),
    ("src-tauri/src/workflow/scheduler.rs", 48, "None,"),
    // §2.5：Dashboard 同样不传连接（原 `dashboard/execute.rs:222`）。
    (
        "src-tauri/src/dashboard/execute.rs",
        71,
        "WorkflowExecutor::execute(",
    ),
    // §2.5：migration step 把 job_id 传 None（原 `migration.rs:509`）。
    ("src-tauri/src/workflow/migration.rs", 221, "job_id: None,"),
    // §2.3：逐条核对过仍正确的若干处（抽样）。
    (
        "packages/feature-core/src/dashboard/create.rs",
        40,
        "version: None,",
    ),
    (
        "src-tauri/src/commands/ai/integration_tests.rs",
        377,
        "version: Some(\"1\".into()),",
    ),
    (
        "packages/feature-core/src/workflow/scheduler.rs",
        223,
        "version: None,",
    ),
    (
        "packages/feature-core/src/dashboard/archive.rs",
        185,
        "version: None,",
    ),
    (
        "src-tauri/src/store/settings.rs",
        15,
        "pub struct OnboardingState {",
    ),
];

#[test]
fn the_document_line_references_still_point_at_what_they_claim() {
    let mut stale: Vec<String> = Vec::new();

    for (relative, line, expected) in LINE_REFERENCES {
        let path = repo_root().join(relative);
        let Ok(text) = std::fs::read_to_string(&path) else {
            stale.push(format!("{relative}：文件不存在"));
            continue;
        };
        let Some(actual) = text.lines().nth(line - 1) else {
            stale.push(format!(
                "{relative}：只有 {} 行，文档指到了第 {line} 行",
                text.lines().count()
            ));
            continue;
        };
        if !actual.contains(expected) {
            stale.push(format!(
                "{relative}:{line}：期望包含 `{expected}`，实际是 `{}`",
                actual.trim()
            ));
        }
    }

    assert!(
        stale.is_empty(),
        "文档里的行号引用必须还指着它声称的那一行：\n{}",
        stale.join("\n")
    );
}

/// 文档断言「`qualify_sql_target` 由 6 个 SQL 驱动实现，mongodb 声明
/// `has_multi_database` 但不走 SQL 路径」——实现方必须与文档一致。
#[test]
fn the_sql_target_qualification_driver_set_matches_the_document() {
    let drivers_dir = repo_root().join("packages/drivers");

    let mut implementors: BTreeSet<String> = BTreeSet::new();
    let mut stack = vec![drivers_dir.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            // 驱动 crate 的源码目录：`<driver>/src/...`。测试支撑代码不算实现方。
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !text.contains("fn qualify_sql_target") {
                continue;
            }
            let components: Vec<String> = path
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect();
            let Some(driver_index) = components.iter().position(|part| part == "drivers") else {
                continue;
            };
            let Some(driver) = components.get(driver_index + 1) else {
                continue;
            };
            // `packages/drivers/http-support/tests/...` 是测试支撑包，不是驱动。
            if driver == "http-support" {
                continue;
            }
            implementors.insert(driver.clone());
        }
    }

    let expected: BTreeSet<String> = [
        "clickhouse",
        "duckdb",
        "mysql",
        "postgres",
        "sqlite",
        "sqlserver",
    ]
    .into_iter()
    .map(String::from)
    .collect();

    assert_eq!(
        implementors, expected,
        "文档断言 `qualify_sql_target` 由 6 个 SQL 驱动实现，实现方集合必须与文档一致"
    );

    // mongodb：声明 `has_multi_database`，但不走 SQL 路径。
    let mongodb = drivers_dir.join("mongodb/src/mongodb.rs");
    let text = std::fs::read_to_string(&mongodb).expect("mongodb 驱动源码可读");
    assert!(
        text.contains("fn has_multi_database(&self) -> bool"),
        "mongodb 应当声明 `has_multi_database`"
    );
    assert!(
        !text.contains("fn qualify_sql_target"),
        "mongodb 不走 SQL 路径，不应实现 `qualify_sql_target`"
    );
}

/// 文档所在目录必须存在（守卫本身的锚点）。
#[test]
fn the_guarded_document_is_where_the_guard_expects_it() {
    assert!(
        doc_path().exists(),
        "{DOC_RELATIVE} 应当存在；若它被改名/移走，请同步更新这条守卫"
    );
    assert!(
        !Path::new(DOC_RELATIVE).is_absolute(),
        "文档路径必须是仓库内相对路径"
    );
}
