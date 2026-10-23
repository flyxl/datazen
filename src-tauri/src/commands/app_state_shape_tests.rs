//! `AppState` 的**形状**守卫：依赖方向只允许一个方向。
//!
//! 本轨的目标是「应用层持有注入的存储，宿主逐步瘦下来」。因此 `AppState`
//! 有一条硬约束：**不得**为了给应用层开路而新增 `pub` 字段，也**不得**反过来
//! 持有 `ApplicationServices`（那是把方向接回去）。
//!
//! 这条守卫是机器可校验的：字段数是**上限**而不是等式——瘦下来是允许的，
//! 变胖不是。

/// 基线字段数（实测于 `f8eed2bb7`）。
///
/// 这是**上限**：本轨的目标就是让 `AppState` 变瘦，所以「等于基线」不该被
/// 允许、「超过基线」必须变红。
const BASELINE_FIELD_COUNT: usize = 24;

/// `AppState` 的定义体（不含 `#[cfg(test)]` 之后的任何东西）。
fn app_state_definition() -> String {
    let source = include_str!("mod.rs");
    let body = source
        .split("pub struct AppState {")
        .nth(1)
        .expect("AppState 的定义必须可解析");
    body.split("\n}")
        .next()
        .expect("AppState 的定义必须有闭合花括号")
        .to_string()
}

/// 定义体里的 `pub` 字段名（按源码顺序）。
fn public_field_names() -> Vec<String> {
    app_state_definition()
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("pub ") {
                return None;
            }
            let name = trimmed
                .trim_start_matches("pub ")
                .split(':')
                .next()?
                .trim()
                .to_string();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
        .collect()
}

#[test]
fn app_state_never_grows_past_its_baseline_shape() {
    let fields = public_field_names();
    assert!(
        fields.len() <= BASELINE_FIELD_COUNT,
        "AppState 的 pub 字段数从 {BASELINE_FIELD_COUNT} 涨到了 {}：\
         方向要反过来——应用层持有注入的存储，宿主只做瘦身。\
         不许为了给应用层开路而新增字段：{fields:?}",
        fields.len()
    );
}

#[test]
fn app_state_does_not_hold_the_application_services() {
    // 应用层持有存储、宿主调用应用层。宿主**反向**持有 `ApplicationServices`
    // 会把这条依赖边接回去：本轨的验收方式是「应用层能自己跑用例」，而不是
    // 「宿主把应用层揣在兜里再发下去」。
    let definition = app_state_definition();
    assert!(
        !definition.contains("ApplicationServices"),
        "AppState 不得持有 ApplicationServices：依赖方向只能是宿主 → 应用层"
    );
}

#[test]
fn the_field_names_are_unique_so_a_duplicate_cannot_hide_behind_the_count() {
    // 字段数是**上限**；如果同一个名字出现两次，编译器会拒绝，所以这条只是把
    // 「上限」这条断言的前提（每个字段一行、一个名字）显式钉住。
    let fields = public_field_names();
    let mut unique = fields.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        fields.len(),
        unique.len(),
        "AppState 的字段名重复了：{fields:?}"
    );
}

/// `state.tunnels` 必须为 0：隧道定义仓储归应用层持有（§5.1.2 第三步）。
///
/// 这条不是"换个写法也算数"的风格约定——`AppState` 上那份 `Arc<dyn TunnelRepository>`
/// 删掉之后，任何 `state.tunnels` 都会编译失败；本守卫在**编译器之前**就把这类回摆
/// 钉红，防止有人为了让命令少一行 `require_application_services` 又把它挂回去。
///
/// 扫描整棵 `src-tauri/src`（含测试），唯一豁免是本文件自己——它必须引用这个字符串
/// 才能断言它不存在。
#[test]
fn the_tunnel_repository_is_no_longer_reachable_from_the_host_state() {
    const NEEDLE: &str = "state.tunnels";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).expect("AppState 守卫要能读到源码目录");
        for entry in entries {
            let path = entry.expect("目录项可读").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("app_state_shape_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("源码文件是 UTF-8");
            for (index, line) in text.lines().enumerate() {
                if line.contains(NEEDLE) {
                    hits.push(format!("{}:{}", path.display(), index + 1));
                }
            }
        }
    }
    hits.sort();
    assert!(
        hits.is_empty(),
        "隧道仓储归 `ApplicationServices::tunnels` 持有（§5.1.2 第三步），宿主态不得再暴露它：{hits:?}"
    );
}

/// 定义体里也不许出现 `tunnels` 字段（上面的扫描只看 `state.tunnels` 的调用面）。
#[test]
fn app_state_definition_has_no_tunnel_repository_field() {
    let definition = app_state_definition();
    assert!(
        !definition.contains("pub tunnels"),
        "AppState 不得持有隧道仓储：它由 ApplicationServices 持有"
    );
}
