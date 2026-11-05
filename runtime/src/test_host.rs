//! Synthetic trusted host for independent guest regressions; production consumers supply their own canonical policy.
use crate::{
    component_runtime::Module,
    policy::{ResourcePolicy, Resources},
    sandbox::Sandbox,
};
pub(crate) struct Host;
impl ResourcePolicy for Host {
    fn admitted(&self, r: Resources) -> bool {
        r.tables <= 1 && r.memories <= 1 && r.elements <= 10_000 && r.pages <= 16
    }
}
pub(crate) fn sandbox(source: &str) -> Result<Sandbox, String> {
    Sandbox::new(source, &Host, "")
}
pub(crate) fn component(source: &str) -> Result<Module, String> {
    Module::new(source, &Host, "")
}
pub(crate) fn component_with_namespaces(source: &str, namespaces: &str) -> Result<Module, String> {
    Module::new(source, &Host, namespaces)
}
