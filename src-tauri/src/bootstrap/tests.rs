use super::finish_app_state;
use super::helpers::*;
use crate::store::Store;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Guards tests that read/write the shared global `settings.json` so they
/// don't race when `cargo test` runs them in parallel.
static SETTINGS_FILE_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn resolve_log_settings_defaults_without_settings_file() {
    let _guard = SETTINGS_FILE_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path();
    let (level, log_dir) = resolve_log_settings_in(data_dir);
    assert_eq!(level, "info");
    assert_eq!(log_dir, data_dir.join("logs"));
}

#[test]
fn resolve_log_settings_reads_custom_level_and_path() {
    let _guard = SETTINGS_FILE_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path();
    std::fs::create_dir_all(data_dir).unwrap();
    let settings_path = data_dir.join("settings.json");
    let custom_log = data_dir.join("custom-logs");
    let settings = serde_json::json!({
        "logLevel": "debug",
        "logPath": custom_log.to_string_lossy(),
    });
    std::fs::write(&settings_path, settings.to_string()).unwrap();

    let (level, log_dir) = resolve_log_settings_in(data_dir);
    assert_eq!(level, "debug");
    assert_eq!(log_dir, custom_log);
}

#[test]
fn unique_driver_types_deduplicates_preserving_order() {
    use crate::db::{ConnectionConfig, SslMode};

    fn conn(id: &str, db_type: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: id.into(),
            database_type: db_type.into(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::default(),
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

    let empty: Vec<ConnectionConfig> = vec![];
    assert!(unique_driver_types(&empty).is_empty());

    let connections = vec![
        conn("a", "postgres"),
        conn("b", "mysql"),
        conn("c", "postgres"),
        conn("d", "redis"),
        conn("e", "mysql"),
    ];
    assert_eq!(
        unique_driver_types(&connections),
        vec!["postgres", "mysql", "redis"]
    );
}

#[test]
fn invoke_handler_contains_tunnel_commands() {
    // Handler list lives in run.rs (single continuous function, no include! splits).
    let src = include_str!("run.rs");
    assert!(src.contains("crate::commands::get_tunnels"));
    assert!(src.contains("crate::commands::save_tunnel"));
    assert!(src.contains("crate::commands::delete_tunnel"));
    assert!(src.contains(".invoke_handler(tauri::generate_handler!["));
}

#[test]
fn test_tester_run_mcp_stdio_entry_chain_wiring() {
    let run = include_str!("run.rs");
    assert!(run.contains("pub fn run_mcp_stdio()"));
    assert!(run.contains("mcp::auth::verify_stdio_token"));
    assert!(run.contains("mcp::start_mcp_stdio"));
    assert!(run.contains("build_app_state"));
    assert!(run.contains("pub fn run()"));

    let main_rs = include_str!("../main.rs");
    assert!(main_rs.contains("datazen::is_mcp_stdio_mode"));
    assert!(main_rs.contains("datazen::run_mcp_stdio()"));
    assert!(main_rs.contains("datazen::run()"));
}

// ── Restored from the pre-split `bootstrap.rs` ──────────────────────────────
//
// `7fc55501` repointed `lib.rs` at `bootstrap/mod.rs` but left the old
// monolithic `bootstrap.rs` in the tree. That file stopped being compiled, so
// the tests below silently stopped running while still looking alive. They are
// ported here (against the split layout) so the guards are real again.

#[test]
fn parse_log_settings_fields_defaults() {
    let (level, path) = parse_log_settings_fields(&serde_json::json!({}));
    assert_eq!(level, "info");
    assert_eq!(path, "");
    let (level, path) = parse_log_settings_fields(&serde_json::json!({
        "logLevel": "warn",
        "logPath": "/var/log/datazen"
    }));
    assert_eq!(level, "warn");
    assert_eq!(path, "/var/log/datazen");
}

#[test]
fn resolve_log_dir_uses_default_or_custom() {
    let data = PathBuf::from("/data/app");
    assert_eq!(resolve_log_dir(&data, ""), PathBuf::from("/data/app/logs"));
    assert_eq!(
        resolve_log_dir(&data, "/tmp/custom"),
        PathBuf::from("/tmp/custom")
    );
}

#[test]
fn resolve_context_dir_uses_default_or_custom() {
    let data = PathBuf::from("/data/app");
    assert_eq!(
        resolve_context_dir(&data, ""),
        PathBuf::from("/data/app/contexts")
    );
    assert_eq!(
        resolve_context_dir(&data, "/home/user/ctx"),
        PathBuf::from("/home/user/ctx")
    );
}

#[test]
fn is_mcp_stdio_mode_detects_flags() {
    assert!(!is_mcp_stdio_mode(&["datazen".into()]));
    assert!(is_mcp_stdio_mode(&["datazen".into(), "--mcp".into()]));
    assert!(is_mcp_stdio_mode(&["datazen".into(), "--mcp-stdio".into()]));
    assert!(!is_mcp_stdio_mode(&["datazen".into(), "--other".into()]));
}

#[test]
fn build_tracing_env_filter_uses_log_level_when_env_unset() {
    let filter = build_tracing_env_filter("warn");
    assert!(filter.to_string().contains("warn") || filter.to_string().contains("WARN"));
}

#[test]
fn is_fullscreen_for_monitor_compares_dimensions() {
    assert!(is_fullscreen_for_monitor(1920, 1080, 1920, 1080));
    assert!(is_fullscreen_for_monitor(2000, 1200, 1920, 1080));
    assert!(!is_fullscreen_for_monitor(800, 600, 1920, 1080));
    assert!(!is_fullscreen_for_monitor(1920, 900, 1920, 1080));
}

#[test]
fn resolve_prompts_dir_appends_prompts_subdir() {
    assert_eq!(resolve_prompts_dir(None), None);
    assert_eq!(
        resolve_prompts_dir(Some(PathBuf::from("/app/resources"))),
        Some(PathBuf::from("/app/resources/prompts"))
    );
}

#[test]
fn should_auto_start_embedded_mcp_follows_setting() {
    assert!(should_auto_start_embedded_mcp(true));
    assert!(!should_auto_start_embedded_mcp(false));
}

#[tokio::test]
async fn finish_app_state_wires_core_services() {
    use crate::db::registry::DriverRegistry;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
    use crate::transfer::adapter_registry::SyncAdapterRegistry;

    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(
        Store::init_with_path(temp.path())
            .await
            .expect("store init"),
    );
    let registry = Arc::new(DriverRegistry::new());
    let mock = MockDriver::new("postgres", MockDriverOptions::default());
    registry.register_test_driver("postgres", mock).await;

    let state = finish_app_state(
        store.clone(),
        registry,
        Arc::new(SyncAdapterRegistry::new()),
        None,
    );
    assert!(state
        .workflow_registry
        .workflows_dir()
        .starts_with(temp.path()));
    assert_eq!(state.store.data_dir(), temp.path());
}

/// [tester] `finish_app_state` wires wapp manager and driver registry shells.
#[tokio::test]
async fn test_tester_finish_app_state_initializes_wapps_and_registry() {
    use crate::db::registry::DriverRegistry;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
    use crate::transfer::adapter_registry::SyncAdapterRegistry;

    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(
        Store::init_with_path(temp.path())
            .await
            .expect("store init"),
    );
    let registry = Arc::new(DriverRegistry::new());
    let db_type = "postgres".to_string();
    registry
        .register_test_driver(
            &db_type,
            MockDriver::new("postgres", MockDriverOptions::default()),
        )
        .await;

    let state = finish_app_state(
        store.clone(),
        registry.clone(),
        Arc::new(SyncAdapterRegistry::new()),
        None,
    );

    assert_eq!(state.wapps.wapps_dir(), temp.path().join("wapps"));
    assert!(Arc::ptr_eq(&state.driver_registry, &registry));
    assert!(registry.get(&db_type).await.is_some());
    assert!(state.wapps.list().is_empty());
}

fn invoke_handler_registration_block() -> &'static str {
    let src = include_str!("run.rs");
    let start = src
        .find(".invoke_handler(tauri::generate_handler![")
        .expect("invoke_handler block");
    let rest = &src[start..];
    let end = rest.find("])").expect("invoke_handler closing");
    &rest[..end]
}

/// [tester] Critical IPC commands remain registered after bootstrap extraction.
#[test]
fn test_tester_invoke_handler_registers_critical_commands() {
    let block = invoke_handler_registration_block();
    for needle in [
        "commands::get_connections,",
        "commands::execute_driver_command,",
        "commands::connect,",
        "commands::mcp_start_stdio,",
        "commands::list_wapps,",
        "app_menu::rebuild_menu,",
    ] {
        assert!(
            block.contains(needle),
            "invoke_handler missing registration: {needle}"
        );
    }
    let registered = block.matches("commands::").count() + block.matches("app_menu::").count();
    assert!(
        registered >= 150,
        "expected a large IPC surface, got {registered} command registrations"
    );
}

/// [tester] `lib.rs` preserves public crate entry points after the split.
#[test]
fn test_tester_lib_reexports_public_entry_points() {
    let lib = include_str!("../lib.rs");
    assert!(lib.contains("pub use bootstrap::{"));
    assert!(lib.contains("run_mcp_stdio"));
    assert!(lib.contains("run,"));
    // is_mcp_stdio_mode must be publicly re-exported so the `datazen` bin
    // can call it from main.rs.
    assert!(lib.contains("is_mcp_stdio_mode"));
    assert!(lib.contains("mod app_menu"));
    assert!(lib.contains("mod bootstrap"));
}

/// [tester] GUI bootstrap registers driver plugins and the datazen URI scheme.
#[test]
fn test_tester_run_registers_wapps_and_uri_scheme() {
    let src = include_str!("run.rs");
    assert!(src.contains("driver_init::register_drivers"));
    assert!(src.contains("register_uri_scheme_protocol(\"datazen\""));
    assert!(src.contains("build_gui_app_state"));
    assert!(src.contains("setup_menu"));
}

/// Every `#[tauri::command]` must be registered, and every registration must
/// resolve to a real command.
///
/// `7fc55501` moved the handler list out of `bootstrap.rs` into `run.rs` and
/// silently dropped 73 registrations — the entire AI, Workflow, Data Sync /
/// Transfer, Schema Diff and file-dialog surfaces, plus `close_database` and
/// `get_open_databases` — while leaving every command definition in place. The
/// crate still compiled and the unit tests still passed, so nothing caught it:
/// the failures only appear at runtime as "command not found".
///
/// This walks the crate source and diffs the declared set against the
/// registered set, so a dropped registration fails the build instead.
#[test]
fn every_tauri_command_is_registered_and_resolvable() {
    use std::collections::BTreeSet;

    /// Collect `#[tauri::command]` fn names from every `.rs` under `src/`.
    fn declared_commands() -> BTreeSet<String> {
        fn walk(dir: &std::path::Path, out: &mut BTreeSet<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let Ok(text) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    let lines: Vec<&str> = text.lines().collect();
                    for (i, line) in lines.iter().enumerate() {
                        if !line.trim_start().starts_with("#[tauri::command") {
                            continue;
                        }
                        // The `fn` is on the same line or the next few.
                        for candidate in lines.iter().skip(i).take(4) {
                            if let Some(pos) = candidate.find("fn ") {
                                let rest = &candidate[pos + 3..];
                                let name: String = rest
                                    .chars()
                                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                                    .collect();
                                if !name.is_empty() {
                                    out.insert(name);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut out = BTreeSet::new();
        walk(std::path::Path::new("src"), &mut out);
        out
    }

    /// Collect the names registered inside the `generate_handler!` block.
    fn registered_commands() -> BTreeSet<String> {
        let src = include_str!("run.rs");
        let start = src
            .find(".invoke_handler(tauri::generate_handler![")
            .expect("invoke_handler block");
        let rest = &src[start..];
        let end = rest.find("])").expect("invoke_handler closing");
        let mut out = BTreeSet::new();
        for line in rest[..end].lines() {
            let tail = line
                .split_once("crate::commands::")
                .or_else(|| line.split_once("crate::app_menu::"))
                .map(|(_, tail)| tail);
            if let Some(tail) = tail {
                let name: String = tail
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    out.insert(name);
                }
            }
        }
        out
    }

    let declared = declared_commands();
    let registered = registered_commands();
    assert!(
        declared.len() > 150,
        "expected to find the full IPC surface, got {} — the source walk is broken",
        declared.len()
    );

    let unregistered: Vec<&String> = declared.difference(&registered).collect();
    assert!(
        unregistered.is_empty(),
        "these #[tauri::command]s are not registered in bootstrap/run.rs, so IPC \
         fails with \"command not found\" at runtime: {unregistered:?}"
    );

    // `rebuild_menu` lives in app_menu, not commands; everything else must exist.
    let dangling: Vec<&String> = registered
        .difference(&declared)
        .filter(|n| n.as_str() != "rebuild_menu")
        .collect();
    assert!(
        dangling.is_empty(),
        "bootstrap/run.rs registers commands that no longer exist: {dangling:?}"
    );
}
