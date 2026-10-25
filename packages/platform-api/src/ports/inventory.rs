//! 端口 trait 的**机器可读清单**：把「端口族有多大」从**手写数字**变成**派生事实**。
//!
//! ## 为什么有这份清单
//!
//! 在这份清单出现之前，`ports/mod.rs` 与 `lib.rs` 的模块头各自手写着同一个端口数量，
//! 而**没有任何东西校验它**：它会静默漂移——实测它已经错过多次（一条并行分支上留着过时的
//! 数字，漏了后来合入的窄端口；另一条分支上同样漏了隧道端口）。而
//! `docs/architecture/platform/shared-boundaries-and-ports.md` §4.2 曾把「确切条数以
//! `ports/mod.rs` 模块头的计数为准」写成文档口径，于是那个手写数字成了**唯一事实来源**，
//! 却没有任何机制保证它对。
//!
//! ## 现在的口径
//!
//! * **数字全部删掉**：`ports/mod.rs` 与 `lib.rs` 的模块头不再复述端口数量，文档也不再
//!   说「以模块头的计数为准」。规模由清单派生。
//! * [`PORT_TRAITS`] 是端口 trait 名字的**唯一人工清单**；`pub trait` 声明本身仍是事实
//!   来源：清单里的每个名字都要对应源码里一处真实的 `pub trait`，源码里多一处或少一处，
//!   测试都红。
//! * 单测还会**禁止数字再出现**：端口族规模一旦又被写回手写数字，CI 立刻红。
//!
//! ## 顺序约定
//!
//! 清单按 **trait 名字的字母序**（ASCII）排列。选它而不是「按模块路径」的理由：trait
//! 在文件之间搬家不该让清单跟着重排；字母序既稳定，又让读者能按名字直接定位。测试拿
//! **排序后的派生集合**逐项比对，所以「清单必须是有序的」这件事本身也被守住。
//!
//! ## 谁在核对
//!
//! `datazen-platform-api` 的单测（见本文件末尾的 `tests` 模块）核对三件事：
//!
//! 1. **清单 ↔ 代码**：解析 `src/ports/` 下每个源文件里真实的 `pub trait` 声明，逐项比对
//!    （多一个、少一个、顺序错都红）。
//! 2. **清单 ↔ 文档**：解析 `docs/architecture/platform/system-overview.md` §6.4 的端口表，
//!    断言表里每一行都在清单里，而清单里不在表里的每一个都在 `ports/mod.rs` 模块头的构成
//!    说明里被**逐个点名**——两边都不能漏、不能多。
//! 3. **模块头不得复活手写计数**：端口族规模一旦又被写成数字，CI 红。
//!
//! 解析细节（源码的代码形状、模块头段落、§6.4 表格）住在 [`parser`] 子模块里；本文件
//! 只留「清单 + 断言」。

/// 端口 trait 清单 —— **唯一的人工清单**。
///
/// 顺序：trait 名字的字母序（ASCII）。
///
/// 增删端口 trait 的顺序：先改源码里的 `pub trait` 声明，再把名字加进/移出这份清单。
/// 端口数量由这份清单派生，**不需要**（也不允许）在模块头或文档里手写。
pub const PORT_TRAITS: &[&str] = &[
    "ArtifactRepository",
    "BudgetCoordinator",
    "Clock",
    "ClusterNodeBudgetPort",
    "ConnectionCatalog",
    "ControlResourcePort",
    "DriverPoolBudgetPort",
    "EventSink",
    "ExecutionSettingsReader",
    "ExecutionTargetResolver",
    "IdentityResolver",
    "JobRepository",
    "KnownHostsRepository",
    "MasterKeyProvider",
    "MigrationProfileReader",
    "MonitorGrantReader",
    "NativeToolPort",
    "NetworkProvider",
    "PolicyService",
    "ProfileReader",
    "ProfileRepository",
    "SecretProvider",
    "SessionDirectory",
    "SubmissionTokenIssuer",
    "TunnelRepository",
];

#[cfg(test)]
mod parser;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::parser::{
        collect_rs_files, declared_port_traits, header_named_traits, module_header, scale_counts,
        section_6_4_table, CRATE_HEADER, MODULE_HEADER, SOURCE_FILES, SYSTEM_OVERVIEW,
    };
    use super::PORT_TRAITS;

    /// 人工清单必须与代码里真实的 `pub trait` 声明**逐项一致**（多一个、少一个、顺序错都红）。
    #[test]
    fn the_manual_inventory_lists_exactly_the_declared_port_traits() {
        let declared = declared_port_traits();
        let unique: BTreeSet<&String> = declared.iter().collect();
        assert_eq!(
            unique.len(),
            declared.len(),
            "同一个端口 trait 被声明了多次：{declared:?}",
        );
        assert_eq!(
            declared, PORT_TRAITS,
            "端口清单与代码里的 `pub trait` 声明不一致。\
             清单是唯一的人工清单，代码是事实来源：增删端口 trait 时两处都要改。",
        );
    }

    /// `src/ports/` 下**每一个** `.rs` 文件都必须被 `include_str!` 覆盖。
    ///
    /// 没有这一条，新增的端口文件若忘了登记进 `SOURCE_FILES`，它里面的 trait 会被
    /// **静默漏掉**——测试照样绿，而清单已经不完整了。
    #[test]
    fn every_port_source_file_is_covered_by_the_include_list() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("ports");
        let mut on_disk = Vec::new();
        collect_rs_files(&root, &root, &mut on_disk);
        on_disk.sort();

        let covered: BTreeSet<&str> = SOURCE_FILES.iter().map(|(path, _)| *path).collect();
        let missing: Vec<&String> = on_disk
            .iter()
            .filter(|path| !covered.contains(path.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "端口源码目录下这些文件没有进 SOURCE_FILES，它们的 `pub trait` 会被漏掉：{missing:?}",
        );
        let extra: Vec<&&str> = covered
            .iter()
            .filter(|path| !on_disk.iter().any(|disk| disk == *path))
            .collect();
        assert!(
            extra.is_empty(),
            "SOURCE_FILES 列了磁盘上不存在的文件：{extra:?}",
        );
    }

    /// 端口族规模**不得**又被写成手写数字。
    ///
    /// 这条断言守的是「数字不会回来」：模块头（`ports/mod.rs` 与其余端口文件的 `//!` 头、
    /// 以及 `lib.rs` 的头）一旦写回「N 个 trait」「N 个端口」「N 行」或「第 N 个」这类
    /// 规模计数，CI 立刻红。规模由 [`PORT_TRAITS`] 派生，不再由人手写。
    #[test]
    fn the_module_headers_do_not_carry_a_hand_written_port_count() {
        let mut violations: Vec<String> = Vec::new();
        let mut headers: Vec<(&str, String)> = SOURCE_FILES
            .iter()
            .map(|(path, source)| (*path, module_header(source)))
            .collect();
        headers.push(("lib.rs", module_header(CRATE_HEADER)));
        for (path, header) in headers {
            if header.trim().is_empty() {
                continue;
            }
            for (line, pattern) in scale_counts(&header) {
                violations.push(format!("{path}:{line} 出现规模计数「{pattern}」"));
            }
        }
        assert!(
            violations.is_empty(),
            "端口族规模必须由 `PORT_TRAITS` 派生，不许在模块头里手写：\n{}",
            violations.join("\n"),
        );
    }

    /// 跨文档的那一半：概要 §6.4 的端口表 ⊆ 清单 ⊆ 模块头逐个点名的集合。
    ///
    /// 两边都不能漏、不能多：§6.4 表里多出来的名字必须在清单里（清单才是端口全集），
    /// 清单里不在 §6.4 表里的每一个都必须在 `ports/mod.rs` 模块头的构成说明里被点名
    /// （否则它就是一个没人认领的端口）。
    #[test]
    fn the_section_6_4_table_and_the_module_header_account_for_every_port_trait() {
        let table = section_6_4_table(SYSTEM_OVERVIEW)
            .unwrap_or_else(|error| panic!("解析概要 §6.4 的端口表失败：{error}"));
        for name in &table {
            assert!(
                PORT_TRAITS.contains(&name.as_str()),
                "概要 §6.4 的端口表里有 `{name}`，但端口清单里没有它——\
                 清单才是端口全集，表里多出来的名字要么补进清单，要么从表里去掉。",
            );
        }
        let named = header_named_traits(MODULE_HEADER);
        let expected: Vec<String> = PORT_TRAITS
            .iter()
            .filter(|trait_name| !table.contains(&trait_name.to_string()))
            .map(|trait_name| (*trait_name).to_string())
            .collect();
        assert_eq!(
            named, expected,
            "不在概要 §6.4 表里的端口，必须在 ports/mod.rs 模块头的构成说明里逐个点名：\
             模块头点名的 {named:?}，清单里未被点名的 {expected:?}",
        );
    }
}
