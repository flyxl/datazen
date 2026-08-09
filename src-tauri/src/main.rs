#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if datazen::is_mcp_stdio_mode(&args) {
        datazen::run_mcp_stdio();
    } else {
        datazen::run();
    }
}
