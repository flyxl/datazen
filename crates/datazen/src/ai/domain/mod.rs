//! AI provider contracts: trait, request/response models, factory registration.
//!
//! These definitions are the stable surface plugin crates implement; they must
//! not depend on any vendor adapter or HTTP machinery living in
//! [`super::providers`].
//!
//! D6 item 4 切片三新增：budget（token 预算常量与公式）/ ctx_yaml（表组
//! 上下文词汇）/ safety（提示词脱敏规则）。

pub mod budget;
pub mod ctx_yaml;
pub mod factory;
pub mod safety;
pub mod traits;
pub mod types;

pub use budget::*;
pub use ctx_yaml::*;
pub use factory::*;
pub use safety::*;
pub use traits::*;
pub use types::*;
