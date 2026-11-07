//! 语言包（external language pack）形态中立内核。
//!
//! 设置页「安装外部语言包」的共享半边（设计档：
//! `docs/development/language-pack-install-design.md`）：一个语言包 = 一个
//! ZIP，内含 `manifest.json` + 至多 13 个域字典文件（`core.json`、
//! `connection.json`……与 `packages/frontend/src/locales/domains.ts` 的
//! 域拆分一一对应）。内核负责**校验规则单家**（`manifest.rs`）、ZIP 两遍
//! 安检解包与原子安装（`install.rs`）、已装包扫描/列表/读取/卸载
//! （`manager.rs`）以及 IPC 投影（`summary.rs`）。
//!
//! 形态中立约定与 `wapps` 域相同：所有入口把存储根（`packs_dir`）作为
//! 参数传入，内核不读取 ambient 路径、不持有连接、不发网络请求。桌面宿主
//! 注入 `{appData}/language-packs`；将来的 Web 宿主可以换一个适配器驱动
//! 同一份实现。
//!
//! 安全边界（fail-closed）：
//! * ZIP 条目只允许根层白名单文件名（manifest + 13 域），目录条目、嵌套
//!   路径、隐藏文件、符号链接、路径穿越一律拒绝；zip-slip 防护沿用
//!   `datazen-infra::path_safety`，压缩比超限（ZIP 炸弹）拒绝。
//! * 消息键拒绝 `__proto__` / `constructor` / `prototype` 与一切 `__`
//!   前缀键：前端的翻译注册表用 JS `Object.assign` 合并字典，这类键会
//!   触发原型污染，Rust 校验侧是唯一可靠闸门。
//! * 无内容数量上限（用户裁决 Q5）：键数、单条译文长度、包体积均不设
//!   限制；只保留与数量无关的格式与安全校验。

pub mod install;
pub mod manager;
pub mod manifest;
pub mod summary;

pub use install::{inspect_language_pack_file, install_from_zip, LanguagePackInspection};
pub use manager::LanguagePackManager;
pub use manifest::{
    validate_message_key, validate_message_value, LanguagePackManifest, BUILTIN_LOCALE_CODES,
    LANGUAGE_PACK_DOMAINS, MANIFEST_FILE, PACK_VERSION,
};
pub use summary::{LanguagePackDomainCount, LanguagePackMessages, LanguagePackSummary};
