mod clause;
mod lbool;
mod lit;
mod var;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt, mem,
};

use crate::clause::{Clause, ClauseId};
pub use lbool::LBool;
pub use lit::{FALSE_LIT, Lit, TRUE_LIT, neg, pos};
pub use var::VarId;

type Callback = Box<dyn Fn(VarId, LBool)>;

pub struct Engine {
    assigns: Vec<LBool>,                      // Current assignments of variables
    reason: Vec<Option<ClauseId>>,            // Reason for each variable's assignment
    propagated_vars: Vec<VarId>,              // Variables that have been propagated by decision variables
    decision_vars: Vec<Option<VarId>>,        // Decision variables that caused the propagation of each variable
    decision_var: VarId,                      // Current decision variable
    pos_watches: Vec<Vec<ClauseId>>,          // Clauses watching the positive literal of each variable
    neg_watches: Vec<Vec<ClauseId>>,          // Clauses watching the negative literal of each variable
    clauses: Vec<Clause>,                     // List of clauses in the engine
    prop_q: VecDeque<VarId>,                  // Queue of variables to propagate
    listeners: HashMap<VarId, Vec<Callback>>, // Listeners for variable assignments
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
        let mut e = Engine {
            assigns: Vec::new(),
            reason: Vec::new(),
            propagated_vars: Vec::new(),
            decision_vars: Vec::new(),
            decision_var: VarId(usize::MAX), // No decision variable at the start
            pos_watches: Vec::new(),
            neg_watches: Vec::new(),
            clauses: Vec::new(),
            prop_q: VecDeque::new(),
            listeners: HashMap::new(),
        };
        e.add_var();
        e.assigns[0] = LBool::True; // TRUE_LIT is always true
        e
    }

    pub fn add_var(&mut self) -> VarId {
        let var_id = self.assigns.len();
        self.assigns.push(LBool::default());
        self.reason.push(None);
        self.decision_vars.push(None);
        self.pos_watches.push(Vec::new());
        self.neg_watches.push(Vec::new());
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

    pub fn decision_var(&self, var: VarId) -> Option<VarId> {
        self.decision_vars[var.0]
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

    pub fn assert(&mut self, lit: Lit) -> Result<(), PropagationError> {
        assert!(self.value(lit.var()) == LBool::Undef, "Variable b{} is already assigned", lit.var());
        self.decision_var = lit.var();
        self.propagated_vars.clear();
        self.enqueue(lit, None);
        while let Some(var) = self.prop_q.pop_front() {
            let watches = if self.value(var) == LBool::True { mem::take(&mut self.neg_watches[var.0]) } else { mem::take(&mut self.pos_watches[var.0]) };
            for i in 0..watches.len() {
                if !self.propagate(watches[i], Lit::new(var, self.value(var) == LBool::True)) {
                    for &watch in watches.iter().skip(i) {
                        if self.value(var) == LBool::True {
                            self.neg_watches[var.0].push(watch);
                        } else {
                            self.pos_watches[var.0].push(watch);
                        }
                    }
                    self.prop_q.clear();
                    return Err(PropagationError::Conflict { clause: self.analyze_conflict(watches[i]) });
                }
            }
        }
        Ok(())
    }

    fn enqueue(&mut self, lit: Lit, reason: Option<ClauseId>) -> bool {
        match self.value(lit.var()) {
            LBool::True => lit.is_positive(),
            LBool::False => !lit.is_positive(),
            LBool::Undef => {
                self.assigns[lit.var().0] = if lit.is_positive() { LBool::True } else { LBool::False };
                self.reason[lit.var().0] = reason;
                self.propagated_vars.push(lit.var());
                if lit.var() != self.decision_var {
                    self.decision_vars[lit.var().0] = Some(self.decision_var);
                }
                self.prop_q.push_back(lit.var());
                if let Some(listeners) = self.listeners.get(&lit.var()) {
                    for listener in listeners {
                        listener(lit.var(), self.value(lit.var()).clone());
                    }
                }
                true
            }
        }
    }

    fn analyze_conflict(&mut self, mut clause: ClauseId) -> Vec<Lit> {
        let mut seen = HashSet::new();
        let mut counter: usize = 0;
        let mut p: Option<(Lit, Option<ClauseId>)> = None;
        let mut learnt = Vec::new();
        learnt.push(Lit::default()); // Placeholder for the asserting literal

        loop {
            // 1. Process the current clause (either the conflict or a reason)
            for lit in &self.clauses[clause.0].lits {
                let v = lit.var();

                // Skip the variable we are currently resolving away
                if Some(v) == p.map(|l| l.0.var()) {
                    continue;
                }

                if !seen.contains(&v) {
                    seen.insert(v);
                    if self.decision_vars[v.0] == Some(self.decision_var) {
                        counter += 1;
                    } else {
                        // This literal comes from a previous decision level
                        learnt.push(*lit);
                    }
                }
            }

            // 2. Find the next variable from the trail assigned at this level
            p = loop {
                let v = self.propagated_vars.pop().expect("There should be a variable to resolve away");
                if seen.contains(&v) {
                    let sign = self.value(v) == LBool::True;
                    let reason = self.reason[v.0];
                    self.undo(v);
                    break Some((Lit::new(v, sign), reason));
                }
                self.undo(v);
            };

            counter -= 1;

            // 3. Check for 1-UIP (First Unique Implication Point)
            if counter == 0 {
                learnt[0] = !p.expect("There should be a literal to assert").0;
                break;
            }

            // 4. Update clause to the reason of the variable we just resolved away
            clause = p.expect("There should be a reason clause for this variable").1.expect("There should be a reason clause for this variable");
        }

        // 5. Final cleanup - undo all assignments made at this level
        while let Some(var) = self.propagated_vars.pop() {
            self.undo(var);
        }
        self.undo(self.decision_var);
        learnt
    }

    pub fn undo(&mut self, var: VarId) {
        self.assigns[var.0] = LBool::Undef;
        self.reason[var.0] = None;
        self.decision_vars[var.0] = None;
    }

    fn propagate(&mut self, clause_id: ClauseId, lit: Lit) -> bool {
        // Ensure the first literal is not the one that was just assigned
        if self.clauses[clause_id.0].lits[0].var() == lit.var() {
            self.clauses[clause_id.0].lits.swap(0, 1);
        }
        // Check if clause is already satisfied
        if self.lit_value(&self.clauses[clause_id.0].lits[0]) == LBool::True {
            // Re-add the clause to the watch list
            if lit.is_positive() {
                self.pos_watches[lit.var().0].push(clause_id);
            } else {
                self.neg_watches[lit.var().0].push(clause_id);
            }
            return true;
        }

        // Find the next unassigned literal
        for i in 2..self.clauses[clause_id.0].lits.len() {
            if self.lit_value(&self.clauses[clause_id.0].lits[i]) != LBool::False {
                // Move this literal to the second position
                self.clauses[clause_id.0].lits.swap(1, i);
                // Update watch lists
                if self.clauses[clause_id.0].lits[1].is_positive() {
                    self.pos_watches[self.clauses[clause_id.0].lits[1].var().0].push(clause_id);
                } else {
                    self.neg_watches[self.clauses[clause_id.0].lits[1].var().0].push(clause_id);
                }
                return true;
            }
        }

        // If we reach here, the clause is either unit or unsatisfied
        if lit.is_positive() {
            self.neg_watches[lit.var().0].push(clause_id);
        } else {
            self.pos_watches[lit.var().0].push(clause_id);
        }
        self.enqueue(self.clauses[clause_id.0].lits[0], Some(clause_id))
    }

    pub fn add_listener<F>(&mut self, var: VarId, listener: F)
    where
        F: Fn(VarId, LBool) + 'static,
    {
        self.listeners.entry(var).or_default().push(Box::new(listener));
    }
}

impl fmt::Display for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, val) in self.assigns.iter().enumerate() {
            writeln!(f, "b{}: {:?}", i, val)?;
        }
        for clause in &self.clauses {
            writeln!(f, "{}", clause)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_assignment_and_value() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        assert_eq!(engine.value(a), LBool::Undef);

        engine.assert(pos(a));
        assert_eq!(engine.value(a), LBool::True);
        assert_eq!(engine.lit_value(&pos(a)), LBool::True);
        assert_eq!(engine.lit_value(&neg(a)), LBool::False);
    }

    #[test]
    fn test_unit_propagation_simple() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Clause: (¬a ∨ b)  => If a is true, b must be true.
        engine.add_clause(vec![neg(a), pos(b)]);

        engine.assert(pos(a));

        // b should be propagated to True
        assert_eq!(engine.value(b), LBool::True, "b should be propagated by unit clause");
        assert_eq!(engine.decision_var(b), Some(a), "a should be the decision variable for b");
    }

    #[test]
    fn test_chained_propagation() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();
        let c = engine.add_var();

        // (¬a ∨ b) and (¬b ∨ c)
        // a -> b -> c
        engine.add_clause(vec![neg(a), pos(b)]);
        engine.add_clause(vec![neg(b), pos(c)]);

        engine.assert(pos(a));

        assert_eq!(engine.value(c), LBool::True, "c should propagate via b");
    }

    #[test]
    fn test_two_watched_literals_movement() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();
        let c = engine.add_var();

        // Clause: (a ∨ b ∨ c)
        // Initially watching a and b.
        engine.add_clause(vec![pos(a), pos(b), pos(c)]);

        // Assign a = False.
        // 2WL should move watch from 'a' to 'c' because 'b' is still Undef.
        engine.assert(neg(a));
        assert_eq!(engine.value(b), LBool::Undef, "b should still be undef");
        assert_eq!(engine.value(c), LBool::Undef, "c should still be undef");

        // Now assign b = False.
        // This should trigger propagation on c.
        engine.assert(neg(b));
        assert_eq!(engine.value(c), LBool::True, "c must be true now");
    }

    #[test]
    fn test_listeners() {
        use std::sync::{Arc, Mutex};

        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        let triggered = Arc::new(Mutex::new(false));
        let triggered_clone = Arc::clone(&triggered);

        // Add listener to b
        engine.add_listener(b, move |_, _| {
            let mut val = triggered_clone.lock().unwrap();
            *val = true;
        });

        // (¬a ∨ b)
        engine.add_clause(vec![neg(a), pos(b)]);

        // Assert a, which propagates b, which should fire the listener
        engine.assert(pos(a));

        assert!(*triggered.lock().unwrap(), "Listener on b should have been triggered");
    }

    #[test]
    #[should_panic(expected = "already assigned")]
    fn test_double_assertion_panic() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        engine.assert(pos(a));
        engine.assert(neg(a)); // Should panic
    }

    #[test]
    fn test_diamond_propagation() {
        let mut engine = Engine::new();
        let x1 = engine.add_var();
        let x2 = engine.add_var();
        let x3 = engine.add_var();
        let x4 = engine.add_var();

        // x1 -> x2  (¬x1 ∨ x2)
        engine.add_clause(vec![neg(x1), pos(x2)]);
        // x1 -> x3  (¬x1 ∨ x3)
        engine.add_clause(vec![neg(x1), pos(x3)]);
        // (x2 ∧ x3) -> x4 (¬x2 ∨ ¬x3 ∨ x4)
        engine.add_clause(vec![neg(x2), neg(x3), pos(x4)]);

        engine.assert(pos(x1));

        assert_eq!(engine.value(x4), LBool::True, "x4 should be forced via x2 and x3");
    }

    #[test]
    fn test_complex_conflict_1uip() {
        let mut engine = Engine::new();
        let vars: Vec<VarId> = (0..10).map(|_| engine.add_var()).collect();

        // Setup a chain: x1 -> x2 -> x3 -> x4
        engine.add_clause(vec![neg(vars[1]), pos(vars[2])]);
        engine.add_clause(vec![neg(vars[2]), pos(vars[3])]);
        engine.add_clause(vec![neg(vars[3]), pos(vars[4])]);

        // Create a conflict path:
        // (x4 ∧ x5) -> Conflict
        // x5 is another decision or forced var
        engine.add_clause(vec![neg(vars[4]), neg(vars[5])]);

        // Another path to the same conflict
        // (x3 ∧ x6) -> x5
        engine.add_clause(vec![neg(vars[3]), neg(vars[6]), pos(vars[5])]);

        // Assert "side" variables that set the stage
        engine.assert(pos(vars[6]));

        // Now trigger the chain
        // This should cause: x1 -> x2 -> x3 -> x4 -> conflict with x5
        let success = engine.assert(pos(vars[1]));

        assert!(!success, "Should detect a conflict");

        let explanation = engine.get_conflict_explanation().unwrap();
        // The explanation should ideally contain the 1-UIP literal
        // and the "reason" variables from lower levels.
        assert!(!explanation.lits.is_empty());
        println!("Conflict explanation: {}", explanation);
    }

    #[test]
    fn test_conflict_analysis() {
        let mut engine = Engine::new();
        let x1 = engine.add_var();
        let x2 = engine.add_var();
        let x3 = engine.add_var();
        let x4 = engine.add_var();
        let x5 = engine.add_var();
        let x6 = engine.add_var();
        let x7 = engine.add_var();
        let x8 = engine.add_var();
        let x9 = engine.add_var();

        // (x1 ∨ x2)
        engine.add_clause(vec![pos(x1), pos(x2)]);
        // (x1 ∨ x3 ∨ x7)
        engine.add_clause(vec![pos(x1), pos(x3), pos(x7)]);
        // (¬x2 ∨ ¬x3 ∨ x4)
        engine.add_clause(vec![neg(x2), neg(x3), pos(x4)]);
        // (¬x4 ∨ x5 ∨ x8)
        engine.add_clause(vec![neg(x4), pos(x5), pos(x8)]);
        // (¬x4 ∨ x6 ∨ x9)
        engine.add_clause(vec![neg(x4), pos(x6), pos(x9)]);
        // (¬x5 ∨ ¬x6)
        engine.add_clause(vec![neg(x5), neg(x6)]);

        // Assert ¬x7
        engine.assert(neg(x7));
        // Assert ¬x8
        engine.assert(neg(x8));
        // Assert ¬x9
        engine.assert(neg(x9));

        // Assert ¬x1, which should trigger conflict analysis
        engine.assert(neg(x1));

        let explanation = engine.get_conflict_explanation().expect("There should be a conflict explanation");
        let expected_explanation = vec![neg(x4), pos(x9), pos(x8)];
        assert_eq!(explanation.lits, expected_explanation, "Conflict explanation should match expected");
    }

    #[test]
    fn test_display_implementations() {
        // Test LBool Display
        assert_eq!(format!("{}", LBool::True), "True");
        assert_eq!(format!("{}", LBool::False), "False");
        assert_eq!(format!("{}", LBool::Undef), "Undef");

        // Test Lit Display
        let lit_pos = pos(5);
        let lit_neg = neg(5);
        assert_eq!(format!("{}", lit_pos), "b5");
        assert_eq!(format!("{}", lit_neg), "¬b5");

        // Test Clause Display
        let clause = Clause { lits: vec![pos(1), neg(2), pos(3)] };
        assert_eq!(format!("{}", clause), "b1 ∨ ¬b2 ∨ b3");

        // Test Engine Display
        let mut engine = Engine::new();
        let a = engine.add_var();
        engine.add_clause(vec![pos(a)]);
        let output = format!("{}", engine);
        assert!(output.contains("b0"));
        assert!(output.contains("b1"));
    }

    #[test]
    fn test_lit_default_and_operators() {
        // Test Default
        let default_lit = Lit::default();
        assert_eq!(default_lit.var(), VarId(usize::MAX));
        assert!(!default_lit.is_positive());

        // Test Not operator
        let lit = pos(VarId(3));
        let neg_lit = !lit;
        assert_eq!(neg_lit.var(), VarId(3));
        assert!(!neg_lit.is_positive());

        // Test Not operator on reference
        let lit_ref = &pos(VarId(4));
        let neg_lit_ref = !lit_ref;
        assert_eq!(neg_lit_ref.var(), VarId(4));
        assert!(!neg_lit_ref.is_positive());

        // Test PartialOrd
        let lit1 = pos(VarId(1));
        let lit2 = pos(VarId(2));
        let lit3 = neg(VarId(1));
        assert!(lit1 < lit2);
        assert!(lit3 < lit1); // same var, but negative sorts before positive
    }

    #[test]
    fn test_decision_var_getter() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Add clause (¬a ∨ b)
        engine.add_clause(vec![neg(a), pos(b)]);

        // Before any assertion
        assert_eq!(engine.decision_var(b), None);

        // After assertion
        engine.assert(pos(a));
        assert_eq!(engine.decision_var(b), Some(a));
    }

    #[test]
    fn test_empty_clause() {
        let mut engine = Engine::new();
        let result = engine.add_clause(vec![]);
        assert!(!result, "Empty clause should return false");
    }

    #[test]
    fn test_unit_clause() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Add unit clause (a)
        engine.add_clause(vec![pos(a)]);

        // Variable 'a' should be propagated immediately
        assert_eq!(engine.value(a), LBool::True);
    }

    #[test]
    fn test_enqueue_already_assigned() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // (a ∨ b)
        engine.add_clause(vec![pos(a), pos(b)]);

        // Assert a = True
        engine.assert(pos(a));
        assert_eq!(engine.value(a), LBool::True);

        // Now try to enqueue a again with True (should succeed as it's consistent)
        let result = engine.enqueue(pos(a), None);
        assert!(result, "Enqueuing consistent value should succeed");

        // Try to enqueue a with False (should fail as it's inconsistent)
        let result2 = engine.enqueue(neg(a), None);
        assert!(!result2, "Enqueuing inconsistent value should fail");
    }

    #[test]
    fn test_no_conflict_explanation() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // No conflict has occurred
        engine.assert(pos(a));

        let explanation = engine.get_conflict_explanation();
        assert!(explanation.is_none(), "Should have no explanation when there's no conflict");
    }

    #[test]
    fn test_propagate_satisfied_clause() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();
        let c = engine.add_var();

        // (a ∨ b ∨ c)
        engine.add_clause(vec![pos(a), pos(b), pos(c)]);

        // Assert a = True (satisfies the clause)
        engine.assert(pos(a));

        // Now assert b = False
        // The propagate function should detect that the clause is already satisfied by 'a'
        engine.assert(neg(b));

        // c should still be undefined
        assert_eq!(engine.value(c), LBool::Undef);
    }

    #[test]
    fn test_watch_literal_unwatching() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();
        let c = engine.add_var();
        let d = engine.add_var();

        // (a ∨ b ∨ c ∨ d) - 4 literals, initially watching a and b
        engine.add_clause(vec![pos(a), pos(b), pos(c), pos(d)]);

        // Assert a = False, should move watch to c
        engine.assert(neg(a));
        assert_eq!(engine.value(b), LBool::Undef);
        assert_eq!(engine.value(c), LBool::Undef);
        assert_eq!(engine.value(d), LBool::Undef);

        // Assert b = False, should move watch to d
        engine.assert(neg(b));
        assert_eq!(engine.value(c), LBool::Undef);
        assert_eq!(engine.value(d), LBool::Undef);

        // Assert c = False, should propagate d
        engine.assert(neg(c));
        assert_eq!(engine.value(d), LBool::True, "d should be propagated");
    }

    #[test]
    fn test_conflict_with_multiple_watched_literals() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Create clauses that will conflict
        // (a ∨ b)
        engine.add_clause(vec![pos(a), pos(b)]);
        // (¬a ∨ ¬b)
        engine.add_clause(vec![neg(a), neg(b)]);

        // Assert a = True, this forces b = False from second clause
        engine.assert(pos(a));
        assert_eq!(engine.value(b), LBool::False);

        // Now try to assert b = True, which conflicts
        // Since b is already assigned, we can't directly assert it
        // So let's test that the watches work correctly
        assert_eq!(engine.value(b), LBool::False, "b should be False");
    }

    #[test]
    fn test_lit_value_with_all_states() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Test Undef state
        assert_eq!(engine.lit_value(&pos(a)), LBool::Undef);
        assert_eq!(engine.lit_value(&neg(a)), LBool::Undef);

        // Test True state
        engine.assert(pos(a));
        assert_eq!(engine.lit_value(&pos(a)), LBool::True);
        assert_eq!(engine.lit_value(&neg(a)), LBool::False);
    }

    #[test]
    fn test_lit_value_with_false_assignment() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Assign a = False
        engine.assert(neg(a));
        assert_eq!(engine.lit_value(&pos(a)), LBool::False);
        assert_eq!(engine.lit_value(&neg(a)), LBool::True);
    }

    #[test]
    fn test_enqueue_with_true_assignment() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Make a = True
        engine.assert(pos(a));

        // Try enqueuing pos(a) again - should return true (already True)
        let result = engine.enqueue(pos(a), None);
        assert!(result);
    }

    #[test]
    fn test_enqueue_with_false_assignment() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Make a = False
        engine.assert(neg(a));

        // Try enqueuing neg(a) again - should return true (already False)
        let result = engine.enqueue(neg(a), None);
        assert!(result);
    }

    #[test]
    fn test_conflict_restoring_pos_watches() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Create a conflict scenario
        // (a ∨ b)
        engine.add_clause(vec![pos(a), pos(b)]);
        // (¬a ∨ ¬b)
        engine.add_clause(vec![neg(a), neg(b)]);

        // Assert a = False
        engine.assert(neg(a));

        // b should be True from first clause
        assert_eq!(engine.value(b), LBool::True);
    }

    #[test]
    fn test_propagate_with_negative_literal() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();
        let c = engine.add_var();

        // Create clause with negative literals watched
        // (¬a ∨ ¬b ∨ c)
        engine.add_clause(vec![neg(a), neg(b), pos(c)]);

        // Assert a = True, which makes ¬a false
        engine.assert(pos(a));

        // Assert b = True, which makes ¬b false, forcing c = True
        engine.assert(pos(b));

        assert_eq!(engine.value(c), LBool::True);
    }

    #[test]
    fn test_engine_display_with_multiple_clauses() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Add multiple clauses to test clause output
        engine.add_clause(vec![pos(a), pos(b)]);
        engine.add_clause(vec![neg(a), pos(b)]);

        let output = format!("{}", engine);
        // Should contain variable assignments and clauses
        assert!(output.contains("b0"));
        assert!(output.contains("b1"));
        assert!(output.contains("b2"));
        assert!(output.contains("∨"));
    }

    #[test]
    fn test_get_conflict_with_learnt_clause() {
        let mut engine = Engine::new();
        let x1 = engine.add_var();
        let x2 = engine.add_var();
        let x3 = engine.add_var();
        let x4 = engine.add_var();
        let x5 = engine.add_var();
        let x6 = engine.add_var();
        let x7 = engine.add_var();
        let x8 = engine.add_var();
        let x9 = engine.add_var();

        // Use a pattern known to create conflicts (from test_conflict_analysis)
        engine.add_clause(vec![pos(x1), pos(x2)]);
        engine.add_clause(vec![pos(x1), pos(x3), pos(x7)]);
        engine.add_clause(vec![neg(x2), neg(x3), pos(x4)]);
        engine.add_clause(vec![neg(x4), pos(x5), pos(x8)]);
        engine.add_clause(vec![neg(x4), pos(x6), pos(x9)]);
        engine.add_clause(vec![neg(x5), neg(x6)]);

        engine.assert(neg(x7));
        engine.assert(neg(x8));
        engine.assert(neg(x9));
        let result = engine.assert(neg(x1));
        assert!(!result, "Should trigger conflict");

        // First call should return the explanation
        let explanation1 = engine.get_conflict_explanation();
        assert!(explanation1.is_some());

        // Second call should return None (learnt was moved)
        let explanation2 = engine.get_conflict_explanation();
        assert!(explanation2.is_none());
    }

    #[test]
    fn test_conflict_with_true_value() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Create simple conflict where a=True causes issue
        // (a ∨ b)
        engine.add_clause(vec![pos(a), pos(b)]);
        // (¬a ∨ ¬b)
        engine.add_clause(vec![neg(a), neg(b)]);

        // Assert a = True, should force b = False from second clause
        engine.assert(pos(a));
        assert_eq!(engine.value(b), LBool::False);
    }

    #[test]
    fn test_watch_negative_literal_swap() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();
        let c = engine.add_var();
        let d = engine.add_var();

        // Clause with negative literals: (¬a ∨ ¬b ∨ ¬c ∨ ¬d)
        engine.add_clause(vec![neg(a), neg(b), neg(c), neg(d)]);

        // Make a = True (so ¬a = False)
        engine.assert(pos(a));

        // Make b = True (so ¬b = False) - should move watch
        engine.assert(pos(b));

        // c and d should still be undefined
        assert_eq!(engine.value(c), LBool::Undef);
        assert_eq!(engine.value(d), LBool::Undef);

        // Make c = True - this makes the clause unit, forcing d = False
        engine.assert(pos(c));

        // d should be forced to False to satisfy the clause
        assert_eq!(engine.value(d), LBool::False);
    }

    #[test]
    fn test_clause_display_via_engine() {
        let clause = Clause { lits: vec![pos(1), neg(2), pos(3)] };

        // Test that Clause Display formats correctly
        let output = format!("{}", clause);
        assert_eq!(output, "b1 ∨ ¬b2 ∨ b3");
    }

    #[test]
    fn test_default_engine() {
        // Test that Engine::default() works
        let engine = Engine::default();
        assert_eq!(engine.value(VarId(0)), LBool::True); // Variable 0 is forced to True in new()
    }
}
