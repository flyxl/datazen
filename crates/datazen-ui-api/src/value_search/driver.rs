use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::CellType;

use super::selection::searchable_columns;
use super::types::{
    Clock, SchemaReader, SearchTableValuesRequest, TableValueSearcher, ValueSearchMode,
    ValueSearchProgress, ValueSearchStatus, ValueSearchTableResult, ValueSearchTarget,
};

pub struct ValueSearchDriverParams {
    pub db_session_id: String,
    pub database: String,
    pub targets: Vec<ValueSearchTarget>,
    pub value: String,
    pub mode: ValueSearchMode,
    pub cell_types: BTreeSet<CellType>,
    pub read_schema: SchemaReader,
    pub search_table: TableValueSearcher,
    pub clock: Clock,
}

type ProgressListener = Rc<RefCell<dyn FnMut(&ValueSearchProgress)>>;

pub struct ValueSearchDriver {
    params: ValueSearchDriverParams,
    listeners: Rc<RefCell<Vec<ProgressListener>>>,
    status: ValueSearchStatus,
    completed: usize,
    results: Vec<ValueSearchTableResult>,
    current: Option<String>,
    next_target: usize,
    token: u64,
    started_at: Option<i64>,
    ended_at: Option<i64>,
}

impl ValueSearchDriver {
    pub fn new(params: ValueSearchDriverParams) -> Self {
        Self {
            params,
            listeners: Rc::new(RefCell::new(Vec::new())),
            status: ValueSearchStatus::Idle,
            completed: 0,
            results: Vec::new(),
            current: None,
            next_target: 0,
            token: 0,
            started_at: None,
            ended_at: None,
        }
    }

    pub fn total(&self) -> usize {
        self.params.targets.len()
    }

    pub fn token(&self) -> u64 {
        self.token
    }

    pub fn status(&self) -> ValueSearchStatus {
        self.status
    }

    pub fn progress(&self) -> ValueSearchProgress {
        let elapsed_ms = match self.started_at {
            Some(started) => {
                let now = match self.ended_at {
                    Some(ended) => ended,
                    None => (self.params.clock)(),
                };
                Some(now - started)
            }
            None => None,
        };
        ValueSearchProgress {
            status: self.status,
            completed: self.completed,
            total: self.total(),
            current: self.current.clone(),
            results: self.results.clone(),
            elapsed_ms,
        }
    }

    pub fn subscribe(&mut self, listener: ProgressListener) -> Box<dyn Fn()> {
        self.listeners.borrow_mut().push(listener.clone());
        listener.borrow_mut()(&self.progress());
        let listeners = self.listeners.clone();
        Box::new(move || {
            let mut current = listeners.borrow_mut();
            if let Some(index) = current.iter().position(|item| Rc::ptr_eq(item, &listener)) {
                current.remove(index);
            }
        })
    }

    fn emit(&self) {
        let listeners = self.listeners.borrow().clone();
        let progress = self.progress();
        for listener in listeners {
            listener.borrow_mut()(&progress);
        }
    }

    pub fn start(&mut self) {
        if self.status == ValueSearchStatus::Running || self.status == ValueSearchStatus::Done {
            return;
        }
        if self.status == ValueSearchStatus::Paused {
            self.status = ValueSearchStatus::Running;
            self.emit();
            return;
        }
        self.status = ValueSearchStatus::Running;
        self.started_at = Some((self.params.clock)());
        self.ended_at = None;
        self.emit();
        self.token += 1;
    }

    pub fn pause(&mut self) {
        if self.status != ValueSearchStatus::Running {
            return;
        }
        self.status = ValueSearchStatus::Paused;
        self.emit();
    }

    pub fn cancel(&mut self) {
        if self.status == ValueSearchStatus::Done || self.status == ValueSearchStatus::Cancelled {
            return;
        }
        self.status = if self.status == ValueSearchStatus::Idle {
            ValueSearchStatus::Idle
        } else {
            ValueSearchStatus::Cancelled
        };
        self.current = None;
        if self.started_at.is_some() {
            self.ended_at = Some((self.params.clock)());
        }
        self.emit();
    }

    fn finish(&mut self) {
        self.status = ValueSearchStatus::Done;
        self.ended_at = Some((self.params.clock)());
        self.emit();
    }

    pub fn advance_one(&mut self) -> bool {
        let token = self.token;
        self.run_step_for_token(token)
    }

    pub fn run_step_for_token(&mut self, token: u64) -> bool {
        if token != self.token
            || self.status == ValueSearchStatus::Cancelled
            || self.status == ValueSearchStatus::Done
        {
            return false;
        }
        if self.status == ValueSearchStatus::Paused {
            return false;
        }
        if self.next_target >= self.params.targets.len() {
            self.finish();
            return false;
        }
        let target = self.params.targets[self.next_target].clone();
        self.current = Some(target.table.clone());
        self.emit();

        let result = match (self.params.read_schema)(&target) {
            Ok(columns) => {
                let names = searchable_columns(&columns, &self.params.cell_types);
                if names.is_empty() {
                    ValueSearchTableResult {
                        table: target.table.clone(),
                        schema: target.schema.clone(),
                        matches: Some(0),
                        columns: Vec::new(),
                        reason: None,
                    }
                } else {
                    let request = SearchTableValuesRequest {
                        db_session_id: self.params.db_session_id.clone(),
                        database: self.params.database.clone(),
                        schema: target.schema.clone(),
                        table: target.table.clone(),
                        columns: names,
                        value: self.params.value.clone(),
                        mode: self.params.mode,
                    };
                    match (self.params.search_table)(&request) {
                        Ok(response) => ValueSearchTableResult {
                            table: response.table,
                            schema: target.schema.clone(),
                            matches: Some(response.matches),
                            columns: response.searched_columns,
                            reason: None,
                        },
                        Err(reason) => ValueSearchTableResult {
                            table: target.table.clone(),
                            schema: target.schema.clone(),
                            matches: None,
                            columns: Vec::new(),
                            reason: Some(reason),
                        },
                    }
                }
            }
            Err(reason) => ValueSearchTableResult {
                table: target.table.clone(),
                schema: target.schema.clone(),
                matches: None,
                columns: Vec::new(),
                reason: Some(reason),
            },
        };

        self.results.push(result);
        self.completed += 1;
        self.next_target += 1;
        self.current = None;
        self.emit();
        true
    }

    pub fn run_to_completion(&mut self, max_steps: usize) -> usize {
        let mut steps = 0usize;
        while steps < max_steps && self.status == ValueSearchStatus::Running && self.advance_one() {
            steps += 1;
        }
        if self.status == ValueSearchStatus::Running && self.next_target >= self.params.targets.len() {
            self.finish();
        }
        steps
    }
}
