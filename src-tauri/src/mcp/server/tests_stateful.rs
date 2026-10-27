    // Stateful session tests. They live in their own file because
    // server/tests_integration.rs is at the per-file size cap.
    use crate::mcp::session::DEFAULT_TTL;
    use crate::testing::app_state::TestAppState;
    use datazen_modules::application::identity_policy::IdentityPolicy;
    use datazen_modules::shared::context::OwnerRef;
    use datazen_modules::execution::SessionHandle;
    use datazen_modules::shared::id::{
        ClientInstanceId, Counter, DbSessionId, JobId, RuntimeEpoch, StageId,
    };
    use datazen_modules::shared::error::ApiErrorCode;
    use rmcp::handler::server::wrapper::Parameters;
    use std::time::{Duration, Instant};

    /// Two MCP server instances over one app state: the shape a stdio process
    /// and a second consumer see. Each one owns a separate backend-generated
    /// client instance id and a separate session registry.
    async fn two_mcp_clients() -> (DataZenMcpServer, DataZenMcpServer) {
        let test = TestAppState::new().await;
        let state = Arc::new(test.state);
        (
            mcp_server_allowing(state.clone(), &["mcp-cfg"]),
            mcp_server_allowing(state, &["mcp-cfg"]),
        )
    }

    /// Register a mapping the way open_session would, but around a fabricated
    /// handle: the gateway round trip is covered by the runtime suites, while
    /// ownership, isolation and expiry are decided before any gateway call.
    fn open_mapping(server: &DataZenMcpServer, ttl: Duration) -> String {
        server
            .sessions
            .open(
                server.client_instance_id(),
                "mcp-cfg",
                SessionHandle::new(DbSessionId::new("mcp-session"), RuntimeEpoch::new("mcp-epoch")),
                Counter::new(1),
                ttl,
                Instant::now(),
            )
            .reference
    }

    fn use_reference(reference: &str) -> Parameters<UseSessionInput> {
        Parameters(UseSessionInput {
            client_session_ref: reference.to_owned(),
            command: "list_tables".to_owned(),
            input: serde_json::json!({}),
            timeout_ms: None,
        })
    }

    fn close_reference(reference: &str) -> Parameters<CloseSessionInput> {
        Parameters(CloseSessionInput {
            client_session_ref: reference.to_owned(),
            mode: None,
        })
    }

    #[tokio::test]
    async fn a_reference_opened_by_one_consumer_is_unusable_by_another() {
        let (first, second) = two_mcp_clients().await;
        assert_ne!(
            first.client_instance_id(),
            second.client_instance_id(),
            "each MCP server instance must be its own client instance"
        );

        let reference = open_mapping(&first, DEFAULT_TTL);

        let use_error = second.use_session(use_reference(&reference)).await.unwrap_err();
        assert!(
            use_error.to_string().contains("SessionLost"),
            "a foreign reference must be reported as lost, got: {use_error}"
        );

        let close_reply = second.close_session(close_reference(&reference)).await.unwrap();
        assert_eq!(
            close_reply,
            r#"{"closed":false}"#,
            "a consumer must not be able to close a mapping it does not own"
        );

        assert!(
            first
                .sessions
                .lookup(&reference, first.client_instance_id(), Instant::now())
                .is_ok(),
            "the owner keeps its mapping after a foreign lookup and close"
        );
        assert_eq!(first.sessions.len(), 1);
        assert!(second.sessions.is_empty());
    }

    #[tokio::test]
    async fn an_unknown_reference_is_reported_as_lost_and_is_never_rebuilt() {
        let (first, _second) = two_mcp_clients().await;

        let error = first
            .use_session(use_reference("never-opened"))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("SessionLost"),
            "got: {error}"
        );
        assert!(
            first.sessions.is_empty(),
            "a lost reference must not be recreated from the request"
        );
    }

    #[tokio::test]
    async fn a_reference_past_its_ttl_is_refused_instead_of_handing_back_a_stale_session() {
        let (first, _second) = two_mcp_clients().await;
        let reference = open_mapping(&first, Duration::from_millis(1));
        tokio::time::sleep(Duration::from_millis(20)).await;

        let error = first.use_session(use_reference(&reference)).await.unwrap_err();
        assert!(
            error.to_string().contains("SessionLost"),
            "got: {error}"
        );
        assert!(
            first.sessions.is_empty(),
            "an expired mapping is dropped instead of being kept for a later caller"
        );
    }

    #[tokio::test]
    async fn a_close_the_gateway_refuses_keeps_the_mapping_so_the_client_can_retry() {
        let (first, _second) = two_mcp_clients().await;
        let reference = open_mapping(&first, DEFAULT_TTL);

        // The fabricated handle is not a session the gateway knows, so the close
        // must fail instead of reporting a session as closed that is still open.
        let outcome = first.close_session(close_reference(&reference)).await;
        assert!(outcome.is_err(), "an unknown runtime handle must not close");
        assert_eq!(
            first.sessions.len(),
            1,
            "the mapping survives a refused close so close_session stays retryable"
        );
    }

    #[tokio::test]
    async fn closing_a_reference_twice_reports_nothing_left_to_close() {
        let (first, _second) = two_mcp_clients().await;
        let reference = open_mapping(&first, DEFAULT_TTL);
        first.sessions.close(&reference, first.client_instance_id());

        let reply = first.close_session(close_reference(&reference)).await.unwrap();
        assert_eq!(
            reply,
            r#"{"closed":false}"#,
            "close is idempotent: the second call reports nothing was closed"
        );
    }

    #[tokio::test]
    async fn a_disconnected_consumer_releases_only_its_own_mappings() {
        let (first, second) = two_mcp_clients().await;
        let mine = open_mapping(&first, DEFAULT_TTL);
        let theirs = open_mapping(&second, DEFAULT_TTL);

        first.release_sessions().await;

        assert!(first.sessions.is_empty());
        assert!(
            first
                .sessions
                .lookup(&mine, first.client_instance_id(), Instant::now())
                .is_err(),
            "a released mapping is gone even while the other consumer shares the connection"
        );
        assert!(
            second
                .sessions
                .lookup(&theirs, second.client_instance_id(), Instant::now())
                .is_ok(),
            "another consumer on the same connection keeps its own session"
        );
    }

    #[tokio::test]
    async fn the_stateful_owner_is_this_servers_backend_generated_client_instance() {
        let (first, _second) = two_mcp_clients().await;
        let adapter = first
            .app_state
            .platform_require()
            .expect("the test app state exposes the platform adapter");
        let context = first.request_context(adapter.identity());
        assert_eq!(
            context.client_instance_id,
            ClientInstanceId::new(first.client_instance_id().to_owned()),
            "the call context carries this server's own id, not the desktop launch id"
        );
        assert_eq!(
            context.organization_id,
            *adapter.identity().organization_id()
        );
        assert_eq!(context.principal_id, *adapter.identity().principal_id());

        let own = OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new(first.client_instance_id().to_owned()),
            purpose: MCP_SESSION_PURPOSE.to_owned(),
        };
        assert!(
            IdentityPolicy::check_owner(&context, &own, None).is_ok(),
            "the consumer that owns the mapping may use it"
        );

        let foreign = OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("another-mcp-consumer".to_owned()),
            purpose: MCP_SESSION_PURPOSE.to_owned(),
        };
        let error = IdentityPolicy::check_owner(&context, &foreign, None).unwrap_err();
        assert_eq!(error.code, ApiErrorCode::PermissionDenied);

        let claimed_job = OwnerRef::Job {
            job_id: JobId::new("job-from-a-request-body"),
            stage_id: StageId::new("stage-1"),
        };
        let error = IdentityPolicy::check_owner(&context, &claimed_job, None).unwrap_err();
        assert_eq!(
            error.code,
            ApiErrorCode::PermissionDenied,
            "a consumer cannot claim a job owner for its session"
        );
    }
