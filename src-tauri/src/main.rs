#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // First statement: select the process-wide rustls backend before anything
    // can build a TLS client config. Both the GUI and `--mcp-stdio` entry points
    // run through here, and without this any rustls user panics (see
    // `datazen_desktop::install_default_crypto_provider`).
    datazen_desktop::install_default_crypto_provider();

    let args: Vec<String> = std::env::args().collect();
    if datazen_desktop::is_mcp_stdio_mode(&args) {
        datazen_desktop::run_mcp_stdio();
    } else {
        datazen_desktop::run();
    }
}
