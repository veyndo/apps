//! Mandatory trusted-host resource admission; runtime limits also apply independently to every guest store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    pub tables: u64,
    pub memories: u64,
    pub elements: u64,
    pub pages: u64,
}
/// Implement from the host's canonical admission owner, never request flags or untrusted app configuration.
pub trait ResourcePolicy {
    fn admitted(&self, resources: Resources) -> bool;
}
