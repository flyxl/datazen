//! The live native host: the shared Rune UI over a file-backed connection
//! store. The demo process (`datazen-rune`) keeps the in-memory world; this
//! one reads and writes real files, starting with the connection list (N2).
//! The data paths follow on the same adapter seam.

fn main() {
    let path = datazen_rune_host::default_connections_path();
    let repository = match datazen_rune_host::FileConnectionRepository::open(&path) {
        Ok(repository) => std::sync::Arc::new(repository),
        Err(error) => {
            eprintln!("the connection store did not open: {error}");
            std::process::exit(1);
        }
    };
    datazen_ui::run_with_adapter(Box::new(
        datazen_ui_api::UseCaseAdapter::from_repository(repository),
    ));
}
