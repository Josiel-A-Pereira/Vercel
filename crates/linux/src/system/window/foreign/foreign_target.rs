use super::foreign_names::ProcessNames;

/// The window a caller means: its title and the names its process answers to.
#[derive(Debug, Clone)]
pub(in crate::system) struct ForeignTarget<'a> {
    pub(in crate::system) title: &'a str,
    pub(in crate::system) names: ProcessNames,
}
