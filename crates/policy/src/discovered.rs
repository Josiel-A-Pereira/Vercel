use crate::PolicyError;
use crate::discover::PolicySources;

/// The policy layers one invocation runs under, and the layers that could
/// not be read. A failed layer refuses mutations; it never lifts the
/// verdict of a healthy layer.
#[derive(Debug, Default)]
pub struct Discovered {
    pub layers: Vec<PolicySources>,
    pub failures: Vec<PolicyError>,
}

impl Discovered {
    /// Whether any policy source is configured, healthy or not.
    pub fn is_configured(&self) -> bool {
        !self.layers.is_empty() || !self.failures.is_empty()
    }

    pub(crate) fn add(&mut self, layer: Result<Option<PolicySources>, PolicyError>) {
        match layer {
            Ok(Some(sources)) => self.layers.push(sources),
            Ok(None) => {}
            Err(error) => self.failures.push(error),
        }
    }
}
