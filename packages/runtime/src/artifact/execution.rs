//! Internal scoped storage for results of an already-admitted delegated execution.
//! Public ArtifactStore operations keep refusing delegated create/read/export.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ExecutionArtifactScope {
    organization: OrganizationId,
    principal: PrincipalId,
    client: ClientInstanceId,
    authentication: Option<AuthenticationSessionId>,
    delegation: DelegationId,
    execution: ExecutionId,
}
impl ExecutionArtifactScope {
    /// Caller must hold the Runtime's admitted execution entry. This capability
    /// is crate-private, process-local and never comes from a transport DTO.
    pub(crate) fn admitted(ctx: &RequestContext, execution: ExecutionId) -> Option<Self> {
        Some(Self {
            organization: ctx.organization_id.clone(),
            principal: ctx.principal_id.clone(),
            client: ctx.client_instance_id.clone(),
            authentication: ctx.authentication_session_id.clone(),
            delegation: ctx.delegation_id.clone()?,
            execution,
        })
    }
}
impl FileArtifactStore {
    pub(crate) fn create_execution_sync(
        &self,
        scope: ExecutionArtifactScope,
        spec: ArtifactSpec,
    ) -> Result<(ArtifactId, ArtifactWriter), PortError> {
        if spec.organization_id != scope.organization
            || spec.owner_principal_id != scope.principal
            || spec.execution_id != scope.execution
            || spec.ttl_seconds == 0
        {
            return Err(PortError::TokenInvalid);
        }
        self.create_record(spec, Some(scope))
    }
    pub(super) fn create_record(
        &self,
        spec: ArtifactSpec,
        execution_scope: Option<ExecutionArtifactScope>,
    ) -> Result<(ArtifactId, ArtifactWriter), PortError> {
        self.expire_at(now())?;
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        if state.records.len() >= 4096 {
            return Err(PortError::QuotaExceeded("artifact count".into()));
        }
        let id = ArtifactId::new(
            crate::directory::DbSessionIdGenerator::new()
                .generate()
                .as_str(),
        );
        let lease = LeaseId::new(
            crate::directory::DbSessionIdGenerator::new()
                .generate()
                .as_str(),
        );
        let record = Record {
            delegated_execution: execution_scope.is_some(),
            execution_scope,
            organization: spec.organization_id,
            owner: spec.owner_principal_id,
            execution: spec.execution_id,
            fingerprint: spec.target_fingerprint.clone(),
            expires: now().saturating_add(spec.ttl_seconds),
            sizes: Vec::new(),
            writer: Some(lease.clone()),
            unpublished_bytes: 0,
            metadata: ArtifactMetadata {
                target_fingerprint: spec.target_fingerprint,
                artifact_id: id.clone(),
                lifecycle: ArtifactLifecycle::Writing,
                published_chunk_count: Counter::ZERO,
                published_byte_size: Counter::ZERO,
                total_chunks: None,
                result_completeness: ResultCompleteness::Pending,
                truncation_reason: None,
            },
        };
        fs::create_dir(self.dir(&id)).map_err(|_| unavailable())?;
        private_dir(&self.dir(&id))?;
        if let Err(error) = self.commit(&record) {
            let _ = fs::remove_dir_all(self.dir(&id));
            return Err(error);
        }
        state.records.insert(id.clone(), record);
        Ok((
            id.clone(),
            ArtifactWriter {
                artifact_id: id,
                lease_id: lease,
                next_chunk_index: Counter::ZERO,
                published_chunk_count: Counter::ZERO,
                published_byte_size: Counter::ZERO,
            },
        ))
    }
    pub(crate) fn read_execution_sync(
        &self,
        scope: &ExecutionArtifactScope,
        id: &ArtifactId,
        index: Counter,
    ) -> Result<(ArtifactChunk, ArtifactMetadata), PortError> {
        let state = self.state.lock().map_err(|_| unavailable())?;
        let record = state
            .records
            .get(id)
            .filter(|record| {
                record.execution_scope.as_ref() == Some(scope)
                    && record.delegated_execution
                    && record.execution == scope.execution
            })
            .ok_or_else(|| PortError::NotFound("artifact".into()))?;
        if record.expires <= now() {
            return Err(PortError::ArtifactExpired);
        }
        if matches!(
            record.metadata.lifecycle,
            ArtifactLifecycle::Revoked | ArtifactLifecycle::Expired
        ) {
            return Err(PortError::NotFound("artifact".into()));
        }
        Ok((
            self.read_record(id, index, record)?,
            record.metadata.clone(),
        ))
    }
    pub(super) fn read_record(
        &self,
        id: &ArtifactId,
        index: Counter,
        r: &Record,
    ) -> Result<ArtifactChunk, PortError> {
        if !r.metadata.is_published(index) {
            return Err(invalid());
        }
        let offset = r.sizes.iter().take(index.get() as usize).sum();
        let bytes = read_block(
            &self.dir(id).join(format!("{}.chunk", index.get())),
            r.sizes[index.get() as usize],
        )?;
        if bytes.len() as u64 != r.sizes[index.get() as usize] {
            return Err(unavailable());
        }
        Ok(ArtifactChunk {
            artifact_id: id.clone(),
            chunk_index: index,
            total_chunks: r.metadata.total_chunks,
            offset: Counter::new(offset),
            bytes,
            result_completeness: r.metadata.result_completeness,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_result_cannot_change_execution_identity_or_outlive_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileArtifactStore::open(dir.path()).unwrap();
        let ctx = RequestContext::new(
            OrganizationId::new("org"),
            PrincipalId::new("service"),
            None,
            ClientInstanceId::new("scheduler"),
            RequestId::new("request"),
            Some(DelegationId::new("grant")),
        );
        let execution = ExecutionId::new("execution");
        let scope = ExecutionArtifactScope::admitted(&ctx, execution.clone()).unwrap();
        let spec = ArtifactSpec::new(
            ctx.organization_id.clone(),
            ctx.principal_id.clone(),
            execution,
            "target".into(),
        );
        let mut wrong = spec.clone();
        wrong.execution_id = ExecutionId::new("other-execution");
        assert!(store.create_execution_sync(scope.clone(), wrong).is_err());
        let (artifact, mut writer) = store.create_execution_sync(scope.clone(), spec).unwrap();
        store
            .append_sync(&mut writer, ChunkPayload::new(b"result".to_vec()))
            .unwrap();
        store.finish_sync(writer, None).unwrap();
        assert!(store
            .read_execution_sync(&scope, &artifact, Counter::ZERO)
            .is_ok());
        assert!(store.describe_sync(&ctx, &artifact).is_err());
        for case in 0..4 {
            let mut wrong = scope.clone();
            match case {
                0 => wrong.execution = ExecutionId::new("other-execution"),
                1 => wrong.principal = PrincipalId::new("other-service"),
                2 => wrong.client = ClientInstanceId::new("other-scheduler"),
                _ => wrong.delegation = DelegationId::new("other-grant"),
            }
            assert!(store
                .read_execution_sync(&wrong, &artifact, Counter::ZERO)
                .is_err());
        }
        // The opaque scope is never serialized with the durable result.
        let metadata = fs::read_to_string(store.dir(&artifact).join("metadata.json")).unwrap();
        for private in ["execution_scope", "scheduler", "grant"] {
            assert!(!metadata.contains(private));
        }
        store
            .state
            .lock()
            .unwrap()
            .records
            .get_mut(&artifact)
            .unwrap()
            .expires = 0;
        assert!(matches!(
            store.read_execution_sync(&scope, &artifact, Counter::ZERO),
            Err(PortError::ArtifactExpired)
        ));
        assert_eq!(
            store.reclaim_expired_sync().unwrap(),
            vec![artifact.clone()]
        );
        assert!(!store.dir(&artifact).exists());
    }
}
