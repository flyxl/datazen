//! A1.4: the rail's React alignment (order, icons, Settings over the toggle)
//! and the real modal dialogs (import, connection form).

use super::*;

#[test]
fn the_rail_orders_sections_like_the_react_sidebar() {
    let mut runtime = runtime();
    runtime.frame(0.0);

    // The top group follows React WorkspaceModeSidebar: Databases, Workflow,
    // Dashboard, Workspace, Extensions; Settings and the toggle sit in the
    // bottom group.
    let mut previous_y = f32::NEG_INFINITY;
    for section in [
        Section::Connections,
        Section::Workflows,
        Section::Dashboards,
        Section::Workspace,
        Section::Wapps,
    ] {
        let id = format!("rail-{}", section.key());
        let (_, y) = runtime.find(&id).unwrap_or_else(|| panic!("missing {id}"));
        assert!(
            y > previous_y,
            "{} sits under the item above it",
            section.key()
        );
        previous_y = y;
    }

    let (_, wapps_y) = runtime.find("rail-wapps").expect("the extensions entry");
    let (_, settings_y) = runtime.find("rail-settings").expect("the settings entry");
    let (_, toggle_y) = runtime.find("rail-toggle").expect("the collapse toggle");
    assert!(settings_y > wapps_y, "settings is in the bottom group");
    assert!(
        toggle_y > settings_y,
        "the toggle is under settings: settings {settings_y}, toggle {toggle_y}"
    );
}

#[test]
fn the_rail_icons_are_the_react_lucide_set() {
    // The rail draws React-aligned lucide icons for the primary sections and
    // the toggle (WorkspaceModeSidebar: Database/Workflow/Gauge/LayoutGrid/
    // Puzzle/Settings/PanelLeft*). Tools keeps its wrench icon.
    assert_eq!(
        crate::icons::section_icon(Section::Connections).name,
        "database"
    );
    assert_eq!(
        crate::icons::section_icon(Section::Workflows).name,
        "workflow"
    );
    assert_eq!(
        crate::icons::section_icon(Section::Dashboards).name,
        "gauge"
    );
    assert_eq!(
        crate::icons::section_icon(Section::Workspace).name,
        "layout-grid"
    );
    assert_eq!(crate::icons::section_icon(Section::Wapps).name, "puzzle");
    assert_eq!(crate::icons::section_icon(Section::Tools).name, "wrench");
    assert_eq!(
        crate::icons::section_icon(Section::Settings).name,
        "settings"
    );
    assert_eq!(crate::icons::ICON_PANEL_LEFT_CLOSE.name, "panel-left-close");
    assert_eq!(crate::icons::ICON_PANEL_LEFT_OPEN.name, "panel-left-open");
}

#[test]
fn the_import_dialog_is_a_modal_over_the_page() {
    use rune::{Key, KeyEvent, Mods};

    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime
        .find("import-connections")
        .expect("the import press is on the page");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);

    // A real dialog floats: the scrim layer, its title, the choose-file
    // press and the password field.
    assert!(
        runtime.find("import-dialog").is_some(),
        "the import dialog floats over the page"
    );
    assert!(
        runtime.find("import-dialog-title").is_some(),
        "the dialog names itself"
    );
    assert!(
        runtime.find("import-choose-file").is_some(),
        "the dialog picks the file itself"
    );
    assert!(
        runtime.find("import-password-input").is_some(),
        "the password rides the dialog"
    );

    // The background is inert: a rail press does nothing while it is up.
    let (rail_x, rail_y) = runtime.find("rail-workspace").expect("the rail item");
    runtime.pointer_down(rail_x, rail_y);
    runtime.pointer_up(rail_x, rail_y);
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.active,
        Section::Connections,
        "the rail stays inert under the dialog"
    );

    // Escape dismisses.
    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert!(
        !runtime.state.import_dialog_open,
        "escape dismissed the dialog"
    );
    assert!(
        runtime.find("import-dialog").is_none(),
        "the dialog layer is gone"
    );

    // It opens again, and Cancel dismisses it too. The dialog's mount
    // reveal has to land before a press reaches it, as the live window's
    // frames would (part_04 settles the same way before choosing a file).
    let (x, y) = runtime
        .find("import-connections")
        .expect("the import press again");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.settle_all();
    runtime.frame(0.0);
    let (x, y) = runtime
        .find("dialog-cancel")
        .expect("the dialog's cancel press");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    assert!(
        !runtime.state.import_dialog_open,
        "cancel dismissed the dialog"
    );
}

#[test]
fn the_connection_dialog_is_a_modal_over_the_page() {
    use rune::{Key, KeyEvent, Mods};

    let mut runtime = runtime();
    runtime.frame(0.0);
    let (x, y) = runtime
        .find("connection-new")
        .expect("the new connection press");
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.frame(0.0);

    // A real dialog floats: the scrim layer, its title and header close,
    // and the form rides inside it.
    assert!(
        runtime.find("connection-dialog").is_some(),
        "the connection dialog floats over the page"
    );
    assert!(
        runtime.find("connection-dialog-title").is_some(),
        "the dialog names itself"
    );
    assert!(
        runtime.find("connection-dialog-close").is_some(),
        "the header close is present"
    );
    assert!(
        runtime.find("connection-form").is_some(),
        "the form rides the dialog"
    );

    // The background is inert: a rail press does nothing while it is up.
    let (rail_x, rail_y) = runtime.find("rail-workspace").expect("the rail item");
    runtime.pointer_down(rail_x, rail_y);
    runtime.pointer_up(rail_x, rail_y);
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.active,
        Section::Connections,
        "the rail stays inert under the dialog"
    );

    // Escape dismisses, and the page is reachable again.
    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert!(
        !runtime.state.connection_dialog_open,
        "escape dismissed the dialog"
    );
    let (rail_x, rail_y) = runtime.find("rail-workspace").expect("the rail item");
    runtime.pointer_down(rail_x, rail_y);
    runtime.pointer_up(rail_x, rail_y);
    runtime.frame(0.0);
    assert_eq!(
        runtime.state.active,
        Section::Workspace,
        "the rail answers again once the dialog is gone"
    );
}
