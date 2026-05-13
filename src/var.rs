use std::fmt;

/// Identifier of a SAT variable managed by [`crate::Engine`].
///
/// Variable identifiers are stable for the lifetime of an engine and are
/// created via [`crate::Engine::add_var`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarId(pub(super) usize);

impl fmt::Display for VarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b{}", self.0)
    }
}
