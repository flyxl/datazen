//! Read-only resource owner admission under current persisted claim and stage.
use super::{access::db_read_error, SqliteJobRepository};
use datazen_platform_api::{
    context::RequestContext, dto::job::JobClaim, error::PortError, id::StageId,
};
use rusqlite::{params, Connection, OptionalExtension};
impl SqliteJobRepository {
    fn current_owner_plan(
        &self,
        conn: &Connection,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: &StageId,
    ) -> Result<(String, String), PortError> {
        let now = self.clock.now();
        conn.query_row(
            "SELECT j.kind,j.plan_json FROM jobs j JOIN job_stages s ON s.job_id=j.job_id \
             WHERE j.job_id=?1 AND j.organization_id=?2 AND j.owner_principal_id=?3 \
             AND j.state='running' AND j.cancel_requested=0 AND j.worker_id=?4 \
             AND j.claim_generation=?5 AND j.claim_expires_at=?6 AND j.claim_expires_at>?7 \
             AND j.claim_client_instance_id=?8 AND j.stage=?9 AND s.stage_id=?9 \
             AND s.claimed_by=?4 AND s.started_at IS NOT NULL AND s.finished_at IS NULL",
            params![
                claim.job_id.as_str(),
                ctx.organization_id.as_str(),
                ctx.principal_id.as_str(),
                claim.worker_id.as_str(),
                i64::try_from(claim.claim_generation.get()).map_err(|_| PortError::StaleClaim)?,
                claim.expires_at.as_str(),
                now.as_str(),
                ctx.client_instance_id.as_str(),
                stage.as_str()
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_read_error)?
        .ok_or(PortError::StaleClaim)
    }
    pub(super) fn validate_stage_owner_read(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: &StageId,
    ) -> Result<(), PortError> {
        self.with_read(|conn| self.current_owner_plan(conn, ctx, claim, stage).map(|_| ()))
    }
}
