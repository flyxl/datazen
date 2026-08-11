fn main() {
    // Redis Driver Commands go through host `execute_driver_command`.
    // This plugin remains only to install the Pub/Sub event sink at startup.
    tauri_plugin::Builder::new(&[]).build();
}
