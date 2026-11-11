//! Shared DataZen UI entry point.
//!
//! This crate is intentionally host-agnostic: it does not depend on Tauri,
//! Axum, database drivers, or the existing React frontend. Native and wasm
//! targets compile the same Rune view tree. Host adapters will be introduced
//! only when their platform contracts are implemented.

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, col, text};

#[derive(Default)]
struct AppState;

fn view(_: &AppState, _: &mut Cx<AppState>) -> El {
    let theme = tok::theme(true);
    let title = text("DataZen")
        .font(type_::TITLE)
        .color(theme.text);
    let subtitle = text("Native UI migration bootstrap")
        .color(theme.text_muted);
    let status = text("Rune renderer is active")
        .color(theme.text);
    col()
        .key("datazen-root")
        .w_full()
        .h_full()
        .center()
        .gap(16.0)
        .fill(theme.bg)
        .child(title)
        .child(subtitle)
        .child(status)
}

/// Start the shared Rune application on the current platform.
///
/// On native targets this opens the Rune window. On wasm32, Rune mounts its
/// WebGPU canvas into the page according to its web host defaults.
pub fn run() {
    rune::run(AppSpec::new(
        "DataZen",
        (1100.0, 720.0),
        AppState::default,
        view,
    ));
}

/// WebAssembly entry point consumed by wasm-bindgen.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn shared_view_contains_the_datazen_root() {
        let env = Env {
            system_dark: true,
            scale: 1.0,
        };
        let mut runtime = Runtime::new(AppState, view, (1100.0, 720.0), env);
        runtime.frame(0.0);

        assert!(runtime.find("datazen-root").is_some());
    }
}
