//! `datazen-workspace-apps` — Workspace App 形态中立内核（Wave-2 track B1）。
//!
//! 本 crate 从宿主 Rust 后端下沉了**与形态无关**的部分：manifest 模式与
//! 校验规则、包安装（ZIP / 目录）、按 wapp 隔离的键值存储。zip 条目路径守卫
//! （zip-slip 防护）下沉到 `datazen-platform-infra::path_safety`，本 crate 只链接，
//! 不再自带第二份实现。
//!
//! 形态中立的关键约定：**所有入口都把存储根作为参数传入**，内核自身不读取
//! 任何 ambient 路径、不持有连接、不发网络请求。因此桌面端（本地文件系统
//! 适配器）与将来的服务端（HTTP 适配器）可以共用同一份实现，只要把同一个
//! `wapps_dir` 换掉即可，无需改动内核签名。
//!
//! 留在宿主的是 **形态相关**的部分：`protocol`（`datazen://` 协议桥 + IPC
//! 关联）与 `WappManager`（启动期扫描、`.enabled` 标记）。宿主侧的
//! `wapps/mod.rs` 现在只是一层 re-export 门面。
//!
//! 硬边界：内核**零 HTTP**（不依赖任何 HTTP 客户端库，也不依赖任何服务端
//! 框架）、**零桌面壳依赖**。二者均由
//! `scripts/check-platform-crate-boundaries.mjs` 的 F-09 / F-10 强制：F-09
//! 禁止桌面壳依赖，F-10 禁止服务端框架；HTTP *客户端* 是 `ai-providers` 的
//! 职责，本 crate 两者都不引入。

pub mod install;
pub mod manifest;
pub mod storage;

#[cfg(test)]
mod fixture_tests;

#[cfg(test)]
mod manifest_tests;

/// Wapp 包必须声明 `apiVersion == WAPP_API_VERSION` 才能加载。
///
/// 这是包协议的常量，不随宿主形态变化，故与校验规则同处内核。
pub const WAPP_API_VERSION: u32 = 2;

#[allow(unused_imports)]
pub use manifest::{
    allowed_wapp_file_ext, is_valid_wapp_id, parse_manifest, validate_manifest, validate_wapp_dir,
    Contributions, PageContribution, Permission, ThemeContribution, WappManifest, MAX_WAPP_FILES,
    MAX_WAPP_UNCOMPRESSED,
};
#[allow(unused_imports)]
pub use storage::{storage_get, storage_remove, storage_set};

pub mod manager;
pub mod protocol;
