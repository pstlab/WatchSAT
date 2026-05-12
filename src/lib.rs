mod clause;
mod lbool;
mod lit;
mod var;

use crate::clause::{Clause, ClauseId};
pub use lbool::LBool;
pub use lit::{FALSE_LIT, Lit, TRUE_LIT, neg, pos};
pub use var::VarId;

pub struct Engine {
    assigns: Vec<LBool>,             // Current assignments of variables
    pos_watches: Vec<Vec<ClauseId>>, // Clauses watching the positive literal of each variable
    neg_watches: Vec<Vec<ClauseId>>, // Clauses watching the negative literal of each variable
    clauses: Vec<Clause>,            // List of clauses in the engine
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropagationError {
    Conflict { clause: Vec<Lit> },
}

impl Engine {
    pub fn new() -> Self {
        let mut e = Engine { assigns: Vec::new(), pos_watches: Vec::new(), neg_watches: Vec::new(), clauses: Vec::new() };
        e.add_var();
        e
    }

    pub fn add_var(&mut self) -> VarId {
        let var_id = self.assigns.len();
        self.assigns.push(LBool::default());
        VarId(var_id)
    }

    pub fn value(&self, var: VarId) -> LBool {
        self.assigns[var.0].clone()
    }

    pub fn lit_value(&self, lit: &Lit) -> LBool {
        match self.value(lit.var()) {
            LBool::True => {
                if lit.is_positive() {
                    LBool::True
                } else {
                    LBool::False
                }
            }
            LBool::False => {
                if lit.is_positive() {
                    LBool::False
                } else {
                    LBool::True
                }
            }
            LBool::Undef => LBool::Undef,
        }
    }

    pub fn add_clause(&mut self, lits: impl IntoIterator<Item = Lit>) -> Result<(), PropagationError> {
        let lits = lits.into_iter().collect::<Vec<_>>();
        if lits.is_empty() {
            return Err(PropagationError::Conflict { clause: vec![] });
        } else if lits.len() == 1 {
            if !self.enqueue(lits[0], None) {
                return Err(PropagationError::Conflict { clause: lits });
            }
            return Ok(());
        }

        let clause_id = self.clauses.len();
        for lit in &lits[..2] {
            if lit.is_positive() {
                self.pos_watches[lit.var().0].push(ClauseId(clause_id));
            } else {
                self.neg_watches[lit.var().0].push(ClauseId(clause_id));
            }
        }
        self.clauses.push(Clause { lits });
        Ok(())
    }

    fn enqueue(&mut self, lit: Lit, reason: Option<ClauseId>) -> bool {
        match self.value(lit.var()) {
            LBool::True => lit.is_positive(),
            LBool::False => !lit.is_positive(),
            LBool::Undef => {
                self.assigns[lit.var().0] = if lit.is_positive() { LBool::True } else { LBool::False };
                true
            }
        }
    }
}
