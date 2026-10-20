//! A dispatch owns an idle hold until its actual background future completes.
//! Multiple accepted commands share a counted hold; receipt replay has no hold.
use super::*;
pub(super) struct ActivityState {
    count: usize,
    original_deadline: Option<u64>,
    last_business_ms: Option<u64>,
}
pub(super) struct IdleActivity {
    service: RuntimeConnectionUseCases,
    handle: Option<SessionHandle>,
    business: bool,
}
impl IdleActivity {
    pub fn business_started(&mut self) {
        self.business = true;
    }
    pub async fn finish(mut self) {
        if let Some(handle) = self.handle.take() {
            self.service
                .finish_idle_activity(&handle, self.business)
                .await;
        }
    }
}
impl Drop for IdleActivity {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let service = self.service.clone();
        let business = self.business;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                service.finish_idle_activity(&handle, business).await;
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
        if let Some(state) = activity.get_mut(handle) {
            state.count += 1;
        } else {
            let original_deadline = self
                .inner
                .registry
                .suspend_idle_deadline(&convert::handle(handle)?)
                .await
                .map_err(convert::runtime)?;
            activity.insert(
                handle.clone(),
                ActivityState {
                    count: 1,
                    original_deadline,
                    last_business_ms: None,
                },
            );
        }
        Ok(IdleActivity {
            service: self.clone(),
            handle: Some(handle.clone()),
            business: false,
        })
    }
    async fn finish_idle_activity(&self, handle: &SessionHandle, business: bool) {
        let mut activity = self.inner.idle_activity.lock().await;
        let Some(state) = activity.get_mut(handle) else {
            return;
        };
        if business {
            state.last_business_ms = Some(self.inner.clock.now_ms());
        }
        state.count -= 1;
        if state.count != 0 {
            return;
        }
        let state = activity.remove(handle).unwrap_or(ActivityState {
            count: 0,
            original_deadline: None,
            last_business_ms: None,
        });
        let Some(last_business_ms) = state.last_business_ms else {
            if let Ok(internal) = convert::handle(handle) {
                let _ = self
                    .inner
                    .registry
                    .set_idle_deadline(&internal, state.original_deadline)
                    .await;
            }
            return;
        };
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
        let deadline = span.map(|span| last_business_ms.saturating_add(span));
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
