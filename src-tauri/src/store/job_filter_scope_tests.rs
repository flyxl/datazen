//! `JobFilter` 的**组织维度必填**在全仓的落点守卫。
//!
//! `JobFilter` 不再实现 `Default`：省略组织维度就没有合法的列举调用，
//! 因此每一个 `JobFilter { ... }` 字面量都必须**显式**写出 `organization_id`。
//!
//! 这条不变量靠人读 diff 是守不住的：`#[cfg(feature = "webdriver")]` 里的调用点
//! 在默认门禁（`cargo check -p datazen --all-targets`）下**根本不编译**，
//! 于是残留的 `..Default::default()` 会一路活到 `pnpm tauri:build:webdriver`
//! （也就是 E2E 构建）才炸成 E0277。因此这里用源码扫描把它钉住：
//! 谁写下不带组织维度的过滤器，这条测试就变红。

use std::path::{Path, PathBuf};

/// 仓库根目录：`src-tauri` 的上一级。
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri sits inside the repository root")
        .to_path_buf()
}

/// 扫描范围（相对仓库根）：宿主与所有共享 crate 的 Rust 源码。
const SCANNED_ROOTS: &[&str] = &["src-tauri/src", "packages"];

/// 目录遍历时跳过的名字：构建产物与依赖目录里没有可执行的契约。
const SKIPPED_DIRECTORIES: &[&str] = &["target", "node_modules", ".git"];

/// 本文件自身：它在自己的说明文字里提到过滤器字面量，扫自己会自证其说。
const SELF: &str = "src-tauri/src/store/job_filter_scope_tests.rs";

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = SCANNED_ROOTS
        .iter()
        .map(|relative| root.join(relative))
        .collect();
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if SKIPPED_DIRECTORIES.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// 去掉注释（行注释与块注释）。
///
/// 文档注释里经常出现过滤器字面量的**说明**，它们不是字面量；
/// 不去掉就会把说明当成违规。行数必须保持不变，因此块注释里的换行要补回去。
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    let mut in_string = false;
    let mut in_character = false;
    let mut in_block_comment = false;
    let mut in_line_comment = false;
    while let Some(character) = characters.next() {
        if in_line_comment {
            if character == '\n' {
                in_line_comment = false;
                out.push(character);
            }
            continue;
        }
        if in_block_comment {
            if character == '\n' {
                out.push(character);
            } else if character == '*' && characters.peek() == Some(&'/') {
                characters.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_string {
            out.push(character);
            if character == '\\' {
                if let Some(escaped) = characters.next() {
                    out.push(escaped);
                }
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if in_character {
            out.push(character);
            if character == '\\' {
                if let Some(escaped) = characters.next() {
                    out.push(escaped);
                }
            } else if character == '\'' {
                in_character = false;
            }
            continue;
        }
        match character {
            '/' if characters.peek() == Some(&'/') => {
                characters.next();
                in_line_comment = true;
            }
            '/' if characters.peek() == Some(&'*') => {
                characters.next();
                in_block_comment = true;
            }
            '"' => {
                in_string = true;
                out.push(character);
            }
            '\'' => {
                in_character = true;
                out.push(character);
            }
            _ => out.push(character),
        }
    }
    out
}

/// 提取源码里的 `JobFilter { ... }` **字面量**（按花括号配对）。
///
/// `pub struct JobFilter {` 是**声明**不是字面量，按 `struct` 前缀排除。
/// 返回 `(行号, 字面量原文)`。
fn job_filter_literals(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = source[cursor..].find("JobFilter") {
        let start = cursor + offset;
        cursor = start + "JobFilter".len();
        if source[..start].trim_end().ends_with("struct") {
            continue;
        }
        let rest = &source[cursor..];
        let trimmed = rest.trim_start();
        if !trimmed.starts_with('{') {
            continue;
        }
        let open = cursor + (rest.len() - trimmed.len());
        let mut depth = 0usize;
        let mut end = source.len();
        for (index, character) in source[open..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        let line = source[..start].lines().count();
        out.push((line, source[start..end].to_string()));
    }
    out
}

#[test]
fn every_job_filter_literal_declares_the_organization_dimension() {
    let root = repository_root();
    let mut literals = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    for file in rust_sources(&root) {
        let relative = file.strip_prefix(&root).unwrap_or(&file);
        if relative == Path::new(SELF) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        let stripped = strip_comments(&source);
        for (line, literal) in job_filter_literals(&stripped) {
            literals += 1;
            if !literal.contains("organization_id") {
                offenders.push(format!(
                    "{}:{line}  {}",
                    relative.display(),
                    literal.replace('\n', " ")
                ));
            }
        }
    }
    assert!(
        literals >= 5,
        "扫描必须真的找到 JobFilter 字面量，否则这条守卫是空的（找到 {literals} 个）"
    );
    assert!(
        offenders.is_empty(),
        "每一个 `JobFilter` 字面量都必须显式写出 `organization_id`（组织维度必填，\
         省略它就没有合法的列举调用）；下面的字面量省略了它，且默认门禁不编译它们所在的 \
         feature 分支，因此只能靠这条守卫拦住：\n{}",
        offenders.join("\n")
    );
}
