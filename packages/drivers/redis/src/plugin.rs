//! Tauri plugin commands for Redis deep ops (`plugin:redis|*`).
//!
//! Handlers are stubbed for Task 3; full implementations land in Task 4.

macro_rules! stub_command {
    ($name:ident) => {
        #[tauri::command]
        async fn $name() -> Result<(), String> {
            Err("not implemented".into())
        }
    };
}

stub_command!(set_string);
stub_command!(hash_set);
stub_command!(hash_del);
stub_command!(list_push);
stub_command!(list_set);
stub_command!(list_pop);
stub_command!(set_add);
stub_command!(set_remove);
stub_command!(zset_add);
stub_command!(zset_remove);
stub_command!(delete_keys);
stub_command!(rename);
stub_command!(set_ttl);
stub_command!(batch_delete_pattern);
stub_command!(batch_set_ttl);
stub_command!(batch_rename_prefix);
stub_command!(flush_db);
stub_command!(flush_all);
stub_command!(count_matching);

/// Register Redis IPC commands as a Tauri plugin (`plugin:redis|*`).
pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("redis")
        .invoke_handler(tauri::generate_handler![
            set_string,
            hash_set,
            hash_del,
            list_push,
            list_set,
            list_pop,
            set_add,
            set_remove,
            zset_add,
            zset_remove,
            delete_keys,
            rename,
            set_ttl,
            batch_delete_pattern,
            batch_set_ttl,
            batch_rename_prefix,
            flush_db,
            flush_all,
            count_matching,
        ])
        .build()
}
