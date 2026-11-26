//! UI ownership of a single reviewed CSV request; the worker owns all I/O.
use crate::state_def::AppState;
use datazen_ui_api::native_csv_import::{CsvImportChoice, CsvImportReview};
use datazen_ui_api::AdapterJob;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Review,
    Append,
}

#[derive(Default)]
pub(crate) struct NativeCsvState {
    pub profile: Option<String>,
    pub review: Option<CsvImportReview>,
    pub inserted: Option<usize>,
    pending: Option<Pending>,
    epoch: u32,
}

impl NativeCsvState {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

impl AppState {
    pub(crate) fn pick_csv_profile(&mut self, id: &str) {
        if self.import_step == 1 && !self.native_csv.busy() {
            self.native_csv.profile = Some(id.to_owned());
            self.import_error = None;
        }
    }

    pub(crate) fn start_csv_review(&mut self) {
        if self.import_step != 1 || self.native_csv.busy() {
            return;
        }
        let Some(connection_id) = self.native_csv.profile.clone() else {
            self.import_error = Some("Choose a saved SQLite target connection.".into());
            return;
        };
        let choice = CsvImportChoice {
            connection_id,
            source_path: self.import_source.text.trim().to_owned(),
            table: self.import_target.text.trim().to_owned(),
        };
        self.native_csv.epoch = self.native_csv.epoch.wrapping_add(1);
        self.native_csv.pending = Some(Pending::Review);
        self.native_csv.review = None;
        self.import_step = 2;
        self.import_error = None;
        let epoch = self.native_csv.epoch;
        if !self
            .desktop
            .begin_job(AdapterJob::ReviewNativeCsv { epoch, choice })
        {
            self.csv_reviewed(epoch, Err("Cannot start the CSV preview.".into()));
        }
    }

    pub(crate) fn csv_reviewed(&mut self, epoch: u32, result: Result<CsvImportReview, String>) {
        if self.native_csv.epoch != epoch || self.native_csv.pending != Some(Pending::Review) {
            return;
        }
        self.native_csv.pending = None;
        match result {
            Ok(review) => self.native_csv.review = Some(review),
            Err(error) => {
                self.import_error = Some(error);
                self.import_step = 1;
            }
        }
    }

    pub(crate) fn cancel_csv_review(&mut self) {
        if !self.native_csv.busy() && self.import_step == 2 {
            self.native_csv.review = None;
            self.import_error = None;
            self.import_step = 1;
        }
    }

    pub(crate) fn append_reviewed_csv(&mut self) {
        if self.native_csv.busy() || self.import_step != 2 {
            return;
        }
        let Some(review) = self.native_csv.review.take() else {
            return;
        };
        self.native_csv.epoch = self.native_csv.epoch.wrapping_add(1);
        self.native_csv.pending = Some(Pending::Append);
        let epoch = self.native_csv.epoch;
        if !self
            .desktop
            .begin_job(AdapterJob::AppendNativeCsv { epoch, review })
        {
            self.csv_appended(epoch, Err("Cannot start the CSV import.".into()));
        }
    }

    pub(crate) fn csv_appended(&mut self, epoch: u32, result: Result<usize, String>) {
        if self.native_csv.epoch != epoch || self.native_csv.pending != Some(Pending::Append) {
            return;
        }
        self.native_csv.pending = None;
        match result {
            Ok(inserted) => {
                self.native_csv.inserted = Some(inserted);
                self.import_step = 3;
            }
            Err(error) => {
                self.import_step = 1;
                self.import_error = Some(error);
            }
        }
    }

    pub(crate) fn restart_csv_import(&mut self) {
        if !self.native_csv.busy() {
            self.native_csv.inserted = None;
            self.native_csv.profile = None;
            self.import_source = rune::TextEdit::new(String::new());
            self.import_target = rune::TextEdit::new(String::new());
            self.import_error = None;
            self.import_step = 0;
        }
    }
}
