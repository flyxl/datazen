//! 用例端口（use-case port）：宿主与用例实现之间的**签名契约**。
//!
//! 与 [`crate::ports`] 的区别是方向——`ports` 是用例**依赖**的基础设施端口（仓储、预算、
//! 时钟……，由宿主实现注入），而本模块是用例**暴露**给宿主的能力面（由 `runtime` 等
//! 实现注入）。两者都在本包，因为两端的签名都必须能被彼此看见，而实现方之间互不依赖。
//!
//! 这些 trait 不进 [`crate::ports::inventory::PORT_TRAITS`]：那份清单统计的是「基础设施
//! 端口族」的规模，用例面另有形状守卫（见各子模块的 `#[cfg(test)]`）。

pub mod connection;

pub use connection::ConnectionUseCases;
