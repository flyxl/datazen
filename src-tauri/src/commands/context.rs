//! AI context file management commands.

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::State;

const ALLOWED_EXTENSIONS: &[&str] = &[
    "txt", "md", "sql", "json", "yaml", "yml", "csv", "tsv", "xml", "ddl", "schema",
];

const MAX_FILE_SIZE: u64 = 512 * 1024; // 512KB per file

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub children: Option<Vec<ContextEntry>>,
}

pub(crate) async fn resolve_context_dir_from_state(
    state: &AppState,
) -> Result<PathBuf, CommandError> {
    resolve_context_dir(state).await
}

async fn resolve_context_dir(state: &AppState) -> Result<PathBuf, CommandError> {
    let settings = state.store.get_settings().await;
    let dir = if settings.context_dir.is_empty() {
        state.store.data_dir().join("contexts")
    } else {
        PathBuf::from(&settings.context_dir)
    };
    if !dir.exists() {
        tokio::fs::create_dir_all(&dir)
            .await
            .cmd_err("context_get_dir")?;
    }
    Ok(dir)
}

fn is_allowed_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ALLOWED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn scan_dir(dir: &std::path::Path, base: &std::path::Path) -> Vec<ContextEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };
    let mut items: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
    items.sort_by_key(|e| e.file_name());

    for entry in items {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden files/dirs
        if name.starts_with('.') {
            continue;
        }
        let rel_path = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        if path.is_dir() {
            let children = scan_dir(&path, base);
            if !children.is_empty() {
                entries.push(ContextEntry {
                    name,
                    path: rel_path,
                    is_dir: true,
                    size: None,
                    children: Some(children),
                });
            }
        } else if is_allowed_file(&path) {
            let size = std::fs::metadata(&path).map(|m| m.len()).ok();
            entries.push(ContextEntry {
                name,
                path: rel_path,
                is_dir: false,
                size,
                children: None,
            });
        }
    }
    entries
}

#[tauri::command]
pub async fn context_get_dir(state: State<'_, AppState>) -> Result<String, CommandError> {
    let dir = resolve_context_dir(&state).await?;
    Ok(dir.display().to_string())
}

#[tauri::command]
pub async fn context_list_files(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<ContextEntry>, CommandError> {
    let dir = resolve_context_dir(&state).await?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = tokio::task::spawn_blocking({
        let dir = dir.clone();
        move || scan_dir(&dir, &dir)
    })
    .await
    .map_err(|e| CommandError::Internal(e.to_string()))
    .cmd_err("context_list_files")?;

    if let Some(q) = query.filter(|q| !q.is_empty()) {
        let q_lower = q.to_lowercase();
        Ok(filter_entries(&entries, &q_lower))
    } else {
        Ok(entries)
    }
}

fn filter_entries(entries: &[ContextEntry], query: &str) -> Vec<ContextEntry> {
    let mut result = Vec::new();
    for entry in entries {
        if entry.is_dir {
            if let Some(ref children) = entry.children {
                let filtered = filter_entries(children, query);
                if !filtered.is_empty() {
                    result.push(ContextEntry {
                        name: entry.name.clone(),
                        path: entry.path.clone(),
                        is_dir: true,
                        size: None,
                        children: Some(filtered),
                    });
                } else if entry.name.to_lowercase().contains(query) {
                    result.push(entry.clone());
                }
            }
        } else if entry.name.to_lowercase().contains(query) {
            result.push(entry.clone());
        }
    }
    result
}

/// Read allowed context files under `dir`, enforcing canonicalize + path-traversal guards.
pub(crate) async fn read_context_paths(
    dir: &Path,
    paths: &[String],
) -> Result<Vec<(String, String)>, CommandError> {
    let mut results = Vec::new();

    for rel_path in paths {
        let full_path = dir.join(rel_path);
        // Security: ensure resolved path is under context_dir
        let canonical = super::error::assert_under_dir(dir, &full_path, "context_read_files")?;

        if canonical.is_dir() {
            // Read all allowed files in directory recursively
            let files = collect_files_in_dir(&canonical);
            for file_path in files {
                if let Ok(content) = read_single_file(&file_path).await {
                    let display = file_path
                        .strip_prefix(dir)
                        .unwrap_or(&file_path)
                        .to_string_lossy()
                        .to_string();
                    results.push((display, content));
                }
            }
        } else if let Ok(content) = read_single_file(&canonical).await {
            results.push((rel_path.clone(), content));
        }
    }

    Ok(results)
}

/// Format file entries as a prompt context block (`[Context: path]\\ncontent`, joined by blank lines).
pub(crate) fn format_context_block(entries: &[(String, String)]) -> String {
    entries
        .iter()
        .map(|(path, content)| format!("[Context: {path}]\n{content}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[tauri::command]
pub async fn context_read_files(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Vec<(String, String)>, CommandError> {
    let dir = resolve_context_dir(&state).await?;
    read_context_paths(&dir, &paths).await
}

pub(crate) fn collect_files_in_dir(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_files_in_dir(&path));
            } else if is_allowed_file(&path) {
                files.push(path);
            }
        }
    }
    files
}

pub(crate) async fn read_single_file(path: &std::path::Path) -> Result<String, CommandError> {
    let meta = tokio::fs::metadata(path)
        .await
        .cmd_err("context_read_files")?;
    if meta.len() > MAX_FILE_SIZE {
        return Err(CommandError::Validation(format!(
            "File too large: {} bytes (max {})",
            meta.len(),
            MAX_FILE_SIZE,
        )));
    }
    tokio::fs::read_to_string(path)
        .await
        .cmd_err("context_read_files")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_allowed_file_accepts_valid_extensions() {
        let valid = [
            "schema.sql",
            "notes.md",
            "data.json",
            "config.yaml",
            "config.yml",
            "readme.txt",
            "data.csv",
            "data.tsv",
            "feed.xml",
            "create.ddl",
            "users.schema",
        ];
        for name in valid {
            assert!(
                is_allowed_file(std::path::Path::new(name)),
                "{name} should be allowed"
            );
        }
    }

    #[test]
    fn test_is_allowed_file_rejects_disallowed_extensions() {
        let invalid = [
            "image.png",
            "binary.exe",
            "archive.zip",
            "script.sh",
            "library.so",
            "code.rs",
            "style.css",
            "noext",
        ];
        for name in invalid {
            assert!(
                !is_allowed_file(std::path::Path::new(name)),
                "{name} should NOT be allowed"
            );
        }
    }

    #[test]
    fn test_is_allowed_file_case_insensitive() {
        assert!(is_allowed_file(std::path::Path::new("DATA.SQL")));
        assert!(is_allowed_file(std::path::Path::new("README.MD")));
        assert!(is_allowed_file(std::path::Path::new("config.YAML")));
    }

    #[test]
    fn test_scan_dir_finds_files_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("schema.sql"), "CREATE TABLE t;").unwrap();
        std::fs::write(dir.path().join("notes.md"), "# Notes").unwrap();
        std::fs::write(dir.path().join("ignored.png"), "binary").unwrap();

        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("data.json"), "{}").unwrap();

        let entries = scan_dir(dir.path(), dir.path());
        assert_eq!(entries.len(), 3); // notes.md, schema.sql, sub/

        let file_names: Vec<&str> = entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.name.as_str())
            .collect();
        assert!(file_names.contains(&"schema.sql"));
        assert!(file_names.contains(&"notes.md"));
        assert!(!file_names.iter().any(|n| *n == "ignored.png"));

        let dir_entry = entries.iter().find(|e| e.is_dir).unwrap();
        assert_eq!(dir_entry.name, "sub");
        assert_eq!(dir_entry.children.as_ref().unwrap().len(), 1);
        assert_eq!(dir_entry.children.as_ref().unwrap()[0].name, "data.json");
    }

    #[test]
    fn test_scan_dir_skips_hidden_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".hidden.txt"), "secret").unwrap();
        std::fs::write(dir.path().join("visible.txt"), "hello").unwrap();
        let hidden_dir = dir.path().join(".git");
        std::fs::create_dir(&hidden_dir).unwrap();
        std::fs::write(hidden_dir.join("config"), "stuff").unwrap();

        let entries = scan_dir(dir.path(), dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "visible.txt");
    }

    #[test]
    fn test_scan_dir_empty_subdir_omitted() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("empty_sub");
        std::fs::create_dir(&sub).unwrap();

        let entries = scan_dir(dir.path(), dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_scan_dir_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("b.sql"), "SELECT 1").unwrap();

        let entries = scan_dir(dir.path(), dir.path());
        let dir_entry = entries.iter().find(|e| e.is_dir).unwrap();
        assert_eq!(dir_entry.path, "a");
        let file_entry = &dir_entry.children.as_ref().unwrap()[0];
        // Path separator may differ on Windows
        assert!(file_entry.path.contains("b.sql"));
    }

    #[test]
    fn test_filter_entries_by_name() {
        let entries = vec![
            ContextEntry {
                name: "users.sql".into(),
                path: "users.sql".into(),
                is_dir: false,
                size: Some(100),
                children: None,
            },
            ContextEntry {
                name: "orders.sql".into(),
                path: "orders.sql".into(),
                is_dir: false,
                size: Some(200),
                children: None,
            },
            ContextEntry {
                name: "config.yaml".into(),
                path: "config.yaml".into(),
                is_dir: false,
                size: Some(50),
                children: None,
            },
        ];

        let result = filter_entries(&entries, "user");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "users.sql");

        let result = filter_entries(&entries, "sql");
        assert_eq!(result.len(), 2);

        let result = filter_entries(&entries, "xyz");
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_entries_searches_in_subdirs() {
        let entries = vec![ContextEntry {
            name: "schemas".into(),
            path: "schemas".into(),
            is_dir: true,
            size: None,
            children: Some(vec![
                ContextEntry {
                    name: "users.sql".into(),
                    path: "schemas/users.sql".into(),
                    is_dir: false,
                    size: Some(100),
                    children: None,
                },
                ContextEntry {
                    name: "products.sql".into(),
                    path: "schemas/products.sql".into(),
                    is_dir: false,
                    size: Some(200),
                    children: None,
                },
            ]),
        }];

        let result = filter_entries(&entries, "user");
        assert_eq!(result.len(), 1);
        assert!(result[0].is_dir);
        assert_eq!(result[0].children.as_ref().unwrap().len(), 1);
        assert_eq!(result[0].children.as_ref().unwrap()[0].name, "users.sql");
    }

    #[test]
    fn test_filter_entries_case_insensitive() {
        let entries = vec![ContextEntry {
            name: "README.md".into(),
            path: "README.md".into(),
            is_dir: false,
            size: Some(50),
            children: None,
        }];

        let result = filter_entries(&entries, "readme");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_collect_files_in_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.sql"), "SELECT 1").unwrap();
        std::fs::write(dir.path().join("b.png"), "binary").unwrap();
        let sub = dir.path().join("nested");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("c.md"), "# doc").unwrap();

        let files = collect_files_in_dir(dir.path());
        assert_eq!(files.len(), 2);
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"a.sql".to_string()));
        assert!(names.contains(&"c.md".to_string()));
        assert!(!names.contains(&"b.png".to_string()));
    }

    #[tokio::test]
    async fn test_read_single_file_success() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();

        let content = read_single_file(&file).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_read_single_file_too_large() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("big.txt");
        let big_content = "x".repeat((MAX_FILE_SIZE + 1) as usize);
        std::fs::write(&file, big_content).unwrap();

        let result = read_single_file(&file).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("File too large"), "got: {err}");
    }

    #[tokio::test]
    async fn test_read_single_file_not_found() {
        let result = read_single_file(std::path::Path::new("/nonexistent/path.txt")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_context_paths_rejects_dotdot_traversal() {
        let root = tempfile::tempdir().unwrap();
        let ctx = root.path().join("ctx");
        std::fs::create_dir(&ctx).unwrap();
        std::fs::write(ctx.join("ok.txt"), "inside").unwrap();
        std::fs::write(root.path().join("secret.txt"), "leak").unwrap();

        let result = read_context_paths(&ctx, &["../secret.txt".into()]).await;
        assert!(result.is_err(), "expected error for ../ traversal");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Path traversal not allowed"), "got: {err}");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_read_context_paths_rejects_symlink_escape() {
        let root = tempfile::tempdir().unwrap();
        let ctx = root.path().join("ctx");
        let outside = root.path().join("outside");
        std::fs::create_dir(&ctx).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "leak").unwrap();
        std::os::unix::fs::symlink(&outside, ctx.join("escape_link")).unwrap();

        let result = read_context_paths(&ctx, &["escape_link/secret.txt".into()]).await;
        assert!(result.is_err(), "expected traversal rejection via symlink");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Path traversal not allowed"), "got: {err}");
    }

    #[tokio::test]
    async fn test_read_context_paths_reads_allowed_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.md"), "# hello").unwrap();

        let entries = read_context_paths(dir.path(), &["notes.md".into()])
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "notes.md");
        assert_eq!(entries[0].1, "# hello");
    }

    #[test]
    fn test_format_context_block() {
        let entries = vec![
            ("a.sql".into(), "SELECT 1".into()),
            ("b.md".into(), "# doc".into()),
        ];
        let block = format_context_block(&entries);
        assert_eq!(
            block,
            "[Context: a.sql]\nSELECT 1\n\n[Context: b.md]\n# doc"
        );
    }

    #[test]
    fn test_filter_entries_matches_dir_name_when_children_empty() {
        let entries = vec![ContextEntry {
            name: "schemas".into(),
            path: "schemas".into(),
            is_dir: true,
            size: None,
            children: Some(vec![ContextEntry {
                name: "other.sql".into(),
                path: "schemas/other.sql".into(),
                is_dir: false,
                size: Some(10),
                children: None,
            }]),
        }];
        let result = filter_entries(&entries, "schemas");
        assert_eq!(result.len(), 1);
        assert!(result[0].is_dir);
    }

    #[tokio::test]
    async fn test_resolve_context_dir_from_state_uses_default_subdir() {
        use crate::ai::{AiProviderRegistry, PromptResolver, SchemaContextBuilder};
        use crate::cache::SchemaCache;
        use crate::commands::AppState;
        use crate::db::DriverRegistry;
        use crate::mcp::client::McpClientManager;
        use crate::monitor::{MonitorConnectionRegistry, MonitorEngine};
        use crate::services::ConnectionManager;
        use crate::store::Store;
        use crate::workflow::{WorkflowHistoryManager, WorkflowRegistry};
        use crate::SyncAdapterRegistry;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("DATAZEN_KEYRING", "file");
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        let registry = Arc::new(DriverRegistry::new());
        let connection_manager = Arc::new(ConnectionManager::new(registry.clone(), store.clone()));
        let monitor_connections =
            Arc::new(MonitorConnectionRegistry::new(connection_manager.clone()));
        let monitor_engine = MonitorEngine::new(store.clone());
        let schema_cache = Arc::new(SchemaCache::new(registry.clone()));
        let data_dir = store.data_dir().to_path_buf();
        let history_db = store.history_db();
        let app_db = store.app_db();

        let state = AppState {
            driver_registry: registry,
            connection_manager: connection_manager.clone(),
            monitor_connections,
            monitor_engine,
            store,
            schema_cache: schema_cache.clone(),
            sync_adapters: Arc::new(SyncAdapterRegistry::new()),
            ai_registry: Arc::new(AiProviderRegistry::new()),
            schema_context_builder: Arc::new(SchemaContextBuilder::new(
                schema_cache,
                connection_manager,
            )),
            prompt_resolver: Arc::new(PromptResolver::new(&data_dir, None)),
            workflow_registry: Arc::new(WorkflowRegistry::new(app_db, data_dir.clone())),
            workflow_history: Arc::new(WorkflowHistoryManager::new(history_db)),
            mcp_client_manager: Arc::new(McpClientManager::new()),
            session_transactions: Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            workflow_scheduler: crate::workflow::scheduler::WorkflowScheduler::new(),
            extensions: Arc::new(crate::extensions::ExtensionManager::new(
                data_dir.join("plugins"),
            )),
        };

        let ctx_dir = resolve_context_dir_from_state(&state).await.unwrap();
        assert!(ctx_dir.ends_with("contexts"));
        assert!(ctx_dir.exists());
    }

    #[tokio::test]
    async fn test_resolve_context_dir_honors_custom_setting() {
        use crate::ai::{AiProviderRegistry, PromptResolver, SchemaContextBuilder};
        use crate::cache::SchemaCache;
        use crate::commands::AppState;
        use crate::db::DriverRegistry;
        use crate::mcp::client::McpClientManager;
        use crate::monitor::{MonitorConnectionRegistry, MonitorEngine};
        use crate::services::ConnectionManager;
        use crate::store::{AppSettings, Store};
        use crate::workflow::{WorkflowHistoryManager, WorkflowRegistry};
        use crate::SyncAdapterRegistry;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let custom = dir.path().join("my-contexts");
        std::env::set_var("DATAZEN_KEYRING", "file");
        let store = Arc::new(Store::init_with_path(dir.path()).await.unwrap());
        store
            .save_settings(AppSettings {
                context_dir: custom.display().to_string(),
                ..AppSettings::default()
            })
            .await
            .unwrap();

        let registry = Arc::new(DriverRegistry::new());
        let connection_manager = Arc::new(ConnectionManager::new(registry.clone(), store.clone()));
        let monitor_connections =
            Arc::new(MonitorConnectionRegistry::new(connection_manager.clone()));
        let monitor_engine = MonitorEngine::new(store.clone());
        let schema_cache = Arc::new(SchemaCache::new(registry.clone()));
        let data_dir = store.data_dir().to_path_buf();
        let history_db = store.history_db();

        let state = AppState {
            driver_registry: registry,
            connection_manager: connection_manager.clone(),
            monitor_connections,
            monitor_engine,
            store: store.clone(),
            schema_cache: schema_cache.clone(),
            sync_adapters: Arc::new(SyncAdapterRegistry::new()),
            ai_registry: Arc::new(AiProviderRegistry::new()),
            schema_context_builder: Arc::new(SchemaContextBuilder::new(
                schema_cache,
                connection_manager,
            )),
            prompt_resolver: Arc::new(PromptResolver::new(&data_dir, None)),
            workflow_registry: Arc::new(WorkflowRegistry::new(store.app_db(), data_dir.clone())),
            workflow_history: Arc::new(WorkflowHistoryManager::new(history_db)),
            mcp_client_manager: Arc::new(McpClientManager::new()),
            session_transactions: Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            workflow_scheduler: crate::workflow::scheduler::WorkflowScheduler::new(),
            extensions: Arc::new(crate::extensions::ExtensionManager::new(
                data_dir.join("plugins"),
            )),
        };

        let ctx_dir = resolve_context_dir_from_state(&state).await.unwrap();
        assert_eq!(ctx_dir, custom);
    }

    #[test]
    fn test_path_traversal_guard_rejects_escape() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("contexts");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("safe.txt"), "ok").unwrap();
        std::fs::write(dir.path().join("outside.txt"), "secret").unwrap();

        let rel = "../outside.txt";
        let full = base.join(rel);
        let canonical = full.canonicalize().expect("resolve outside");
        let canonical_base = base.canonicalize().unwrap();
        assert!(
            !canonical.starts_with(&canonical_base),
            "traversal path must escape context root"
        );
    }

    #[test]
    fn test_scan_dir_unreadable_returns_empty() {
        #[cfg(unix)]
        {
            let dir = tempfile::tempdir().unwrap();
            let restricted = dir.path().join("locked");
            std::fs::create_dir(&restricted).unwrap();
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&restricted).unwrap().permissions();
            perms.set_mode(0o000);
            std::fs::set_permissions(&restricted, perms).unwrap();
            let entries = scan_dir(&restricted, dir.path());
            assert!(entries.is_empty());
            // restore for cleanup
            let mut perms = std::fs::metadata(&restricted).unwrap().permissions();
            perms.set_mode(0o700);
            let _ = std::fs::set_permissions(&restricted, perms);
        }
    }
}
