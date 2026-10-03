//! Non-queuing per-process bulkheads isolate slow apps without holding database connections.
use super::*;
use std::sync::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
pub(crate) struct ServiceLimits {
    total: Arc<Semaphore>,
    tenants: Mutex<HashMap<String, Arc<Semaphore>>>,
}
impl Default for ServiceLimits {
    fn default() -> Self {
        Self {
            total: Arc::new(Semaphore::new(64)),
            tenants: Mutex::new(HashMap::new()),
        }
    }
}
impl ServiceLimits {
    pub(crate) fn enter(
        &self,
        tenant: &str,
        app: &str,
    ) -> Result<(OwnedSemaphorePermit, OwnedSemaphorePermit)> {
        let busy = || {
            Error(
                StatusCode::TOO_MANY_REQUESTS,
                "App execution capacity reached; retry later".into(),
            )
        };
        let total = self.total.clone().try_acquire_owned().map_err(|_| busy())?;
        let mut tenants = self.tenants.lock().map_err(|_| busy())?;
        // Drop idle keys; active permits keep their semaphore alive. No eviction can reset an active limit.
        if tenants.len() >= 1024 {
            tenants.retain(|_, s| Arc::strong_count(s) > 1);
        }
        let key = format!("{tenant}:{app}");
        let slots = tenants
            .entry(key)
            .or_insert_with(|| Arc::new(Semaphore::new(8)))
            .clone();
        let local = slots.try_acquire_owned().map_err(|_| busy())?;
        Ok((total, local))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_busy_app_cannot_exhaust_other_tenants_and_drop_restores_capacity() {
        let limits = ServiceLimits::default();
        let held = (0..8)
            .map(|_| limits.enter("one", "slow").unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            limits.enter("one", "slow").unwrap_err().0,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert!(limits.enter("two", "slow").is_ok());
        assert!(limits.enter("one", "fast").is_ok());
        drop(held);
        assert!(limits.enter("one", "slow").is_ok());
    }
}
