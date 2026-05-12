use crate::Lit;
use std::fmt;

pub(super) struct Clause {
    pub(super) lits: Vec<Lit>,
}

impl fmt::Display for Clause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lits: Vec<String> = self.lits.iter().map(|l| l.to_string()).collect();
        write!(f, "{}", lits.join(" ∨ "))
    }
}
