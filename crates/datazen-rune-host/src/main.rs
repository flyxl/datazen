//! The live native host: the shared Rune UI over real use cases. The demo
//! process (`datazen-rune`) keeps the in-memory world; this one reads and
//! writes real files and opens real driver sessions — N2 the connection
//! store, N3 the sqlite session and the live catalog, the data paths on the
//! same adapter seam.

fn main() {
    let path = datazen_rune_host::default_connections_path();
    let repository = match datazen_rune_host::FileConnectionRepository::open(&path) {
        Ok(repository) => std::sync::Arc::new(repository),
        Err(error) => {
            eprintln!("the connection store did not open: {error}");
            std::process::exit(1);
        }
    };
    // T9: the app is constructed, not run — the tray installs over the
    // handle between the two. A busless desktop logs the absence and the
    // window interface continues.
    let app = datazen_ui::app_with_adapter(Box::new(datazen_rune_host::LiveAdapter::new(
        repository,
    )));
    datazen_rune_host::tray::install(app.handle());
    app.run();
}
