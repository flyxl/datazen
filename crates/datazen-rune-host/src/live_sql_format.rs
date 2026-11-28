//! Native adapter boundary for the optional local SQL formatter. Only live
//! session metadata and the authoritative settings cache are read here.
//! Draft text is handed to the dedicated compute worker, never a DB lane.

use std::sync::{Mutex, MutexGuard, TryLockError};

use datazen_driver_api::{DatabaseDriver, DriverCategory};
use datazen_ui_api::{
    native_sql_format::{
        options_from_settings, shortcut_from_settings, trim_like_react, NativeSqlFormatContext,
        NativeSqlFormatDelivery, NativeSqlFormatError, NativeSqlFormatLanguage, NativeSqlFormatRequest,
        NATIVE_SQL_FORMAT_MAX_INPUT_BYTES,
    },
    NativeQueryDatabase,
};

use crate::native_sql_format::{supports_language, worker::FormatterWorker};
use super::LiveAdapter;

pub(super) struct LiveSqlFormatter {
    worker: FormatterWorker,
    accepted: Option<Accepted>,
}

struct Accepted {
    generation: u64,
    target: NativeQueryDatabase,
    admitted: Snapshot,
}

#[derive(PartialEq, Eq)]
struct Snapshot {
    context: NativeSqlFormatContext,
    // Still reject a reopened connection with the same public generation,
    // profile and database. Opaque driver handles never cross into the UI.
    handle_id: String,
    pool_id: String,
}

enum SnapshotUnavailable {
    Busy,
    Invalid,
}

fn cache_now<T>(slot: &Mutex<T>) -> Result<MutexGuard<'_, T>, SnapshotUnavailable> {
    slot.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => SnapshotUnavailable::Busy,
        TryLockError::Poisoned(_) => SnapshotUnavailable::Invalid,
    })
}

pub(super) fn language_for_sql_driver(driver: &dyn DatabaseDriver) -> Option<NativeSqlFormatLanguage> {
    if driver.driver_category() != DriverCategory::Sql {
        return None;
    }
    // Stock linked aliases reuse their actual family (QuestDB/Cloudberry,
    // Doris/StarRocks/Manticore/OceanBase). The direct frontend language
    // wins first, so MariaDB does not fall back to shared MySQL wire grammar.
    Some(NativeSqlFormatLanguage::for_driver(&driver.driver_type(), Some(&driver.sync_family())))
}

// The API helper merges known options. Validate their cheap outer shape
// before any cloning, so an invalid nested/huge setting never burdens poll.
fn format_settings_shape_supported(document: &serde_json::Value) -> bool {
    if !document.is_object() {
        return false;
    }
    let Some(format) = document.get("sqlFormatOptions").filter(|v| !v.is_null()) else { return true };
    let Some(format) = format.as_object() else { return false };
    ["keywordCase", "indentStyle"].into_iter().all(|name| {
        format.get(name).is_none_or(|value| value.as_str().is_some_and(|text| text.len() <= 16))
    }) && format.get("breakBeforeBooleanOperators").is_none_or(serde_json::Value::is_boolean)
        && format.get("linesBetweenQueries").is_none_or(serde_json::Value::is_number)
}

pub(super) fn effective_shortcut(settings: &serde_json::Value) -> Option<String> {
    const MAX_CHORD_BYTES: usize = 64;
    let large_custom = settings.get("customKeymap").and_then(|map| map.get("formatSql"))
        .and_then(serde_json::Value::as_str).is_some_and(|value| value.len() > MAX_CHORD_BYTES);
    if large_custom {
        None // The action remains available on the toolbar.
    } else {
        let value = shortcut_from_settings(settings);
        (!value.is_empty() && value.len() <= MAX_CHORD_BYTES).then_some(value)
    }
}

impl LiveSqlFormatter {
    pub(super) fn launch() -> Option<Self> {
        Some(Self { worker: FormatterWorker::launch().ok()?, accepted: None })
    }

    fn cancel(&mut self, generation: u64) {
        self.worker.cancel(generation);
        if self.accepted.as_ref().is_some_and(|job| job.generation == generation) {
            self.accepted = None;
        }
    }
}

impl LiveAdapter {
    fn format_snapshot(&self, target: Option<&NativeQueryDatabase>) -> Result<Snapshot, SnapshotUnavailable> {
        use SnapshotUnavailable::Invalid;

        let (session_generation, default_target, language, handle_id, pool_id) = {
            let session = cache_now(&self.session)?;
            if session.kv.is_some() || !session.is_open() {
                return Err(Invalid);
            }
            let connection = session.connection.as_ref().filter(|value| !value.is_empty()).ok_or(Invalid)?;
            let database = session.database.as_ref().filter(|value| !value.is_empty()).ok_or(Invalid)?;
            let default = NativeQueryDatabase { connection_id: connection.clone(), database: database.clone() };
            if target.is_some_and(|target| target != &default && !session.allows_query_database(target)) {
                return Err(Invalid);
            }
            let driver = session.driver.as_deref().ok_or(Invalid)?;
            let handle = session.handle.as_ref().ok_or(Invalid)?;
            let language = language_for_sql_driver(driver).ok_or(Invalid)?;
            if !supports_language(language) {
                return Err(Invalid);
            }
            (session.generation, default, language, handle.id.clone(), handle.pool_id.clone())
        };

        // Neither lock survives this method. In particular, neither the
        // connection lock nor the settings lock reaches the compute engine.
        let settings = cache_now(&self.settings_document)?;
        if !format_settings_shape_supported(&settings) {
            return Err(Invalid);
        }
        let options = options_from_settings(&settings).ok_or(Invalid)?;
        let shortcut = effective_shortcut(&settings);
        Ok(Snapshot {
            context: NativeSqlFormatContext { session_generation, default_target, language, options, shortcut },
            handle_id,
            pool_id,
        })
    }

    pub(super) fn local_format_context(&self) -> Option<NativeSqlFormatContext> {
        self.native_sql_formatter.as_ref()?;
        Some(self.format_snapshot(None).ok()?.context)
    }

    pub(super) fn begin_local_format(
        &mut self,
        request: NativeSqlFormatRequest,
    ) -> Result<(), NativeSqlFormatError> {
        use NativeSqlFormatError::{Cancelled, Limit, Rejected, Unavailable};
        if self.native_sql_formatter.is_none() {
            return Err(Unavailable);
        }
        if request.sql.len() > NATIVE_SQL_FORMAT_MAX_INPUT_BYTES {
            return Err(Limit);
        }
        if request.sql.is_empty()
            || request.sql.contains('\r')
            || trim_like_react(&request.sql).len() != request.sql.len()
        {
            return Err(Rejected);
        }

        let admitted = self.format_snapshot(Some(&request.target)).map_err(|error| match error {
            SnapshotUnavailable::Busy => Unavailable,
            SnapshotUnavailable::Invalid => Cancelled,
        })?;
        let options = admitted.context.engine_options();
        // Audience-selected namespace and current persisted preferences both
        // have to match. Do not trust a request to choose its own dialect.
        if request.session_generation != admitted.context.session_generation || request.options != options {
            return Err(Cancelled);
        }
        let format = self.native_sql_formatter.as_mut().ok_or(Unavailable)?;
        match format.worker.submit(request.generation, request.sql, options) {
            Ok(()) => {
                format.accepted = Some(Accepted { generation: request.generation, target: request.target, admitted });
                Ok(())
            }
            Err(Unavailable) => {
                // submit only returns Unavailable when the worker cannot
                // accept any more (not the provider's per-request error).
                self.native_sql_formatter = None;
                Err(Unavailable)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn poll_local_format(&mut self) -> Option<NativeSqlFormatDelivery> {
        let accepted = self.native_sql_formatter.as_ref()?.accepted.as_ref()?;
        let generation = accepted.generation;
        let current = match self.format_snapshot(Some(&accepted.target)) {
            Ok(snapshot) => snapshot == accepted.admitted,
            Err(SnapshotUnavailable::Busy) => return None,
            Err(SnapshotUnavailable::Invalid) => false,
        };
        let format = self.native_sql_formatter.as_mut()?;
        if !current {
            format.cancel(generation);
            return Some(NativeSqlFormatDelivery { generation, result: Err(NativeSqlFormatError::Cancelled) });
        }
        let done = format.worker.poll().filter(|done| done.generation == generation)?;
        format.accepted = None;
        Some(done)
    }

    pub(super) fn cancel_local_format(&mut self, generation: u64) {
        if let Some(format) = self.native_sql_formatter.as_mut() {
            format.cancel(generation);
        }
    }
}
