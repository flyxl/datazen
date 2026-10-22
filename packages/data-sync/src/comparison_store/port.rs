//! 比较存储的端口：**只**决定「存储根从哪来」。
//!
//! ## 为什么落盘实现留在本包
//!
//! `disk.rs` / `recovery.rs` 描述的是**本地文件系统**语义：私有目录、0600 文件、
//! 帧式行文件、SQLite 独占租约、崩溃后的陈旧目录回收。这些都与宿主形态无关——
//! 任何有本地文件系统的宿主（桌面、Web 后端、CLI）都能直接用同一份实现。把它们
//! 搬进宿主目录等于把「解耦」做成「搬家」：宿主凭空多出几百行，第二个形态还得
//! 再抄一遍。
//!
//! 因此本端口**只收根与作用域**：
//!
//! * **根**：比较结果写到哪个目录，由宿主在组装期注入；
//! * **作用域**：越组织访问按「**不可用**」处理（CM-05），绝不返回「无权限」。
//!
//! 其余语义（写入、分页读取、索引校验、崩溃回收）全部留在包内。

use std::path::{Path, PathBuf};

use datazen_platform_api::id::OrganizationId;
use datazen_platform_api::ports::scope::OrganizationScope;

/// 比较存储根的端口。
///
/// 端口只回答一个问题：**某个组织可用的比较存储根是哪个目录**。
/// 越组织访问一律按「**不可用**」处理——返回 `None`，而不是「无权限」错误，
/// 以免泄漏「存在但无权」（CM-05）。
pub trait ComparisonStorageRoot: Send + Sync {
    /// 取该作用域下的比较存储根。
    ///
    /// 越组织访问返回 `None`；调用方据此按「**不可用**」处理（错误文案是
    /// `"Data Sync comparison storage is unavailable"`）。比较侧没有「不存在」
    /// 这个对照态——它本来就不是一个可按 id 寻址的资源。
    fn root_for(&self, scope: &OrganizationScope) -> Option<PathBuf>;
}

/// 本地文件系统实现：**一个根目录只属于一个组织**。
///
/// 这是默认实现，也是形态中立的那一个：宿主把「比较结果落在哪」决定下来，
/// 剩下的落盘语义由包内承担。
#[derive(Debug, Clone)]
pub struct LocalComparisonStorageRoot {
    root: PathBuf,
    organization_id: OrganizationId,
}

impl LocalComparisonStorageRoot {
    /// 组装一个「根目录 ↔ 单一组织」的绑定。
    pub fn new(root: impl Into<PathBuf>, organization_id: OrganizationId) -> Self {
        Self {
            root: root.into(),
            organization_id,
        }
    }

    /// 根目录本身（不含作用域判定）。
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// 该根归属的组织。
    pub fn organization_id(&self) -> &OrganizationId {
        &self.organization_id
    }
}

impl ComparisonStorageRoot for LocalComparisonStorageRoot {
    fn root_for(&self, scope: &OrganizationScope) -> Option<PathBuf> {
        if scope.organization_id() == &self.organization_id {
            Some(self.root.clone())
        } else {
            None
        }
    }
}

/// 桌面等本地形态的默认比较存储根。
///
/// 路径约定留在包里（它是包内落盘语义的一部分），但**用哪个根**由宿主注入：
/// 宿主可以指到 `{appData}` 下的目录，也可以沿用这里的临时目录。
pub fn default_comparison_storage_root() -> PathBuf {
    std::env::temp_dir().join("datazen-sync-comparisons")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(organization: &str) -> OrganizationScope {
        OrganizationScope::new(OrganizationId::new(organization))
    }

    #[test]
    fn the_bound_root_is_returned_for_its_own_organization() {
        let root = LocalComparisonStorageRoot::new(
            PathBuf::from("/tmp/datazen-sync-comparisons"),
            OrganizationId::new("org-a"),
        );
        assert_eq!(
            root.root_for(&scope("org-a")),
            Some(PathBuf::from("/tmp/datazen-sync-comparisons"))
        );
    }

    /// CM-05：越组织访问按「不可用」处理，不返回「无权限」。
    #[test]
    fn another_organization_gets_no_root_instead_of_a_permission_error() {
        let root = LocalComparisonStorageRoot::new(
            PathBuf::from("/tmp/datazen-sync-comparisons"),
            OrganizationId::new("org-a"),
        );
        assert_eq!(root.root_for(&scope("org-b")), None);
    }
}
