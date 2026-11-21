#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionShareMode {
    Export,
    Import,
}

impl ConnectionShareMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Import => "import",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionImportSource {
    File,
    Dbx,
    Navicat,
    DataGrip,
    Dbeaver,
    TablePlus,
}

impl ConnectionImportSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Dbx => "dbx",
            Self::Navicat => "navicat",
            Self::DataGrip => "datagrip",
            Self::Dbeaver => "dbeaver",
            Self::TablePlus => "tableplus",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionShareState {
    pub open: bool,
    pub mode: ConnectionShareMode,
    pub import_source: ConnectionImportSource,
}

impl Default for ConnectionShareState {
    fn default() -> Self {
        Self {
            open: false,
            mode: ConnectionShareMode::Export,
            import_source: ConnectionImportSource::File,
        }
    }
}

impl ConnectionShareState {
    pub fn open_connection_share_dialog(
        &mut self,
        mode: ConnectionShareMode,
        import_source: Option<ConnectionImportSource>,
    ) {
        self.open = true;
        self.mode = mode;
        self.import_source = import_source.unwrap_or(ConnectionImportSource::File);
    }

    pub fn close_connection_share_dialog(&mut self) {
        self.open = false;
    }
}

pub fn open_connection_share_dialog(
    state: &mut ConnectionShareState,
    mode: ConnectionShareMode,
    import_source: Option<ConnectionImportSource>,
) {
    state.open_connection_share_dialog(mode, import_source);
}

pub fn close_connection_share_dialog(state: &mut ConnectionShareState) {
    state.close_connection_share_dialog();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ConnectionShareState {
        ConnectionShareState::default()
    }

    #[test]
    fn the_initial_state_is_closed_on_export_from_file() {
        let initial = state();
        assert!(!initial.open);
        assert_eq!(initial.mode, ConnectionShareMode::Export);
        assert_eq!(initial.import_source, ConnectionImportSource::File);
    }

    #[test]
    fn opening_without_a_source_defaults_to_file() {
        let mut current = state();
        current.open_connection_share_dialog(ConnectionShareMode::Export, None);
        assert!(current.open);
        assert_eq!(current.mode, ConnectionShareMode::Export);
        assert_eq!(current.import_source, ConnectionImportSource::File);
    }

    #[test]
    fn an_import_source_is_kept_verbatim() {
        for source in [
            ConnectionImportSource::Dbx,
            ConnectionImportSource::Navicat,
            ConnectionImportSource::DataGrip,
            ConnectionImportSource::Dbeaver,
            ConnectionImportSource::TablePlus,
        ] {
            let mut current = state();
            current.open_connection_share_dialog(ConnectionShareMode::Import, Some(source));
            assert!(current.open);
            assert_eq!(current.mode, ConnectionShareMode::Import);
            assert_eq!(current.import_source, source);
        }
    }

    #[test]
    fn close_only_clears_open_and_keeps_mode_and_source() {
        let mut current = state();
        current.open_connection_share_dialog(
            ConnectionShareMode::Import,
            Some(ConnectionImportSource::TablePlus),
        );
        current.close_connection_share_dialog();
        assert!(!current.open);
        assert_eq!(current.mode, ConnectionShareMode::Import);
        assert_eq!(current.import_source, ConnectionImportSource::TablePlus);
    }

    #[test]
    fn closing_before_any_open_leaves_the_defaults() {
        let mut current = state();
        current.close_connection_share_dialog();
        assert_eq!(current, state());
    }

    #[test]
    fn reopening_overwrites_mode_and_source_without_a_counter() {
        let mut current = state();
        current.open_connection_share_dialog(
            ConnectionShareMode::Import,
            Some(ConnectionImportSource::Dbx),
        );
        current.open_connection_share_dialog(ConnectionShareMode::Export, None);
        assert!(current.open);
        assert_eq!(current.mode, ConnectionShareMode::Export);
        assert_eq!(current.import_source, ConnectionImportSource::File);
    }

    #[test]
    fn mode_spellings_are_the_react_literals() {
        assert_eq!(ConnectionShareMode::Export.as_str(), "export");
        assert_eq!(ConnectionShareMode::Import.as_str(), "import");
    }

    #[test]
    fn source_spellings_are_the_react_literals() {
        assert_eq!(ConnectionImportSource::File.as_str(), "file");
        assert_eq!(ConnectionImportSource::Dbx.as_str(), "dbx");
        assert_eq!(ConnectionImportSource::Navicat.as_str(), "navicat");
        assert_eq!(ConnectionImportSource::DataGrip.as_str(), "datagrip");
        assert_eq!(ConnectionImportSource::Dbeaver.as_str(), "dbeaver");
        assert_eq!(ConnectionImportSource::TablePlus.as_str(), "tableplus");
    }

    #[test]
    fn the_free_functions_drive_the_same_transitions() {
        let mut current = state();
        open_connection_share_dialog(
            &mut current,
            ConnectionShareMode::Import,
            Some(ConnectionImportSource::Navicat),
        );
        assert_eq!(
            current,
            ConnectionShareState {
                open: true,
                mode: ConnectionShareMode::Import,
                import_source: ConnectionImportSource::Navicat,
            }
        );
        close_connection_share_dialog(&mut current);
        assert_eq!(
            current,
            ConnectionShareState {
                open: false,
                mode: ConnectionShareMode::Import,
                import_source: ConnectionImportSource::Navicat,
            }
        );
    }
}
