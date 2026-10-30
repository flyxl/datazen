//! Host-facing use-case surface for the connection feature (host-boundary F4a).
//!
//! 固定 `api` 模块的写法（边界方案 §3）：入参是作用域 + 已构造好的服务，
//! 不依赖 Tauri / Axum / `AppState`；桌面宿主在这里只保留传输胶水、会话
//! 编排与宿主专有钩子（如「保存后退休旧会话」），Web 宿主以后复用同一批
//! 用例函数而不必再抄一份编排。
//!
//! 错误用 feature error（`ConnectionError`），业务错误文本已在 application
//! 层定稿；宿主各自持有一份 `ConnectionError → CommandError / HTTP` 的映射
//! 表（桌面是 `commands::connection::conn_cmd`），映射表不允许进本模块。
//!
//! 本刀只收 CRUD 与「测试连接的配置解析」；`list_plain` / `groups` 的导入
//! 导出路径（`commands/connection_import`）与会话/隧道编排留待后续刀，
//! 仍由宿主直呼服务。
//!
//! DTO 复用 `datazen-driver-api` 的 `ConnectionConfig`，不另定义一份。

use datazen_driver_api::ConnectionConfig;

use super::application::{merge_missing_secrets, ConnectionError, ConnectionService};
use super::domain::ConnectionScope;

/// 列出组织内全部连接档案，凭据已脱敏（展示面；明文只走 [`get_connection`]）。
pub async fn list_connections(
    scope: &ConnectionScope,
    service: &ConnectionService,
) -> Result<Vec<ConnectionConfig>, ConnectionError> {
    service.list_redacted(scope).await
}

/// 单个档案，含明文凭据——仅供 connect / test 路径。
///
/// 不存在时返回 `Ok(None)`，由调用方决定要不要把它升格为错误
/// （桌面 connect 路径用 `NotFound`，测试路径静默合并）。
pub async fn get_connection(
    scope: &ConnectionScope,
    service: &ConnectionService,
    connection_id: &str,
) -> Result<Option<ConnectionConfig>, ConnectionError> {
    service.get(scope, connection_id).await
}

/// 按表单语义保存：`clear_missing_credentials` 为真时逐字写入（用户显式
/// 清空密钥），否则空密钥字段从已存副本合并回来，脱敏回显不能清空库。
pub async fn save_connection(
    scope: &ConnectionScope,
    service: &ConnectionService,
    config: ConnectionConfig,
    clear_missing_credentials: bool,
) -> Result<(), ConnectionError> {
    service
        .save_merged(scope, config, clear_missing_credentials)
        .await
}

/// 在新 id / 新名称下复制档案，不带源的连接时间戳。
pub async fn duplicate_connection(
    scope: &ConnectionScope,
    service: &ConnectionService,
    source_id: &str,
    new_id: &str,
    new_name: &str,
) -> Result<(), ConnectionError> {
    service.duplicate(scope, source_id, new_id, new_name).await
}

/// 删除档案。会话退休是宿主编排，不在本用例内。
pub async fn delete_connection(
    scope: &ConnectionScope,
    service: &ConnectionService,
    connection_id: &str,
) -> Result<(), ConnectionError> {
    service.delete(scope, connection_id).await
}

/// 按给定顺序重排档案列表。
pub async fn reorder_connections(
    scope: &ConnectionScope,
    service: &ConnectionService,
    ordered_ids: Vec<String>,
) -> Result<(), ConnectionError> {
    service.reorder(scope, ordered_ids).await
}

/// 解析「测试连接」应使用的配置：表单回显的密文字段为空时，从已存档案
/// 合并回来（与保存路径同一语义），让"改了名字再测一次"不必重新输密钥。
///
/// 档案不存在（新档案权限内测试）原样返回传入配置；隧道拨号本身是宿主
/// 注入端口的编排，留在宿主。
pub async fn resolve_test_config(
    scope: &ConnectionScope,
    service: &ConnectionService,
    mut config: ConnectionConfig,
) -> Result<ConnectionConfig, ConnectionError> {
    if let Some(saved) = service.get(scope, &config.id).await? {
        merge_missing_secrets(&mut config, saved);
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct MemoryRepository {
        connections: Mutex<Vec<ConnectionConfig>>,
    }

    impl MemoryRepository {
        fn with(connections: Vec<ConnectionConfig>) -> Self {
            Self {
                connections: Mutex::new(connections),
            }
        }
    }

    #[async_trait]
    impl super::super::domain::ConnectionRepository for MemoryRepository {
        async fn list(
            &self,
            _scope: &ConnectionScope,
        ) -> Result<Vec<ConnectionConfig>, super::super::domain::ConnectionRepositoryError>
        {
            Ok(self.connections.lock().unwrap().clone())
        }

        async fn get(
            &self,
            _scope: &ConnectionScope,
            connection_id: &str,
        ) -> Result<Option<ConnectionConfig>, super::super::domain::ConnectionRepositoryError>
        {
            Ok(self
                .connections
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == connection_id)
                .cloned())
        }

        async fn save(
            &self,
            _scope: &ConnectionScope,
            config: ConnectionConfig,
        ) -> Result<(), super::super::domain::ConnectionRepositoryError> {
            let mut connections = self.connections.lock().unwrap();
            match connections.iter().position(|c| c.id == config.id) {
                Some(pos) => connections[pos] = config,
                None => connections.push(config),
            }
            Ok(())
        }

        async fn delete(
            &self,
            _scope: &ConnectionScope,
            connection_id: &str,
        ) -> Result<(), super::super::domain::ConnectionRepositoryError> {
            self.connections
                .lock()
                .unwrap()
                .retain(|c| c.id != connection_id);
            Ok(())
        }

        async fn reorder(
            &self,
            _scope: &ConnectionScope,
            ordered_ids: Vec<String>,
        ) -> Result<(), super::super::domain::ConnectionRepositoryError> {
            let mut connections = self.connections.lock().unwrap();
            connections.sort_by_key(|c| {
                ordered_ids
                    .iter()
                    .position(|id| *id == c.id)
                    .unwrap_or(usize::MAX)
            });
            Ok(())
        }

        async fn groups(
            &self,
            _scope: &ConnectionScope,
        ) -> Result<Vec<String>, super::super::domain::ConnectionRepositoryError> {
            Ok(Vec::new())
        }

        async fn save_groups(
            &self,
            _scope: &ConnectionScope,
            _groups: Vec<String>,
        ) -> Result<(), super::super::domain::ConnectionRepositoryError> {
            Ok(())
        }

        async fn touch_last_connected_at(
            &self,
            _scope: &ConnectionScope,
            _connection_id: &str,
        ) -> Result<(), super::super::domain::ConnectionRepositoryError> {
            Ok(())
        }
    }

    fn service(connections: Vec<ConnectionConfig>) -> ConnectionService {
        ConnectionService::new(Arc::new(MemoryRepository::with(connections)))
    }

    fn sample(id: &str, password: Option<&str>) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: format!("Profile {id}"),
            database_type: "postgresql".into(),
            host: Some("db.example.com".into()),
            port: Some(5432),
            database: Some("app".into()),
            username: Some("user".into()),
            password: password.map(Into::into),
            schema: None,
            ssl_mode: datazen_driver_api::SslMode::Prefer,
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    fn scope() -> ConnectionScope {
        ConnectionScope::new("org-test")
    }

    #[tokio::test]
    async fn list_connections_returns_the_redacted_projection() {
        let svc = service(vec![sample("c1", Some("secret-marker"))]);
        let list = list_connections(&scope(), &svc).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].password, None, "presentation list must be redacted");
    }

    #[tokio::test]
    async fn get_connection_returns_none_for_unknown_ids() {
        let svc = service(vec![sample("c1", None)]);
        assert!(get_connection(&scope(), &svc, "missing")
            .await
            .unwrap()
            .is_none());
        assert!(get_connection(&scope(), &svc, "c1")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn resolve_test_config_merges_saved_secrets_into_the_echo() {
        let svc = service(vec![sample("c1", Some("stored-secret"))]);
        let mut echo = sample("c1", None);
        echo.name = "Renamed".into();
        let resolved = resolve_test_config(&scope(), &svc, echo).await.unwrap();
        assert_eq!(resolved.password.as_deref(), Some("stored-secret"));
        assert_eq!(resolved.name, "Renamed");
    }

    #[tokio::test]
    async fn resolve_test_config_passes_unknown_profiles_through() {
        let svc = service(Vec::new());
        let config = sample("fresh", Some("typed-once"));
        let resolved = resolve_test_config(&scope(), &svc, config).await.unwrap();
        assert_eq!(resolved.password.as_deref(), Some("typed-once"));
        assert_eq!(resolved.id, "fresh");
    }

    #[tokio::test]
    async fn duplicate_connection_copies_without_the_connect_stamp() {
        let mut source = sample("src", None);
        source.last_connected_at = Some("2026-10-01T00:00:00Z".into());
        let svc = service(vec![source]);
        duplicate_connection(&scope(), &svc, "src", "dst", "Copy")
            .await
            .unwrap();
        let copy = get_connection(&scope(), &svc, "dst")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(copy.name, "Copy");
        assert_eq!(copy.last_connected_at, None);
    }
}
