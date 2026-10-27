//! 稳定指纹的唯一实现。
//!
//! 从 `runtime::connection::types` 迁入：Job 内核对「幂等键 / 计划 id」做稳定指纹，
//! 而连接运行时反过来又要用同一份实现（`gateway::token`、`connection::port`）。
//! 放进内核后两边共用**一个**实现，`datazen_modules::execution::runtime::connection::types::fnv1a64_hex`
//! 作为再导出继续可用。

/// FNV-1a 64 位哈希，十六进制输出。夹具与幂等指纹都走它，保证跨运行完全确定。
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
