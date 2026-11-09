//! Complete ordered package admission over shared domain validators; only actual host policy/trust decisions are required ports.
use crate::{
    Action, business_capability, commerce_hooks, configuration, distribution, events, evolution,
    intelligence,
    manifest::{self, Manifest},
    native_lifecycle, object_extensions, ontology,
    package::{self, CompileError},
    payment_provider, presentation, settings, subscriptions, surfaces, triggers, views,
};
pub use chrono::{DateTime, Utc};
/// Trusted host policies have no defaults. These decisions never come from package JSON or a caller's role/tenant fields.
pub trait Policy:
    configuration::Policy
    + package::Policy
    + views::Policy
    + native_lifecycle::CompilerPolicy
    + distribution::SignatureVerifier
{
    fn unsigned_namespace(&self, manifest: &Manifest) -> Result<(), Self::Error>;
    fn destination(&self, url: &str) -> Result<(), Self::Error>;
    fn readonly(&self, action: &Action) -> bool;
    fn now(&self) -> DateTime<Utc>;
}
struct Destination<'a, P>(&'a P);
impl<P: Policy> events::DestinationPolicy for Destination<'_, P> {
    type Error = P::Error;
    fn validate_destination(&self, url: &str) -> Result<(), Self::Error> {
        self.0.destination(url)
    }
}
struct SurfacePolicy<'a, P> {
    manifest: &'a Manifest,
    policy: &'a P,
}
fn view_contract(m: &Manifest) -> views::Contract<'_> {
    views::Contract {
        entities: &m.entities,
        actions: &m.actions,
        surfaces: &m.surfaces,
        views: &m.views,
    }
}
impl<P: Policy> surfaces::Policy for SurfacePolicy<'_, P> {
    fn native_view(&self, s: &surfaces::Surface) -> bool {
        views::is_native(&view_contract(self.manifest), s)
    }
    fn assistant_view(&self, surface: &surfaces::Surface) -> Option<&views::NativeView> {
        self.manifest
            .views
            .iter()
            .find(|view| surface.ui_path == format!("native/{}", view.id))
    }
    fn known_scope(&self, s: &str) -> bool {
        package::Policy::known_scope(self.policy, s)
    }
    fn readonly(&self, a: &Action) -> bool {
        self.policy.readonly(a)
    }
}
/// Runs every standard domain in its original native order; returning success does not install, authenticate or execute an app.
pub fn compile<P: Policy>(m: &Manifest, policy: &P) -> Result<(), CompileError<P::Error>> {
    // Exhaustive on purpose: adding package metadata requires reviewing the complete
    // domain pipeline, even when an individual validator accepts the whole Manifest.
    let Manifest {
        id: _,
        version: _,
        core_api: _,
        runtime: _,
        name: _,
        category: _,
        distribution: _,
        presentation: _,
        capability: _,
        lifecycles: _,
        permissions: _,
        configuration: _,
        settings: _,
        commerce_hooks: _,
        payment_provider: _,
        entities: _,
        slots: _,
        actions: _,
        events: _,
        event_filters: _,
        event_delivery: _,
        surfaces: _,
        api_routes: _,
        intelligence: _,
        views: _,
        schedules: _,
        schema_migrations: _,
        webhooks: _,
    } = m;
    let model = CompileError::Model;
    object_extensions::validate(&m.entities, &m.actions, &m.permissions).map_err(model)?;
    let digest = manifest::canonical_digest(m);
    native_lifecycle::validate(
        &native_lifecycle::Package {
            id: &m.id,
            version: &m.version,
            digest: &digest,
            permissions: &m.permissions,
            lifecycles: &m.lifecycles,
        },
        policy,
    )
    .map_err(model)?;
    settings::validate(&m.settings).map_err(model)?;
    configuration::validate(
        &configuration::Context {
            entities: &m.entities,
            actions: &m.actions,
            configuration: m.configuration.as_ref(),
        },
        policy,
    )
    .map_err(model)?;
    presentation::validate(m.presentation.as_ref()).map_err(model)?;
    ontology::validate(&intelligence::Context {
        id: &m.id,
        version: &m.version,
        entities: &m.entities,
        actions: &m.actions,
        permissions: &m.permissions,
        intelligence: m.intelligence.as_ref(),
    })
    .map_err(model)?;
    evolution::validate(&m.version, &m.schema_migrations).map_err(model)?;
    distribution::verify(m, policy).map_err(|e| match e {
        distribution::VerifyError::Host(e) => CompileError::Host(e),
        distribution::VerifyError::Model(e) => CompileError::Model(e),
    })?;
    policy.unsigned_namespace(m).map_err(CompileError::Host)?;
    commerce_hooks::validate(
        &commerce_hooks::Context {
            entities: &m.entities,
            permissions: &m.permissions,
            commerce_hooks: m.commerce_hooks.as_ref(),
        },
        |s| package::Policy::component_source(policy, s),
    )
    .map_err(model)?;
    business_capability::validate(
        &business_capability::Context {
            entities: &m.entities,
            commerce_hooks: m.commerce_hooks.as_ref(),
            capability: m.capability.as_ref(),
        },
        policy,
    )
    .map_err(model)?;
    payment_provider::validate_contract(m).map_err(model)?;
    package::validate(&m.common_contract(), policy).map_err(model)?;
    let context = events::Context {
        id: &m.id,
        events: &m.events,
        permissions: &m.permissions,
        event_filters: &m.event_filters,
    };
    subscriptions::validate(&context).map_err(model)?;
    events::validate(&context, m.event_delivery.as_ref(), &Destination(policy))?;
    triggers::schedules(&m.schedules, &m.actions, policy.now()).map_err(model)?;
    triggers::webhooks(&m.webhooks, &m.actions).map_err(model)?;
    surfaces::validate(
        &surfaces::Contract {
            runtime: &m.runtime,
            permissions: &m.permissions,
            actions: &m.actions,
            surfaces: &m.surfaces,
            api_routes: &m.api_routes,
        },
        &SurfacePolicy {
            manifest: m,
            policy,
        },
    )
    .map_err(model)?;
    intelligence::validate(m.intelligence.as_ref(), &m.actions, &m.entities).map_err(model)?;
    views::validate(&view_contract(m), policy).map_err(model)
}
