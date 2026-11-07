//! 查询收藏（原 `src-tauri/src/store/models.rs::FavoriteQuery` 的共享归位；
//! history 模块文档开头点名的词汇来源之一）。
//!
//! ## 形状纪律（不变量）
//!
//! serde camelCase 键名是前端契约（`id` / `connectionId` / `createdAt` /
//! `updatedAt` / `keyword` / `database` / `folder`），模块单测钉住——
//! 迁移不得改变序列化形态。
//!
//! ## 存储形态分叉（登记）
//!
//! 桌面形态的收藏是**用户目录里的一个 `.sql` 文件**（§2.6 改版：`id` =
//! 文件名词干，ULID 由应用铸造、永不用标题派生，改名不改文件；面板经
//! `get_favorites_root` 拿目录路径对接同步服务）。web 形态没有本机文件
//! 系统——服务端把同一条词汇存进管理库（v10 `query_favorites` 表，
//! per (organization, principal) 隔离，named_views v9 同款口径）。词汇
//! 一条、存储两形态；`get_favorites_root` / `refresh_favorites` 两个
//! 「文件系统语义」命令不进 HTTP 面（桌面专属能力，WEB_CAPABILITIES
//! 声明缺位）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A saved SQL favorite. Desktop: one `.sql` file under the favorites root;
/// server: one row of the v10 `query_favorites` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteQuery {
    pub id: String,
    /// Owning persisted connection id (config).
    pub connection_id: String,
    pub title: String,
    pub sql: String,
    pub created_at: DateTime<Utc>,
    /// `None` for favorites migrated from SQLite, which had no such column.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// Optional completion keyword (desktop front-matter `-- keyword:`).
    /// Read and round-tripped today; binding it to completion is a later track.
    #[serde(default)]
    pub keyword: Option<String>,
    /// Logical database the statement targets, when the record has one.
    #[serde(default)]
    pub database: Option<String>,
    /// Directory of this favorite relative to the favorites root, `/`-separated.
    /// `None` = the root itself. (Server rows keep the field for wire parity;
    /// the server has no filesystem root.)
    #[serde(default)]
    pub folder: Option<String>,
}

/// 创建请求的词面（HTTP `POST /api/v1/query-favorites` 体的共享形状；
/// 桌面 `NewFavorite` 是其无 folder 变体的本机形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteDraft {
    /// Owning persisted connection id (config).
    pub connection_id: String,
    pub title: String,
    pub sql: String,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
}

impl FavoriteDraft {
    /// 纯字段校验：连接 id 与标题非空（SQL 允许空串——草稿语义，桌面
    /// 文件同理允许空文件）。
    pub fn validate(&self) -> Result<(), String> {
        if self.connection_id.trim().is_empty() {
            return Err("connectionId is required.".into());
        }
        if self.title.trim().is_empty() {
            return Err("title is required.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// IPC 形状钉住：camelCase 键名是前端契约，迁移不得改变序列化形态。
    #[test]
    fn favorite_query_keeps_camel_case_wire_shape() {
        let favorite = FavoriteQuery {
            id: "01HZX".into(),
            connection_id: "c1".into(),
            title: "Users by email".into(),
            sql: "SELECT 1".into(),
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            updated_at: None,
            keyword: Some("users".into()),
            database: Some("app".into()),
            folder: None,
        };
        let json = serde_json::to_value(&favorite).unwrap();
        assert_eq!(json["connectionId"], "c1");
        assert_eq!(json["createdAt"], "1970-01-01T00:00:00Z");
        assert_eq!(json["keyword"], "users");
        let round: FavoriteQuery = serde_json::from_value(json).unwrap();
        assert_eq!(round, favorite);
    }

    #[test]
    fn draft_rejects_blank_connection_or_title() {
        let ok = FavoriteDraft {
            connection_id: "c1".into(),
            title: "t".into(),
            sql: "SELECT 1".into(),
            database: None,
            keyword: None,
            folder: None,
        };
        assert!(ok.validate().is_ok());
        let blank = FavoriteDraft {
            connection_id: "  ".into(),
            ..ok.clone()
        };
        assert!(blank.validate().is_err());
        let no_title = FavoriteDraft {
            title: String::new(),
            ..ok
        };
        assert!(no_title.validate().is_err());
    }
}
