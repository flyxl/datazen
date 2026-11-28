//! The live native host: the shared Rune UI over real use cases. The demo
//! process (`datazen-rune`) keeps the in-memory world; this one reads and
//! writes real files and opens real driver sessions — N2 the connection
//! store, N3 the sqlite session and the live catalog, the data paths on the
//! same adapter seam.

use std::collections::BTreeMap;
use std::io::{self, IsTerminal};

fn replace_saved_passwords() -> Result<(), &'static str> {
    if std::env::args_os().len() != 2 || io::stdin().is_terminal() {
        return Err("Pipe a JSON object mapping connection IDs to new passwords into --replace-saved-passwords.");
    }
    let invalid = "Expected one JSON object mapping existing connection IDs to nonempty passwords.";
    let replacements: BTreeMap<String, String> =
        serde_json::from_reader(io::stdin().lock()).map_err(|_| invalid)?;
    if replacements.is_empty()
        || replacements
            .iter()
            .any(|(id, value)| id.is_empty() || value.is_empty())
    {
        return Err(invalid);
    }

    let path = datazen_rune_host::default_connections_path();
    let repository = datazen_rune_host::FileConnectionRepository::open(&path)
        .map_err(|_| "Could not open the connection store.")?;
    repository.replace_saved_passwords(&replacements).map_err(|_| {
        "Could not replace saved passwords. Check the connection IDs, store permissions and encryption key."
    })
}

fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--replace-saved-passwords")
    {
        if let Err(message) = replace_saved_passwords() {
            eprintln!("{message}");
            std::process::exit(1);
        }
        return;
    }

    #[cfg(feature = "redis-e2e-fixture")]
    {
        let mut fixture_args = std::env::args().skip(1);
        if fixture_args.next().as_deref() == Some("--redis-driver-fixture") {
            datazen_driver_redis::e2e_fixture::run_cli(fixture_args);
            return;
        }
    }

    let path = datazen_rune_host::default_connections_path();
    let data_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    if !datazen_rune_host::logging::init(data_dir) {
        eprintln!("Could not initialize native console diagnostics.");
    }
    datazen_rune_host::logging::opening_profile();
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
    let app =
        datazen_ui::app_with_adapter(Box::new(datazen_rune_host::LiveAdapter::with_data_dir(
            repository, data_dir,
        )));
    datazen_rune_host::logging::start_gui(datazen_rune_host::tray::install(app.handle()));
    app.run();
}
