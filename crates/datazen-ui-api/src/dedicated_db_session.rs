#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedicatedSideSession {
    pub connection_id: String,
    pub database: String,
    pub db_session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedicatedConnectResult {
    pub db_session_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListDatabasesResult {
    pub databases: Vec<String>,
    pub db_session_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseOverride<'a> {
    Default,
    Null,
    Value(&'a str),
}

impl DatabaseOverride<'_> {
    fn connect_arg(self, database: &str) -> Option<String> {
        match self {
            DatabaseOverride::Default => Some(database.to_owned()),
            DatabaseOverride::Null => None,
            DatabaseOverride::Value(value) => Some(value.to_owned()),
        }
    }
}

pub struct DedicatedSessionPorts {
    pub connect_dedicated: Box<dyn FnMut(&str, Option<&str>) -> Result<DedicatedConnectResult, String>>,
    pub release_connection: Box<dyn FnMut(&str) -> Result<(), String>>,
    pub ping_connection: Box<dyn FnMut(&str) -> Result<bool, String>>,
    pub get_databases: Box<dyn FnMut(&str, &str) -> Result<Vec<String>, String>>,
}

pub struct DedicatedSessionService {
    ports: DedicatedSessionPorts,
}

impl DedicatedSessionService {
    pub fn new(ports: DedicatedSessionPorts) -> Self {
        Self { ports }
    }

    pub fn release_dedicated_session(&mut self, db_session_id: Option<&str>) {
        let Some(db_session_id) = db_session_id.filter(|id| !id.is_empty()) else {
            return;
        };
        let _ = (self.ports.release_connection)(db_session_id);
    }

    pub fn ensure_dedicated_session(
        &mut self,
        current: Option<&DedicatedSideSession>,
        connection_id: &str,
        database: &str,
        database_override: DatabaseOverride<'_>,
    ) -> Result<Option<DedicatedSideSession>, String> {
        if connection_id.is_empty() || database.is_empty() {
            return Ok(None);
        }
        if let Some(current) = current {
            if current.connection_id == connection_id
                && current.database == database
                && !current.db_session_id.is_empty()
            {
                let alive = (self.ports.ping_connection)(&current.db_session_id).unwrap_or(false);
                if alive {
                    return Ok(Some(current.clone()));
                }
            }
        }
        self.release_dedicated_session(current.map(|value| value.db_session_id.as_str()));
        let override_arg = database_override.connect_arg(database);
        let connected = (self.ports.connect_dedicated)(connection_id, override_arg.as_deref())?;
        Ok(Some(DedicatedSideSession {
            connection_id: connection_id.to_owned(),
            database: database.to_owned(),
            db_session_id: connected.db_session_id,
        }))
    }

    pub fn ensure_dedicated_session_for_effect(
        &mut self,
        current: Option<&DedicatedSideSession>,
        connection_id: &str,
        database: &str,
        is_current: &dyn Fn() -> bool,
        database_override: DatabaseOverride<'_>,
    ) -> Result<Option<DedicatedSideSession>, String> {
        let next = self.ensure_dedicated_session(current, connection_id, database, database_override)?;
        if is_current() {
            return Ok(next);
        }
        let next_id = next.as_ref().map(|value| value.db_session_id.as_str());
        let current_id = current.map(|value| value.db_session_id.as_str());
        if let Some(id) = next_id {
            if Some(id) != current_id {
                self.release_dedicated_session(Some(id));
            }
        }
        Ok(None)
    }

    pub fn list_databases_dedicated(
        &mut self,
        connection_id: &str,
        preferred_database: Option<&str>,
    ) -> Result<ListDatabasesResult, String> {
        let preferred = preferred_database.filter(|value| !value.is_empty());
        let connected = (self.ports.connect_dedicated)(connection_id, preferred)?;
        let session_id = connected.db_session_id;
        let result = (self.ports.get_databases)(&session_id, connection_id);
        self.release_dedicated_session(Some(&session_id));
        let databases = result?;
        Ok(ListDatabasesResult { databases, db_session_id: None })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Default)]
    struct Log {
        calls: RefCell<Vec<String>>,
        connect_results: RefCell<Vec<String>>,
        ping_results: RefCell<Vec<bool>>,
        ping_errors: RefCell<bool>,
        release_errors: RefCell<bool>,
        database_errors: RefCell<bool>,
    }

    impl Log {
        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    fn new_service(log: &Rc<Log>) -> DedicatedSessionService {
        let connect_log = log.clone();
        let release_log = log.clone();
        let ping_log = log.clone();
        let database_log = log.clone();
        DedicatedSessionService::new(DedicatedSessionPorts {
            connect_dedicated: Box::new(move |connection_id, database| {
                connect_log.calls.borrow_mut().push(format!("connect:{connection_id}:{}", database.unwrap_or("<none>")));
                let mut results = connect_log.connect_results.borrow_mut();
                let id = if results.is_empty() {
                    "new-session".to_owned()
                } else {
                    results.remove(0)
                };
                Ok(DedicatedConnectResult { db_session_id: id })
            }),
            release_connection: Box::new(move |id| {
                release_log.calls.borrow_mut().push(format!("release:{id}"));
                if *release_log.release_errors.borrow() {
                    return Err("release failed".to_owned());
                }
                Ok(())
            }),
            ping_connection: Box::new(move |id| {
                ping_log.calls.borrow_mut().push(format!("ping:{id}"));
                if *ping_log.ping_errors.borrow() {
                    return Err("ping failed".to_owned());
                }
                let mut results = ping_log.ping_results.borrow_mut();
                Ok(if results.is_empty() { true } else { results.remove(0) })
            }),
            get_databases: Box::new(move |session_id, connection_id| {
                database_log.calls.borrow_mut().push(format!("databases:{session_id}:{connection_id}"));
                if *database_log.database_errors.borrow() {
                    return Err("databases failed".to_owned());
                }
                Ok(vec!["db1".to_owned(), "db2".to_owned()])
            }),
        })
    }

    fn session(id: &str) -> DedicatedSideSession {
        DedicatedSideSession {
            connection_id: "c1".to_owned(),
            database: "main".to_owned(),
            db_session_id: id.to_owned(),
        }
    }

    #[test]
    fn release_skips_empty_ids_and_swallows_errors() {
        let log = Rc::new(Log::default());
        let mut service = new_service(&log);
        service.release_dedicated_session(None);
        service.release_dedicated_session(Some(""));
        assert!(log.calls().is_empty());
        *log.release_errors.borrow_mut() = true;
        service.release_dedicated_session(Some("s1"));
        assert_eq!(log.calls(), vec!["release:s1".to_owned()]);
    }

    #[test]
    fn missing_connection_or_database_returns_null_without_calls() {
        let log = Rc::new(Log::default());
        let mut service = new_service(&log);
        assert_eq!(
            service.ensure_dedicated_session(None, "", "main", DatabaseOverride::Default).expect("ok"),
            None
        );
        assert_eq!(
            service.ensure_dedicated_session(None, "c1", "", DatabaseOverride::Default).expect("ok"),
            None
        );
        assert!(log.calls().is_empty());
    }

    #[test]
    fn matching_live_session_is_reused_without_reconnect() {
        let log = Rc::new(Log::default());
        let mut service = new_service(&log);
        let current = session("s1");
        let next = service
            .ensure_dedicated_session(Some(&current), "c1", "main", DatabaseOverride::Default)
            .expect("ok");
        assert_eq!(next, Some(current));
        assert_eq!(log.calls(), vec!["ping:s1".to_owned()]);
    }

    #[test]
    fn dead_failed_or_stale_sessions_reconnect_after_release() {
        for fail_ping in [false, true] {
            let log = Rc::new(Log::default());
            log.ping_results.borrow_mut().push(false);
            if fail_ping {
                *log.ping_errors.borrow_mut() = true;
            }
            log.connect_results.borrow_mut().push("s2".to_owned());
            let mut service = new_service(&log);
            let current = session("s1");
            let next = service
                .ensure_dedicated_session(Some(&current), "c1", "main", DatabaseOverride::Default)
                .expect("ok");
            let next = next.expect("a session");
            assert_eq!(next.db_session_id, "s2");
            assert_eq!(next.connection_id, "c1");
            assert_eq!(next.database, "main");
            assert_eq!(log.calls(), vec!["ping:s1".to_owned(), "release:s1".to_owned(), "connect:c1:main".to_owned()]);
        }
    }

    #[test]
    fn changed_identity_skips_ping_and_releases_the_old_session() {
        let log = Rc::new(Log::default());
        log.connect_results.borrow_mut().push("s2".to_owned());
        let mut service = new_service(&log);
        let current = session("s1");
        let next = service
            .ensure_dedicated_session(Some(&current), "c1", "other", DatabaseOverride::Default)
            .expect("ok");
        assert_eq!(next.expect("session").database, "other");
        assert_eq!(log.calls(), vec!["release:s1".to_owned(), "connect:c1:other".to_owned()]);
    }

    #[test]
    fn override_variants_follow_nullish_default_not_truthiness() {
        let log = Rc::new(Log::default());
        log.connect_results.borrow_mut().push("a".to_owned());
        log.connect_results.borrow_mut().push("b".to_owned());
        log.connect_results.borrow_mut().push("c".to_owned());
        let mut service = new_service(&log);
        service.ensure_dedicated_session(None, "c1", "main", DatabaseOverride::Default).expect("ok");
        service.ensure_dedicated_session(None, "c1", "main", DatabaseOverride::Null).expect("ok");
        service.ensure_dedicated_session(None, "c1", "main", DatabaseOverride::Value("")).expect("ok");
        assert_eq!(
            log.calls(),
            vec![
                "connect:c1:main".to_owned(),
                "connect:c1:<none>".to_owned(),
                "connect:c1:".to_owned(),
            ]
        );
    }

    #[test]
    fn connect_errors_propagate_and_no_session_is_returned() {
        let log = Rc::new(Log::default());
        let mut service = DedicatedSessionService::new(DedicatedSessionPorts {
            connect_dedicated: Box::new(|_, _| Err("connect failed".to_owned())),
            release_connection: Box::new(|_| Ok(())),
            ping_connection: Box::new(|_| Ok(false)),
            get_databases: Box::new(|_, _| Ok(Vec::new())),
        });
        assert_eq!(
            service.ensure_dedicated_session(None, "c1", "main", DatabaseOverride::Default).unwrap_err(),
            "connect failed"
        );
        let _ = log;
    }

    #[test]
    fn effect_releases_a_new_session_when_stale_but_not_a_reused_one() {
        let log = Rc::new(Log::default());
        log.ping_results.borrow_mut().push(false);
        log.ping_results.borrow_mut().push(false);
        log.connect_results.borrow_mut().push("s2".to_owned());
        log.connect_results.borrow_mut().push("s1".to_owned());
        let mut service = new_service(&log);
        let current = session("s1");

        let stale = service
            .ensure_dedicated_session_for_effect(Some(&current), "c1", "main", &|| false, DatabaseOverride::Default)
            .expect("ok");
        assert_eq!(stale, None);
        assert_eq!(log.calls(), vec!["ping:s1".to_owned(), "release:s1".to_owned(), "connect:c1:main".to_owned(), "release:s2".to_owned()]);

        log.calls.borrow_mut().clear();
        let reused = service
            .ensure_dedicated_session_for_effect(Some(&current), "c1", "main", &|| false, DatabaseOverride::Default)
            .expect("ok");
        assert_eq!(reused, None);
        assert_eq!(log.calls(), vec!["ping:s1".to_owned(), "release:s1".to_owned(), "connect:c1:main".to_owned()], "only ensure releases the old session; the effect adds no second release");

        log.calls.borrow_mut().clear();
        let live = service
            .ensure_dedicated_session_for_effect(Some(&current), "c1", "main", &|| true, DatabaseOverride::Default)
            .expect("ok");
        assert_eq!(live, Some(current));
        assert_eq!(log.calls(), vec!["ping:s1".to_owned()]);
    }

    #[test]
    fn list_databases_uses_a_short_lived_session_and_releases_on_both_paths() {
        let log = Rc::new(Log::default());
        log.connect_results.borrow_mut().push("tmp".to_owned());
        let mut service = new_service(&log);
        let result = service.list_databases_dedicated("c1", Some("preferred")).expect("ok");
        assert_eq!(result.databases, vec!["db1".to_owned(), "db2".to_owned()]);
        assert_eq!(result.db_session_id, None);
        assert_eq!(
            log.calls(),
            vec![
                "connect:c1:preferred".to_owned(),
                "databases:tmp:c1".to_owned(),
                "release:tmp".to_owned(),
            ]
        );

        log.calls.borrow_mut().clear();
        log.connect_results.borrow_mut().push("tmp2".to_owned());
        *log.database_errors.borrow_mut() = true;
        let mut service = new_service(&log);
        assert_eq!(service.list_databases_dedicated("c1", None).unwrap_err(), "databases failed");
        assert_eq!(
            log.calls(),
            vec!["connect:c1:<none>".to_owned(), "databases:tmp2:c1".to_owned(), "release:tmp2".to_owned()],
            "release still runs when listing fails"
        );
    }

    #[test]
    fn list_databases_preferred_empty_string_is_absent() {
        let log = Rc::new(Log::default());
        log.connect_results.borrow_mut().push("tmp".to_owned());
        let mut service = new_service(&log);
        service.list_databases_dedicated("c1", Some("")).expect("ok");
        assert_eq!(log.calls()[0], "connect:c1:<none>".to_owned());
    }

    #[test]
    fn list_databases_connect_failure_does_not_release() {
        let mut service = DedicatedSessionService::new(DedicatedSessionPorts {
            connect_dedicated: Box::new(|_, _| Err("connect failed".to_owned())),
            release_connection: Box::new(|_| Ok(())),
            ping_connection: Box::new(|_| Ok(false)),
            get_databases: Box::new(|_, _| Ok(Vec::new())),
        });
        assert_eq!(
            service.list_databases_dedicated("c1", None).unwrap_err(),
            "connect failed"
        );
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _: Option<crate::DedicatedSideSession> = None;
        let _: Option<crate::DedicatedConnectResult> = None;
        let _: Option<crate::ListDatabasesResult> = None;
        let _: Option<crate::DedicatedSessionPorts> = None;
        let _: Option<crate::DedicatedSessionService> = None;
        let _: Option<crate::DatabaseOverride<'static>> = None;
    }
}
