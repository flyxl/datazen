//! SQLite endpoint selection and results for the live native tools.
use crate::state_def::AppState;
use datazen_ui_api::native_transfer::{
    NativeSchemaDiff, NativeSyncPreview, NativeTransferRequest, SqliteReadComparison,
    TransferCatalog, TransferChoice, TransferEndpoint, TransferSide,
};
use datazen_ui_api::AdapterJob;

#[derive(Default)]
pub(crate) struct EndpointState {
    pub active: Option<TransferChoice>,
    pub catalog: Option<TransferCatalog>,
    pub loading: bool,
    pub error: Option<String>,
    pub column: usize,
    epoch: u32,
}

impl EndpointState {
    pub fn selected_column(&self) -> Option<&str> {
        self.catalog
            .as_ref()?
            .columns
            .get(self.column)
            .map(String::as_str)
    }

    fn endpoint(&self) -> Option<TransferEndpoint> {
        let catalog = self.catalog.as_ref()?;
        Some(TransferEndpoint {
            connection_id: catalog.choice.connection_id.clone(),
            table: catalog.choice.table.clone()?,
            identity: catalog.identity.clone(),
        })
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum NativeTool {
    #[default]
    Transfer,
    SchemaDiff,
    SyncPreview,
}

#[derive(Default)]
pub(crate) struct NativeTransferState {
    pub source: EndpointState,
    pub target: EndpointState,
    pub pairs: Vec<(String, String)>,
    pub confirming: Option<NativeTransferRequest>,
    pub running: bool,
    pub error: Option<String>,
    pub inserted: Option<u64>,
    pub tool: NativeTool,
    pub comparing_schema: bool,
    pub schema_error: Option<String>,
    pub schema_diff: Option<NativeSchemaDiff>,
    pub previewing_sync: bool,
    pub sync_error: Option<String>,
    pub sync_preview: Option<NativeSyncPreview>,
    compare_epoch: u32,
    epoch: u32,
}

impl NativeTransferState {
    pub fn side(&self, side: TransferSide) -> &EndpointState {
        match side {
            TransferSide::Source => &self.source,
            TransferSide::Target => &self.target,
        }
    }

    fn side_mut(&mut self, side: TransferSide) -> &mut EndpointState {
        match side {
            TransferSide::Source => &mut self.source,
            TransferSide::Target => &mut self.target,
        }
    }

    pub fn busy(&self) -> bool {
        self.confirming.is_some() || self.running || self.comparing_schema || self.previewing_sync
    }

    pub fn read_completed(
        &mut self,
        epoch: u32,
        side: TransferSide,
        result: Result<TransferCatalog, String>,
    ) {
        let endpoint = self.side_mut(side);
        if epoch != endpoint.epoch || !endpoint.loading {
            return;
        }
        endpoint.loading = false;
        match result {
            Ok(catalog) => {
                endpoint.active = Some(catalog.choice.clone());
                endpoint.catalog = Some(catalog);
            }
            Err(error) => endpoint.error = Some(error),
        }
    }

    pub fn run_completed(&mut self, epoch: u32, result: Result<u64, String>) {
        if epoch != self.epoch || !self.running {
            return;
        }
        self.running = false;
        match result {
            Ok(rows) => self.inserted = Some(rows),
            Err(error) => self.error = Some(error),
        }
    }

    pub fn schema_completed(&mut self, epoch: u32, result: Result<NativeSchemaDiff, String>) {
        if epoch != self.compare_epoch || !self.comparing_schema {
            return;
        }
        self.comparing_schema = false;
        match result {
            Ok(diff) => self.schema_diff = Some(diff),
            Err(error) => self.schema_error = Some(error),
        }
    }

    pub fn sync_completed(&mut self, epoch: u32, result: Result<NativeSyncPreview, String>) {
        if epoch != self.compare_epoch || !self.previewing_sync {
            return;
        }
        self.previewing_sync = false;
        match result {
            Ok(preview) => self.sync_preview = Some(preview),
            Err(error) => self.sync_error = Some(error),
        }
    }

    fn comparison_request(&self) -> Result<SqliteReadComparison, String> {
        let (Some(source), Some(target)) = (self.source.endpoint(), self.target.endpoint()) else {
            return Err("Select an existing source and target table.".into());
        };
        if source.identity == target.identity {
            return Err("Select two different SQLite files.".into());
        }
        Ok(SqliteReadComparison { source, target })
    }

    pub fn step_column(&mut self, side: TransferSide) {
        if self.busy() {
            return;
        }
        let endpoint = self.side_mut(side);
        let size = endpoint
            .catalog
            .as_ref()
            .map_or(0, |catalog| catalog.columns.len());
        if size > 0 {
            endpoint.column = (endpoint.column + 1) % size;
        }
    }

    pub fn add_pair(&mut self) {
        if self.busy() {
            return;
        }
        let (Some(left), Some(right)) =
            (self.source.selected_column(), self.target.selected_column())
        else {
            self.error = Some("Select a column on each side.".into());
            return;
        };
        if self.pairs.iter().any(|(l, r)| l == left || r == right) {
            self.error = Some("Each column can be mapped once.".into());
            return;
        }
        self.pairs.push((left.to_owned(), right.to_owned()));
        self.error = None;
    }

    pub fn remove_pair(&mut self, index: usize) {
        if !self.busy() && index < self.pairs.len() {
            self.pairs.remove(index);
            self.error = None;
        }
    }

    pub fn review(&mut self) {
        if self.busy() {
            return;
        }
        self.error = None;
        self.inserted = None;
        let (Some(source), Some(target)) = (self.source.endpoint(), self.target.endpoint()) else {
            self.error = Some("Select an existing source and target table.".into());
            return;
        };
        if source.identity == target.identity {
            self.error = Some("Select two different SQLite files.".into());
        } else if self
            .target
            .catalog
            .as_ref()
            .is_some_and(|catalog| catalog.read_only)
        {
            self.error = Some("The target is read-only.".into());
        } else if self.pairs.is_empty() {
            self.error = Some("Add at least one column mapping.".into());
        } else {
            self.confirming = Some(NativeTransferRequest {
                source,
                target,
                columns: self.pairs.clone(),
            });
        }
    }
}

impl AppState {
    pub(crate) fn choose_transfer(&mut self, side: TransferSide, choice: TransferChoice) {
        let state = &mut self.native_transfer;
        if state.busy() {
            return;
        }
        state.pairs.clear();
        state.error = None;
        state.inserted = None;
        state.schema_diff = None;
        state.schema_error = None;
        state.sync_preview = None;
        state.sync_error = None;
        let entry = state.side_mut(side);
        let epoch = entry.epoch.wrapping_add(1);
        *entry = EndpointState {
            active: Some(choice.clone()),
            loading: true,
            epoch,
            ..Default::default()
        };
        if !self.desktop.begin_job(AdapterJob::ReadNativeTransfer {
            epoch,
            side,
            choice,
        }) {
            state.read_completed(
                epoch,
                side,
                Err("Cannot start the SQLite catalog read.".into()),
            );
        }
    }

    pub(crate) fn set_native_tool(&mut self, tool: NativeTool) {
        let state = &mut self.native_transfer;
        if !state.busy() && state.tool != tool {
            // An intervening transfer can change the same table pair.
            state.sync_preview = None;
            state.sync_error = None;
            state.tool = tool;
        }
    }

    pub(crate) fn compare_native_schema(&mut self) {
        let state = &mut self.native_transfer;
        if state.busy() || state.source.loading || state.target.loading {
            return;
        }
        state.schema_diff = None;
        state.schema_error = None;
        let request = match state.comparison_request() {
            Ok(request) => request,
            Err(error) => {
                state.schema_error = Some(error);
                return;
            }
        };
        state.compare_epoch = state.compare_epoch.wrapping_add(1);
        state.comparing_schema = true;
        if !self.desktop.begin_job(AdapterJob::CompareNativeSchema {
            epoch: state.compare_epoch,
            request,
        }) {
            state.schema_completed(
                state.compare_epoch,
                Err("Cannot start the schema comparison.".into()),
            );
        }
    }

    pub(crate) fn preview_native_sync(&mut self) {
        let state = &mut self.native_transfer;
        if state.busy() || state.source.loading || state.target.loading {
            return;
        }
        state.sync_preview = None;
        state.sync_error = None;
        let request = match state.comparison_request() {
            Ok(request) => request,
            Err(error) => {
                state.sync_error = Some(error);
                return;
            }
        };
        state.compare_epoch = state.compare_epoch.wrapping_add(1);
        state.previewing_sync = true;
        if !self.desktop.begin_job(AdapterJob::PreviewNativeSync {
            epoch: state.compare_epoch,
            request,
        }) {
            state.sync_completed(
                state.compare_epoch,
                Err("Cannot start the data sync preview.".into()),
            );
        }
    }

    pub(crate) fn start_transfer(&mut self) {
        let state = &mut self.native_transfer;
        let Some(request) = state.confirming.take() else {
            return;
        };
        state.epoch = state.epoch.wrapping_add(1);
        state.running = true;
        if !self.desktop.begin_job(AdapterJob::RunNativeTransfer {
            epoch: state.epoch,
            request,
        }) {
            state.run_completed(state.epoch, Err("Cannot start the transfer worker.".into()));
        }
    }
}
