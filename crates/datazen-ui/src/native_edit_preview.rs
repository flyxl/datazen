//! Read-only live plan preview and explicit confirmation for one staged cell batch.
use crate::{
    state_def::AppState,
    types::{Section, TableView},
    views::pending_plan_dialog::{ChangeWarning, PlannedStatement, RowChangePlanVm},
};
use datazen_ui_api::{AdapterJob, CellEditBatch, CellEditPreview, CellEditReview};

struct Request {
    nonce: u32,
    generation: u32,
    batch: CellEditBatch,
}

struct Ready {
    request: Request,
    review: CellEditReview,
    dialog: RowChangePlanVm,
}

#[derive(Default)]
pub(crate) struct NativeEditPreview {
    nonce: u32,
    pending: Option<Request>,
    ready: Option<Ready>,
    committing: Option<u32>,
    error: Option<(u32, String)>,
}

impl AppState {
    fn matches_edit_preview(&self, request: &Request) -> bool {
        self.active == Section::Workspace
            && self.table_view == TableView::Data
            && self.read_generation == request.generation
            && self.table_loading.is_none()
            && self.connect_loading.is_none()
            && !self.grid.is_editing()
            && self.selected_table().as_deref() == Some(request.batch.table.as_str())
            && self.grid.commit_batch().as_ref() == Some(&request.batch)
    }

    pub(crate) fn request_cell_preview(&mut self) {
        if self.commit_loading.is_some()
            || self.table_loading.is_some()
            || !self.desktop.supports_native_cell_preview()
        {
            return;
        }
        let Some(batch) = self.grid.commit_batch() else {
            return;
        };
        if self
            .edit_preview
            .pending
            .as_ref()
            .is_some_and(|p| self.matches_edit_preview(p))
        {
            return;
        }
        self.edit_preview.nonce = self.edit_preview.nonce.wrapping_add(1);
        let request = Request {
            nonce: self.edit_preview.nonce,
            generation: self.read_generation,
            batch,
        };
        self.edit_preview.ready = None;
        self.edit_preview.error = None;
        if self.desktop.begin_job(AdapterJob::PreviewCells {
            nonce: request.nonce,
            batch: request.batch.clone(),
        }) {
            self.edit_preview.pending = Some(request);
        } else {
            self.edit_preview.pending = None;
            self.edit_preview.error = Some((
                request.generation,
                "Could not start the edit preview.".into(),
            ));
        }
    }

    pub(crate) fn deliver_cell_preview(
        &mut self,
        nonce: u32,
        result: Result<CellEditPreview, String>,
    ) {
        if self
            .edit_preview
            .pending
            .as_ref()
            .is_none_or(|p| p.nonce != nonce)
        {
            return;
        }
        let Some(request) = self.edit_preview.pending.take() else {
            return;
        };
        if !self.matches_edit_preview(&request) || self.commit_loading.is_some() {
            return;
        }
        let preview = match result {
            Ok(preview) => preview,
            Err(error) => {
                self.edit_preview.error = Some((request.generation, error));
                return;
            }
        };
        let dialog = RowChangePlanVm {
            fingerprint: preview.review.fingerprint.clone(),
            table: request.batch.table.clone(),
            updates: preview
                .updates
                .into_iter()
                .map(|(sql_template, parameter_summary)| PlannedStatement {
                    sql_template,
                    parameter_summary,
                })
                .collect(),
            warnings: preview
                .warnings
                .into_iter()
                .map(|(code, message)| ChangeWarning { code, message })
                .collect(),
            ..Default::default()
        };
        self.edit_preview.ready = Some(Ready {
            request,
            review: preview.review,
            dialog,
        });
    }

    pub(crate) fn active_cell_preview(&self) -> Option<&RowChangePlanVm> {
        let ready = self.edit_preview.ready.as_ref()?;
        (self.commit_loading.is_none() && self.matches_edit_preview(&ready.request))
            .then_some(&ready.dialog)
    }

    pub(crate) fn active_cell_preview_error(&self) -> Option<&str> {
        self.edit_preview
            .error
            .as_ref()
            .and_then(|(generation, error)| {
                (*generation == self.read_generation).then_some(error.as_str())
            })
    }

    pub(crate) fn close_cell_preview(&mut self) {
        self.edit_preview.ready = None;
    }

    pub(crate) fn commit_cell_preview(&mut self) {
        let Some(ready) = self.edit_preview.ready.take() else {
            return;
        };
        if !self.matches_edit_preview(&ready.request)
            || self.commit_loading.is_some()
            || ready.dialog.updates.is_empty()
        {
            return;
        }
        let generation = ready.request.generation;
        if self.desktop.begin_job(AdapterJob::CommitReviewedCells {
            generation,
            batch: ready.request.batch,
            review: ready.review,
        }) {
            self.commit_loading = Some(generation);
            self.edit_preview.committing = Some(generation);
        } else {
            self.edit_preview.error =
                Some((generation, "Could not start the reviewed commit.".into()));
        }
    }

    pub(crate) fn finish_reviewed_commit(&mut self, generation: u32, count: &Result<u32, String>) {
        if self.edit_preview.committing != Some(generation) {
            return;
        }
        self.edit_preview.committing = None;
        if let Err(error) = count {
            self.edit_preview.error = Some((generation, error.clone()));
        }
    }
}
