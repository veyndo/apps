//! One bounded evaluation path for HTTP and MCP; task-owned leases survive client disconnects until actual guest execution ends.
use crate::{
    App,
    contract::{Evaluation, EvaluationResult},
    error::{Problem, Result},
    identity::Principal,
};
use axum::http::StatusCode;
use sha2::{Digest, Sha256};
use tokio::sync::OwnedSemaphorePermit;
use veyndo_app_runtime::{
    component_runtime::Module,
    policy::{ResourcePolicy, Resources},
};
struct Policy;
impl ResourcePolicy for Policy {
    fn admitted(&self, r: Resources) -> bool {
        r.tables <= 1 && r.memories <= 1 && r.elements <= 10000 && r.pages <= 16
    }
}
use crate::Slots;
struct Lease {
    slots: Slots,
    keys: [(String, Option<String>); 2],
    _global: OwnedSemaphorePermit,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut slots) = self.slots.lock() {
            for key in &self.keys {
                if let Some(n) = slots.get_mut(key) {
                    *n -= 1;
                    if *n == 0 {
                        slots.remove(key);
                    }
                }
            }
        }
    }
}
fn busy() -> Problem {
    Problem::new(
        StatusCode::TOO_MANY_REQUESTS,
        "execution_capacity",
        "The app runtime is busy for this caller; retry shortly",
    )
}
fn capacity(app: &App, p: &Principal) -> Result<Lease> {
    let global = app
        .execution
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    let keys = [
        (p.tenant.clone(), None),
        (p.tenant.clone(), Some(p.subject.clone())),
    ];
    let mut slots = app.tenants.lock().map_err(|_| busy())?;
    for (key, max) in keys.iter().zip([2, 1]) {
        if slots.get(key).copied().unwrap_or(0) >= max {
            return Err(busy());
        }
    }
    for key in &keys {
        *slots.entry(key.clone()).or_default() += 1;
    }
    Ok(Lease {
        slots: app.tenants.clone(),
        keys,
        _global: global,
    })
}
pub async fn evaluate(app: &App, p: Principal, input: Evaluation) -> Result<EvaluationResult> {
    input.validate()?;
    let lease = capacity(app, &p)?;
    tokio::task::spawn_blocking(move ||{
        let _lease=lease;
        let source_digest=format!("{:x}",Sha256::digest(input.source.as_bytes()));
        let input_digest=veyndo_app_model::manifest::value_digest(&serde_json::json!(input));
        let module=Module::new(&input.source,&Policy,"").map_err(|_|Problem::new(StatusCode::UNPROCESSABLE_ENTITY,"component_rejected","Check the Wasm component syntax, allowed imports, namespace and memory/table limits"))?;
        module.validate_interface().map_err(|_|Problem::new(StatusCode::UNPROCESSABLE_ENTITY,"interface_rejected","The component must implement the veyndo:commerce@1.0.0 WIT interface"))?;
        let snapshot=veyndo_app_runtime::business_snapshot::snapshot(&input.snapshot);
        let outcome=module.evaluate(snapshot,input.hook.runtime(),&input.context.to_string()).map_err(|_|Problem::new(StatusCode::UNPROCESSABLE_ENTITY,"execution_rejected","The guest trapped, exceeded its call budget or returned an invalid money/reason result"))?;
        Ok(EvaluationResult{contract_version:1,mode:"simulation",effect_applied:false,source_digest,input_digest,hook:input.hook,outcome:veyndo_app_model::business_capability::Expected{adjustment:veyndo_app_model::business_snapshot::Amount{minor:outcome.adjustment.minor,currency:outcome.adjustment.currency,scale:outcome.adjustment.scale},accepted:outcome.accepted,reason_code:outcome.reason_code}})
    }).await.map_err(|_|Problem::new(StatusCode::INTERNAL_SERVER_ERROR,"runtime_unavailable","The isolated evaluation could not finish; no commerce effect was applied"))?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_waiter_cannot_release_execution_capacity() {
        let app = App::new(
            crate::config::Config::new(
                "http://127.0.0.1:1/introspect",
                "x".repeat(32),
                "runtime".into(),
                "127.0.0.1:0".parse().unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
        let principal = || Principal {
            tenant: "shop".into(),
            subject: "actor".into(),
        };
        let lease = capacity(&app, &principal()).unwrap();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let (finish, wait) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                wait.recv().unwrap();
                drop(lease);
                finished.send(()).unwrap();
            })
            .await
            .unwrap();
        });
        ready.await.unwrap();
        task.abort();
        assert!(capacity(&app, &principal()).is_err());
        finish.send(()).unwrap();
        done.await.unwrap();
        assert!(capacity(&app, &principal()).is_ok());
        assert!(app.tenants.lock().unwrap().is_empty());
    }
}
