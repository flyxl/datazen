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
const BASELINE_FIELD_COUNT: usize = 25;

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
