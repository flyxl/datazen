//! 连接用例面的**再导出**：`ConnectionUseCases` 的定义已搬到
//! [`datazen_platform_api::use_cases::connection`]（P0 断环）。
//!
//! ## 为什么不在本 crate
//!
//! 依赖图里 `APP → PA, DAPI`（**没有** `APP → RT`），而实现本 trait 的正是 `runtime`。
//! 签名里出现的 `ApiError`、请求 DTO、契约视图因此都必须住在两者之外、更内层的
//! platform-api，否则 `runtime` 要么反向依赖 application，要么把每个类型重定义一遍。
//!
//! ## 签名上的三条约束仍然有效（守卫已随 trait 一并迁移）
//!
//! * 身份只从 `&RequestContext` 来（INV-01，第一个参数恒定）；
//! * 失败一律是 [`ApiError`]，端口层 `PortError` 由组装层映射；
//! * 对象安全：组装层注入 `Arc<dyn ConnectionUseCases>`，async_trait 产物必须可 dyn 化。
//!
//! 本模块只做再导出，路径 `crate::sessions::ConnectionUseCases` 逐字不变；同类型证明见下。

pub use datazen_platform_api::use_cases::connection::ConnectionUseCases;

#[cfg(test)]
mod tests {
    use super::*;

    /// 编译期证明：本 crate 的 `ConnectionUseCases` **就是** platform-api 的那一个 trait，
    /// 而不是本地另有一份同名副本（两份定义会让 `Arc<dyn …>` 互不兼容，且在
    /// `ApplicationServices::new` 处才暴露）。
    #[test]
    fn the_use_case_trait_is_the_platform_api_type_not_a_local_copy() {
        fn takes_platform(_: &dyn datazen_platform_api::use_cases::connection::ConnectionUseCases) {
        }
        fn takes_local(value: &dyn ConnectionUseCases) {
            takes_platform(value);
        }
        let _ = takes_local;
        // 注入点写的是 `Arc<dyn ConnectionUseCases>`：两侧同类型时这一行才编译得过。
        fn takes_arc(_: std::sync::Arc<dyn ConnectionUseCases>) {}
        let _ = takes_arc;
    }
}
