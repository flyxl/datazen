//! L3：会话参数提取（web-server-host §3.2 的 L3 行）。
//!
//! 职责（§3.2 原文）：「仅提取句柄与 epoch，**不查询存在性**」。本层不
//! 访问会话目录、不回答「句柄是否还活着」——存活与 `runtimeEpoch` 校验
//! 在 L6 可见性判断**之后**，由持有运行时目录的 handler 执行（§3.2 L6 行，
//! `SessionLost` / `RuntimeEpochMismatch` 只对已授权 owner 产生）。
//!
//! 失败行为（§3.2）：**无**——格式错误交由 L5 处置；L5（体量与速率）是
//! 推迟项，本刀把形状非法的头登记进 [`SessionParams::malformed`]，消费方
//! （handler）据此拒绝。提取是纯头操作，不消费请求体。
//!
//! wire 名是**实现注册名**：web-server-host §3 只定义了「句柄 + epoch 两个
//! 参数」的职责，未冻结 header 字面名；冻结随 WR-D4 的 handler 契约切片
//! 一起落（与 CSRF 头的注册方式同款，见 `crate::csrf::CSRF_HEADER`）。

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

use crate::http::middleware::under_api;

/// 运行时会话句柄的请求头（实现注册名，见模块头）。
pub const SESSION_HANDLE_HEADER: &str = "x-dz-session-handle";

/// `runtimeEpoch` 的请求头（实现注册名，见模块头）。
pub const RUNTIME_EPOCH_HEADER: &str = "x-dz-runtime-epoch";

/// 从请求头提取的会话参数（§3.2：句柄 + epoch）。
///
/// 没有携带任何头也**合法**：并非所有路由都寻址运行时会话（集合类、
/// 配置类路由不带句柄）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionParams {
    /// 运行时会话句柄（运行时句柄引用；**不查询存在性**，§3.2）。
    pub handle: Option<String>,
    /// 运行时纪元（资源丢失/替换后递增，INV-07/INV-08）。
    pub runtime_epoch: Option<u64>,
    /// 形状非法的头名登记（L3 不拒绝——§3.2 把格式错误交给 L5；L5 落地前
    /// 由消费方据此拒绝）。
    pub malformed: Vec<&'static str>,
}

/// 纯提取函数：`HeaderMap → SessionParams`。中间件本体是它的薄包装
/// （纯逻辑放这里，钉子测试直接吃 `HeaderMap`，不需要起 Router）。
pub fn extract_params(headers: &HeaderMap) -> SessionParams {
    let mut params = SessionParams {
        handle: headers
            .get(SESSION_HANDLE_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        runtime_epoch: None,
        malformed: Vec::new(),
    };
    match headers
        .get(RUNTIME_EPOCH_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::parse::<u64>)
    {
        Some(Ok(epoch)) => params.runtime_epoch = Some(epoch),
        Some(Err(_)) => params.malformed.push(RUNTIME_EPOCH_HEADER),
        None => {}
    }
    params
}

/// L3 中间件本体。挂载点：`build_router_with_auth`，L2 之内、L4 之外。
pub(crate) async fn session_params(mut req: Request, next: Next) -> Response {
    if !under_api(req.uri().path()) {
        return next.run(req).await;
    }

    // 先读头（不可变借用）再插扩展（可变借用），两段借用在插扩展前结束。
    let params = extract_params(req.headers());
    req.extensions_mut().insert(params);
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(handle: Option<&str>, epoch: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(handle) = handle {
            headers.insert(SESSION_HANDLE_HEADER, handle.parse().expect("handle"));
        }
        if let Some(epoch) = epoch {
            headers.insert(RUNTIME_EPOCH_HEADER, epoch.parse().expect("epoch"));
        }
        headers
    }

    /// §3.2 的「仅提取」：合法头原样登记，缺头合法。
    #[test]
    fn wellformed_headers_are_extracted_and_absence_is_legal() {
        let params = extract_params(&headers(Some("dbs-1"), Some("7")));
        assert_eq!(params.handle.as_deref(), Some("dbs-1"));
        assert_eq!(params.runtime_epoch, Some(7));
        assert!(params.malformed.is_empty());

        let params = extract_params(&HeaderMap::new());
        assert_eq!(params, SessionParams::default(), "无头也合法");
    }

    /// 形状非法的 epoch 不被本层拒绝，登记后交消费方（§3.2）。
    #[test]
    fn a_malformed_epoch_is_registered_not_rejected() {
        let params = extract_params(&headers(Some("dbs-1"), Some("not-a-number")));
        assert_eq!(params.runtime_epoch, None);
        assert_eq!(params.malformed, vec![RUNTIME_EPOCH_HEADER]);
        assert_eq!(
            params.handle.as_deref(),
            Some("dbs-1"),
            "坏 epoch 不影响句柄"
        );
    }
}
