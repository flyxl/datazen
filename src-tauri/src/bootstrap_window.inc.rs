// Window events, app build, and process run loop (continuation of `run`).
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    if crate::commands::window::main_close_blocked_by_child_windows(
                        window.app_handle().webview_windows().keys().cloned(),
                    ) {
                        api.prevent_close();
                        let _ = window.minimize();
                        return;
                    }
                    if tray::should_close_to_tray(window.app_handle()) {
                        api.prevent_close();
                        // macOS: hide so the Dock icon remains; Reopen restores MainWindow.
                        // Windows/Linux: minimize so the taskbar entry can restore MainWindow.
                        #[cfg(target_os = "macos")]
                        {
                            let _ = window.hide();
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            let _ = window.minimize();
                        }
                    }
                }
            }
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::Resized(_) = event {
                let win = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    let is_fs = win.is_fullscreen().unwrap_or_else(|_| {
                        if let (Ok(cur_size), Ok(Some(monitor))) =
                            (win.inner_size(), win.current_monitor())
                        {
                            let mon = monitor.size();
                            is_fullscreen_for_monitor(
                                cur_size.width,
                                cur_size.height,
                                mon.width,
                                mon.height,
                            )
                        } else {
                            false
                        }
                    });
                    let _ = win.emit("fullscreen-changed", is_fs);

                    // Re-check after animation fully settles (macOS fullscreen animation is ~400ms)
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    let is_fs_settled = win.is_fullscreen().unwrap_or_else(|_| {
                        if let (Ok(cur_size), Ok(Some(monitor))) =
                            (win.inner_size(), win.current_monitor())
                        {
                            let mon = monitor.size();
                            is_fullscreen_for_monitor(
                                cur_size.width,
                                cur_size.height,
                                mon.width,
                                mon.height,
                            )
                        } else {
                            false
                        }
                    });
                    if is_fs_settled != is_fs {
                        let _ = win.emit("fullscreen-changed", is_fs_settled);
                    }
                });
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, .. } => {
                    if tray::should_prevent_exit(app_handle) {
                        api.prevent_exit();
                        return;
                    }
                    let state = app_handle.state::<AppState>();
                    tauri::async_runtime::block_on(async {
                        state.connection_manager.shutdown().await;
                        state.mcp_client_manager.disconnect_all().await;
                    });
                }
                // macOS Dock click (applicationShouldHandleReopen): raise MainWindow.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    tray::focus_main_window(app_handle);
                }
                _ => {}
            }
        });
}
