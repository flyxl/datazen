//! 剥除器「模块形状」不变式的常驻回归测试。
//!
//! 这些测试查的是**剥除器本身**的性质（`mod x;` 外部声明不得被当成内联块吞掉），
//! 而不是「生产路径有没有接上 `AlwaysAllow`」——后者是同目录 `production_wiring.rs` 的职责。
//! BLOCKER D 的形状恰好落在这条分界上，所以两者必须分别有各自的常驻测试：
//! `production_wiring.rs` 证明「守卫量得到接线」，这里证明「它量的是完整的那段代码」。
//!
//! 这些断言直接复用 `production_wiring` 的实现而不是复刻一份，
//! 所以守卫的逻辑改了、这里自动跟着改——复刻会退化成两份各自腐化的实现。

use super::production_wiring::{
    collect_rs, inline_module_brace, is_test_only, matching_brace, next_inline_test_block,
    repo_root, wired_names,
};
use std::fs;
use std::path::Path;

/// 文件里全部「`#[cfg(test…)]`」属性的字节偏移。
fn cfg_test_attr_offsets(src: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = src[cursor..].find("#[cfg(") {
        let at = cursor + rel;
        cursor = at + 4;
        let close = at + src[at..].find(']').unwrap_or(src[at..].len() - 1);
        let attr = &src[at..=close];
        if !attr.contains("not(")
            && (attr.contains("test)") || attr.contains("test,") || attr.contains("doctest"))
        {
            found.push(at);
        }
    }
    found
}

/// 剥除器实际会吞掉的内联块，其属性起点依次是什么。
fn inline_block_starts(src: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut cursor = 0;
    while let Some((attr_at, brace_at)) = next_inline_test_block(src, cursor) {
        starts.push(attr_at);
        cursor = match matching_brace(src, brace_at) {
            Some(end) => end + 1,
            None => break,
        };
    }
    starts
}

/// 仓库级不变式：**任何** `#[cfg(test…)] mod x;`（外部声明，模块体在别的文件里）
/// 都不许被当成内联块吃掉。
///
/// 这条不变式就是 BLOCKER D 的形状本身，而且它覆盖全仓库——不只是本轨碰过的那几个文件。
/// 旧实现里，`mod x;` 会跟后面第一个不相关的 `{` 配对，于是从该属性一路吞到那个块的结尾，
/// 守卫对这一整段生产代码彻底失明。
#[test]
fn no_braceless_test_module_declaration_is_ever_swallowed() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    files.sort();

    let mut offenders = Vec::new();
    let mut external_decl_files = 0usize;
    let mut external_decls = 0usize;
    let mut scanned = 0usize;
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .expect("扫描到的文件都在仓库根之下")
            .to_string_lossy()
            .into_owned();
        if is_test_only(Path::new(&rel), &root) {
            continue;
        }
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        scanned += 1;
        let starts = inline_block_starts(&src);
        let mut here = 0usize;
        for attr_at in cfg_test_attr_offsets(&src) {
            if inline_module_brace(&src, attr_at + src[attr_at..].find(']').unwrap_or(0) + 1)
                .is_some()
            {
                continue; // 真内联块，本来就该被剥
            }
            external_decls += 1;
            here += 1;
            let line = src[..attr_at].lines().count();
            if starts.contains(&attr_at) {
                offenders.push(format!("{rel}:{line}"));
            }
        }
        if here > 0 {
            external_decl_files += 1;
        }
    }

    assert!(
        scanned > 100,
        "扫描到的文件太少，这条不变式等于没跑：{scanned}"
    );
    assert!(
        offenders.is_empty(),
        "这些 `mod x;` 外部声明被当成了内联块，{} 行生产代码对守卫不可见：{offenders:?}",
        offenders.len()
    );
    // 顺带钉住这条不变式**确实有非空输入**可查，否则它就是最危险的那种绿。
    assert!(
        external_decl_files >= 10 && external_decls >= 20,
        "外部测试模块声明太少（{external_decl_files} 个文件 / {external_decls} 处），\
         这条不变式的覆盖面可能已经失效"
    );
    eprintln!(
        "外部测试模块声明：{external_decls} 处 / {external_decl_files} 个文件，\
         扫描生产文件 {scanned} 个，全部未被误吞"
    );
}

/// `mod x;`（模块体在别的文件里）后面**没有花括号**，不许拿后面不相关的 `{` 来配对。
///
/// 这条形状在本仓库极其常见：`packages/runtime/src/gateway/mod.rs` 的
/// `#[cfg(test)]` 模块声明**全部**是外部声明。旧实现会把 `#[cfg(test)]` 与后面某个
/// `impl Foo {` 配成一对，把两者之间的生产代码整段吞掉，守卫于是对那个窗口失明。
#[test]
fn a_braceless_test_module_declaration_must_not_swallow_the_next_block() {
    let src = r#"
#[cfg(test)]
pub(crate) mod testing_support;

pub fn planted_for_guard_bypass() -> Arc<dyn Authorizer> {
    Arc::new(AlwaysAllow)
}

impl SomeProductionType {
    fn body(&self) {}
}
"#;
    let hits = wired_names(src);
    assert_eq!(
        hits.len(),
        1,
        "外部声明之后的生产代码被当成测试块吞掉了，实际：{hits:?}"
    );
    assert_eq!(
        hits[0].0, 6,
        "报出来的行号必须是探针所在的真实行（`Arc::new(AlwaysAllow)` 在第 6 行）：{hits:?}"
    );

    // 反向：真正的 `mod x { … }` 内联块**仍要**被剥掉，否则这条修复会矫枉过正。
    let inline = r#"
#[cfg(test)]
mod inner_tests {
    pub fn helper() -> Arc<AlwaysDeny> {
        Arc::new(AlwaysDeny)
    }
}
pub fn production_body() -> Arc<AlwaysAllow> {
    Arc::new(AlwaysAllow)
}
"#;
    let hits = wired_names(inline);
    // 内联块里的 `AlwaysDeny` 必须一条都不剩；`production_body` 的两行都留下。
    let seen: Vec<usize> = hits.iter().map(|(line, _)| *line).collect();
    assert_eq!(
        seen,
        vec![8, 9],
        "内联测试块没被剥掉（或剥过头了），实际：{hits:?}"
    );
}
