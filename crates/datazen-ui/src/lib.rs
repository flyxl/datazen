//! Shared DataZen UI entry point for native and WebAssembly hosts.
//!
//! The page shell is deliberately independent of DataZen's transport and storage
//! implementations. Platform adapters will be added behind typed UI ports.

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, col, row, text};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Section {
    #[default]
    Connections,
    SqlEditor,
    DataBrowser,
    Schema,
    Workflows,
    Settings,
}

impl Section {
    const ALL: [Self; 6] = [
        Self::Connections,
        Self::SqlEditor,
        Self::DataBrowser,
        Self::Schema,
        Self::Workflows,
        Self::Settings,
    ];

    const fn key(self) -> &'static str {
        match self {
            Self::Connections => "connections",
            Self::SqlEditor => "sql-editor",
            Self::DataBrowser => "data-browser",
            Self::Schema => "schema",
            Self::Workflows => "workflows",
            Self::Settings => "settings",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Connections => "Connections",
            Self::SqlEditor => "SQL Editor",
            Self::DataBrowser => "Data Browser",
            Self::Schema => "Schema",
            Self::Workflows => "Workflows",
            Self::Settings => "Settings",
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::Connections => "Manage database profiles and open a session.",
            Self::SqlEditor => "Run SQL, inspect execution state and review query results.",
            Self::DataBrowser => "Browse rows, edit cells and inspect table data.",
            Self::Schema => "Explore databases, schemas and database objects.",
            Self::Workflows => "Compose repeatable database and automation workflows.",
            Self::Settings => "Configure appearance, editor behaviour and platform options.",
        }
    }

    const fn next_step(self) -> &'static str {
        match self {
            Self::Connections => "Next implementation: connect this screen to ConnectionUseCases.",
            Self::SqlEditor => "The CodeMirror parity POC is a migration gate; this page is not an editor yet.",
            Self::DataBrowser => "The DataGrid parity POC is a migration gate; no database rows are loaded yet.",
            Self::Schema => "Schema metadata will be supplied by the shared DataZen application API.",
            Self::Workflows => "Workflow execution stays in the existing DataZen business core.",
            Self::Settings => "Platform-specific settings will be exposed as explicit host capabilities.",
        }
    }
}

#[derive(Debug, Default)]
struct AppState {
    active: Section,
}

impl AppState {
    fn select(&mut self, section: Section) {
        self.active = section;
    }
}

fn navigation_item(state: &AppState, section: Section, dark: bool) -> El {
    let theme = tok::theme(dark);
    let selected = state.active == section;

    row()
        .key(format!("nav-{}", section.key()))
        .w_full()
        .h(36.0)
        .px(12.0)
        .center_y()
        .radius(7.0)
        .fill(if selected {
            tok::accent(0, dark)
        } else {
            theme.control_off
        })
        .child(
            text(section.label())
                .font(type_::LABEL)
                .color(theme.text),
        )
        .role("tab")
        .label(section.label())
        .on_press(move |state: &mut AppState| state.select(section))
}
fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = cx.system_dark();
    let theme = tok::theme(dark);

    let mut navigation = col().w_full().gap(4.0);
    for section in Section::ALL {
        navigation = navigation.child(navigation_item(state, section, dark));
    }

    let sidebar = col()
        .key("datazen-sidebar")
        .w(224.0)
        .h_full()
        .px(16.0)
        .py(20.0)
        .gap(18.0)
        .fill(theme.bg)
        .child(
            text("DataZen")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            text("DATABASE WORKSPACE")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(navigation)
        .child(
            col()
                .w_full()
                .h(1.0)
                .fill(theme.control_off),
        )
        .child(
            text("Rune native UI preview")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );

    let header = row()
        .key("datazen-header")
        .w_full()
        .h(48.0)
        .center_y()
        .gap(12.0)
        .child(
            text(state.active.label())
                .font(type_::TITLE)
                .color(theme.text)
                .w_full(),
        )
        .child(
            text(if dark { "DARK" } else { "LIGHT" })
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );

    let page = col()
        .key("datazen-page")
        .w_full()
        .h_full()
        .px(24.0)
        .py(24.0)
        .gap(16.0)
        .fill(theme.bg)
        .child(header)
        .child(
            text(state.active.description())
                .font(type_::LABEL)
                .color(theme.text),
        )
        .child(
            col()
                .key("datazen-preview-card")
                .w_full()
                .h(220.0)
                .px(20.0)
                .py(20.0)
                .gap(12.0)
                .radius(12.0)
                .fill(theme.control_off)
                .child(
                    text("Shared Rune view is running")
                        .font(type_::TITLE)
                        .color(theme.text),
                )
                .child(
                    text("This interactive shell is the first UI slice shared by the native desktop binary and the WebAssembly entrypoint.")
                        .font(type_::LABEL)
                        .color(theme.text_secondary)
                        .w_full(),
                )
                .child(
                    text(state.active.next_step())
                        .font(type_::LABEL)
                        .color(theme.text_secondary)
                        .w_full(),
                ),
        )
        .child(
            row()
                .w_full()
                .h(28.0)
                .center_y()
                .child(
                    text("Migration preview · existing React and Tauri entrypoints are unchanged")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );

    row()
        .key("datazen-root")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(sidebar)
        .child(page)
}

/// Launch the same Rune view on native desktop and WebAssembly.
pub fn run() {
    rune::run(AppSpec::new(
        "DataZen",
        (1440.0, 900.0),
        AppState::default,
        view,
    ));
}

/// WebAssembly entrypoint consumed by wasm-bindgen.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    run();
}

#[cfg(test)]
mod tests {
    use super::{AppState, Section, view};
    use rune::{Env, Runtime};

    fn runtime() -> Runtime<AppState> {
        let env = Env {
            system_dark: true,
            scale: 1.0,
        };
        Runtime::new(AppState::default(), view, (1440.0, 900.0), env)
    }

    #[test]
    fn shell_renders_the_datazen_navigation() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        assert!(runtime.find("datazen-root").is_some());
        for section in Section::ALL {
            let id = format!("nav-{}", section.key());
            assert!(runtime.find(&id).is_some(), "missing navigation node: {id}");
        }
    }

    #[test]
    fn clicking_navigation_changes_the_active_section() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime
            .find("nav-sql-editor")
            .expect("SQL Editor navigation item should exist");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);

        assert_eq!(runtime.state.active, Section::SqlEditor);
    }
}
