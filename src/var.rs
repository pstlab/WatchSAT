use std::{fmt, ops::Deref};

/// Identifier of a SAT variable managed by [`crate::Engine`].
///
/// Variable identifiers are stable for the lifetime of an engine and are
/// created via [`crate::Engine::add_var`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VarId(usize);

impl VarId {
    pub(crate) const fn new(index: usize) -> Self {
        VarId(index)
    }
}

impl Deref for VarId {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for VarId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b{}", self.0)
    }
}
