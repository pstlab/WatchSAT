use crate::Lit;
use std::{fmt, ops::Deref};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ClauseId(usize);

impl ClauseId {
    pub(crate) const fn new(index: usize) -> Self {
        ClauseId(index)
    }
}

impl Deref for ClauseId {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for ClauseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "c{}", self.0)
    }
}

pub(super) struct Clause {
    pub(super) lits: Vec<Lit>,
}

impl fmt::Display for Clause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lits: Vec<String> = self.lits.iter().map(|l| l.to_string()).collect();
        write!(f, "{}", lits.join(" ∨ "))
    }
}
