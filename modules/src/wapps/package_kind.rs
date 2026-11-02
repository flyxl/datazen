//! Wapp 包种类的 IPC 词汇与展示标签。
//!
//! B2 wapps 域刀下沉物（原桌面 `commands/wapps/mod.rs` 的
//! `WappPackageKind` 与 `package_label`）：`packageKind` 参数接受哪些别名、
//! 如何归一，是线契约——「选哪种原生对话框」是宿主形态，但词汇解析与
//! 标签截取规则形态中立，Web 宿主的同名参数爬同一份归一表。解析错误的
//! 文本是 IPC 契约（宿主把它映射成 Validation 错误），下沉时逐字保留。

use std::path::Path;

/// How a wapp package was picked: a ZIP archive or a plain directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WappPackageKind {
    Zip,
    Folder,
}

impl WappPackageKind {
    /// Parse the IPC `packageKind` vocabulary (`zip` / `file` pick an archive,
    /// `folder` / `dir` / `directory` pick a directory).
    ///
    /// The error text is the wire contract; the host surfaces it verbatim as
    /// a validation error.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "zip" | "file" => Ok(Self::Zip),
            "folder" | "dir" | "directory" => Ok(Self::Folder),
            other => Err(format!("Invalid wapp package kind: {other}")),
        }
    }
}

/// The user-facing label of a picked package: its file name, or `package`
/// when the path ends in a separator and has none.
pub fn package_label(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "package".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 迁自桌面 `commands/wapps/tests.rs`（B2 域刀：纯词汇解析归位 modules）。
    #[test]
    fn wapp_package_kind_parses_zip_and_folder_aliases() {
        assert_eq!(WappPackageKind::parse("zip").unwrap(), WappPackageKind::Zip);
        assert_eq!(
            WappPackageKind::parse("folder").unwrap(),
            WappPackageKind::Folder
        );
        assert!(WappPackageKind::parse("bogus").is_err());
    }
}
