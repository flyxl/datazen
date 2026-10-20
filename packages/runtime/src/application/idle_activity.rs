//! A dispatch owns an idle hold until its actual background future completes.
//! Multiple accepted commands share a counted hold; receipt replay has no hold.
use super::*;
pub(super) struct IdleActivity {
    service: RuntimeConnectionUseCases,
    handle: Option<SessionHandle>,
}
impl IdleActivity {
    pub async fn finish(mut self) {
        if let Some(handle) = self.handle.take() {
            self.service.finish_idle_activity(&handle).await;
        }
    }
}
impl Drop for IdleActivity {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let service = self.service.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                service.finish_idle_activity(&handle).await;
            });
        }
    }
}
impl RuntimeConnectionUseCases {
    pub(super) async fn begin_idle_activity(
        &self,
        handle: &SessionHandle,
    ) -> Result<IdleActivity, ApiError> {
        let mut activity = self.inner.idle_activity.lock().await;
        let count = activity.entry(handle.clone()).or_default();
        if *count == 0 {
            // Keep the hold even across the actor completion/application finish
            // handoff, where the registry no longer has an in-flight command.
            if let Err(error) = self
                .inner
                .registry
                .set_idle_deadline(&convert::handle(handle)?, None)
                .await
            {
                activity.remove(handle);
                return Err(convert::runtime(error));
            }
        }
        *count += 1;
        let _ = self
            .inner
            .directory
            .touch(handle, Timestamp::new(super::operations::utc_now()))
            .await;
        Ok(IdleActivity {
            service: self.clone(),
            handle: Some(handle.clone()),
        })
    }
    async fn finish_idle_activity(&self, handle: &SessionHandle) {
        let mut activity = self.inner.idle_activity.lock().await;
        let Some(count) = activity.get_mut(handle) else {
            return;
        };
        *count -= 1;
        if *count != 0 {
            return;
        }
        activity.remove(handle);
        let _ = self
            .inner
            .directory
            .touch(handle, Timestamp::new(super::operations::utc_now()))
            .await;
        let policy = self.idle_policy();
        let detached = lock(&self.inner.sessions)
            .get(&handle.db_session_id)
            .is_some_and(|entry| !entry.attached);
        let span = if detached {
            match (policy.deadline_ms(), policy.detached_grace_ms()) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        } else {
            policy.deadline_ms()
        };
        let deadline = span.map(|span| self.inner.clock.now_ms().saturating_add(span));
        if let Ok(internal) = convert::handle(handle) {
            if let Err(error) = self
                .inner
                .registry
                .set_idle_deadline(&internal, deadline)
                .await
            {
                // Explicit close/lost resource wins. Never reopen it or change SQL outcome.
                tracing::debug!(code=?error,"idle completion refresh unavailable");
            }
        }
    }
}
