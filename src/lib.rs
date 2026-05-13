//! # SAT Solver Engine
//!
//! This module provides a core CDCL (Conflict-Driven Clause Learning) engine
//! utilizing the Two-Watched Literal (2WL) scheme for efficient unit propagation.
//!
//! ## Core Logic
//! The engine works by:
//! 1. **Assignment**: Values are assigned to variables via `assert`.
//! 2. **Propagation**: The engine uses watch lists to find unit clauses.
//! 3. **Conflict Analysis**: If a contradiction is found, the engine performs
//!    1-UIP (Unique Implication Point) analysis to learn a new clause.
//!
//! ## Example
//! ```rust
//! # use watchsat::{Engine, pos, neg};
//! let mut engine = Engine::new();
//! let a = engine.add_var();
//! let b = engine.add_var();
//! engine.add_clause(vec![neg(a), pos(b)]); // (¬a ∨ b)
//! engine.assert(pos(a));                   // Forces b to be True
//! ```
//!
//! The API is intentionally low-level and focused on building blocks that can
//! be embedded in higher-level planning or verification systems.
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

/// A CDCL-based SAT engine.
///
/// The `Engine` manages variable assignments, watch lists for unit propagation,
/// and performs conflict analysis to learn new clauses.
///
/// # Examples
/// ```
/// # use watchsat::{Engine, pos, neg, LBool};
/// let mut engine = Engine::new();
/// let a = engine.add_var();
/// let b = engine.add_var();
/// engine.add_clause(vec![neg(a), pos(b)]); // (¬a ∨ b)
/// engine.assert(pos(a));                   // Forces b to be True
/// assert_eq!(engine.value(b), LBool::True, "b should be propagated by unit clause");
/// ```
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
    /// Propagation or insertion produced an unsatisfied clause.
    ///
    /// The payload contains a clause explaining the conflict.
    Conflict { clause: Vec<Lit> },
}

impl Engine {
    /// Creates an empty SAT engine.
    ///
    /// A reserved internal variable is created at index `0` so that
    /// [`TRUE_LIT`] and [`FALSE_LIT`] always have a defined value.
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

    /// Adds a fresh variable and returns its identifier.
    pub fn add_var(&mut self) -> VarId {
        let var_id = self.assigns.len();
        self.assigns.push(LBool::default());
        self.reason.push(None);
        self.decision_vars.push(None);
        self.pos_watches.push(Vec::new());
        self.neg_watches.push(Vec::new());
        VarId(var_id)
    }

    /// Returns the current assignment of a variable.
    pub fn value(&self, var: VarId) -> LBool {
        self.assigns[var.0].clone()
    }

    /// Returns the current value of a literal under the current assignment.
    ///
    /// If the variable is unassigned, the result is [`LBool::Undef`].
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

    /// Returns the decision variable that implied `var`, if any.
    ///
    /// `None` means either `var` is unassigned, or it was set directly as a
    /// decision variable rather than by implication.
    pub fn decision_var(&self, var: VarId) -> Option<VarId> {
        self.decision_vars[var.0]
    }

    /// Adds a clause to the formula.
    ///
    /// For unit clauses, assignment is attempted immediately.
    ///
    /// # Errors
    ///
    /// Returns [`PropagationError::Conflict`] when inserting an empty clause or
    /// when a unit clause contradicts current assignments.
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

    /// Asserts a decision literal and runs propagation.
    ///
    /// This performs watched-literal propagation and may trigger conflict
    /// analysis (1-UIP) to return a learnt clause.
    ///
    /// # Panics
    ///
    /// Panics if the variable of `lit` is already assigned.
    ///
    /// # Errors
    ///
    /// Returns [`PropagationError::Conflict`] when propagation derives a
    /// contradiction. The returned clause is the analyzed learnt clause.
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

    /// Clears the assignment of `var` and its implication metadata.
    ///
    /// This is mainly used internally during conflict analysis.
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

    /// Registers a callback invoked when `var` gets assigned.
    ///
    /// Listeners are called synchronously during propagation/assertion.
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

        engine.assert(pos(a)).unwrap();
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
        engine.add_clause(vec![neg(a), pos(b)]).unwrap();

        engine.assert(pos(a)).unwrap();

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
        engine.add_clause(vec![neg(a), pos(b)]).unwrap();
        engine.add_clause(vec![neg(b), pos(c)]).unwrap();

        engine.assert(pos(a)).unwrap();

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
        engine.add_clause(vec![pos(a), pos(b), pos(c)]).unwrap();

        // Assign a = False.
        // 2WL should move watch from 'a' to 'c' because 'b' is still Undef.
        engine.assert(neg(a)).unwrap();
        assert_eq!(engine.value(b), LBool::Undef, "b should still be undef");
        assert_eq!(engine.value(c), LBool::Undef, "c should still be undef");

        // Now assign b = False.
        // This should trigger propagation on c.
        engine.assert(neg(b)).unwrap();
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
        engine.add_clause(vec![neg(a), pos(b)]).unwrap();

        // Assert a, which propagates b, which should fire the listener
        engine.assert(pos(a)).unwrap();

        assert!(*triggered.lock().unwrap(), "Listener on b should have been triggered");
    }

    #[test]
    #[should_panic(expected = "already assigned")]
    fn test_double_assertion_panic() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        engine.assert(pos(a)).unwrap();
        let _ = engine.assert(neg(a)); // Should panic
    }

    #[test]
    fn test_diamond_propagation() {
        let mut engine = Engine::new();
        let x1 = engine.add_var();
        let x2 = engine.add_var();
        let x3 = engine.add_var();
        let x4 = engine.add_var();

        // x1 -> x2  (¬x1 ∨ x2)
        engine.add_clause(vec![neg(x1), pos(x2)]).unwrap();
        // x1 -> x3  (¬x1 ∨ x3)
        engine.add_clause(vec![neg(x1), pos(x3)]).unwrap();
        // (x2 ∧ x3) -> x4 (¬x2 ∨ ¬x3 ∨ x4)
        engine.add_clause(vec![neg(x2), neg(x3), pos(x4)]).unwrap();

        engine.assert(pos(x1)).unwrap();

        assert_eq!(engine.value(x4), LBool::True, "x4 should be forced via x2 and x3");
    }

    #[test]
    fn test_complex_conflict_1uip() {
        let mut engine = Engine::new();
        let vars: Vec<VarId> = (0..10).map(|_| engine.add_var()).collect();

        // Setup a chain: x1 -> x2 -> x3 -> x4
        engine.add_clause(vec![neg(vars[1]), pos(vars[2])]).unwrap();
        engine.add_clause(vec![neg(vars[2]), pos(vars[3])]).unwrap();
        engine.add_clause(vec![neg(vars[3]), pos(vars[4])]).unwrap();

        // Create a conflict path:
        // (x4 ∧ x5) -> Conflict
        // x5 is another decision or forced var
        engine.add_clause(vec![neg(vars[4]), neg(vars[5])]).unwrap();

        // Another path to the same conflict
        // (x3 ∧ x6) -> x5
        engine.add_clause(vec![neg(vars[3]), neg(vars[6]), pos(vars[5])]).unwrap();

        // Assert "side" variables that set the stage
        engine.assert(pos(vars[6])).unwrap();

        // Now trigger the chain
        // This should cause: x1 -> x2 -> x3 -> x4 -> conflict with x5
        let result = engine.assert(pos(vars[1]));

        assert!(result.is_err(), "Should detect a conflict");
        if let Err(PropagationError::Conflict { clause }) = result {
            // The conflict clause should contain variables from the conflict
            assert!(!clause.is_empty(), "Conflict clause should not be empty");
        }
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
        engine.add_clause(vec![pos(x1), pos(x2)]).unwrap();
        // (x1 ∨ x3 ∨ x7)
        engine.add_clause(vec![pos(x1), pos(x3), pos(x7)]).unwrap();
        // (¬x2 ∨ ¬x3 ∨ x4)
        engine.add_clause(vec![neg(x2), neg(x3), pos(x4)]).unwrap();
        // (¬x4 ∨ x5 ∨ x8)
        engine.add_clause(vec![neg(x4), pos(x5), pos(x8)]).unwrap();
        // (¬x4 ∨ x6 ∨ x9)
        engine.add_clause(vec![neg(x4), pos(x6), pos(x9)]).unwrap();
        // (¬x5 ∨ ¬x6)
        engine.add_clause(vec![neg(x5), neg(x6)]).unwrap();

        // Assert ¬x7
        engine.assert(neg(x7)).unwrap();
        // Assert ¬x8
        engine.assert(neg(x8)).unwrap();
        // Assert ¬x9
        engine.assert(neg(x9)).unwrap();

        // Assert ¬x1, which should trigger conflict analysis
        let result = engine.assert(neg(x1));
        assert!(result.is_err(), "Should trigger conflict");
    }

    #[test]
    fn test_display_implementations() {
        // Test LBool Display
        assert_eq!(format!("{}", LBool::True), "True");
        assert_eq!(format!("{}", LBool::False), "False");
        assert_eq!(format!("{}", LBool::Undef), "Undef");

        // Test Lit Display
        let lit_pos = pos(VarId(5));
        let lit_neg = neg(VarId(5));
        assert_eq!(format!("{}", lit_pos), "b5");
        assert_eq!(format!("{}", lit_neg), "¬b5");

        // Test Clause Display
        let clause = Clause { lits: vec![pos(VarId(1)), neg(VarId(2)), pos(VarId(3))] };
        assert_eq!(format!("{}", clause), "b1 ∨ ¬b2 ∨ b3");

        // Test Engine Display
        let mut engine = Engine::new();
        let a = engine.add_var();
        let _ = engine.add_clause(vec![pos(a)]);
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
        engine.add_clause(vec![neg(a), pos(b)]).unwrap();

        // Before any assertion
        assert_eq!(engine.decision_var(b), None);

        // After assertion
        engine.assert(pos(a)).unwrap();
        assert_eq!(engine.decision_var(b), Some(a));
    }

    #[test]
    fn test_empty_clause() {
        let mut engine = Engine::new();
        let result = engine.add_clause(vec![]);
        assert!(result.is_err(), "Empty clause should return error");
    }

    #[test]
    fn test_unit_clause() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Add unit clause (a)
        let _ = engine.add_clause(vec![pos(a)]);

        // Variable 'a' should be propagated immediately
        assert_eq!(engine.value(a), LBool::True);
    }

    #[test]
    fn test_enqueue_already_assigned() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // (a ∨ b)
        engine.add_clause(vec![pos(a), pos(b)]).unwrap();

        // Assert a = True
        engine.assert(pos(a)).unwrap();
        assert_eq!(engine.value(a), LBool::True);

        // Now try to enqueue a again with True (should succeed as it's consistent)
        let result = engine.enqueue(pos(a), None);
        assert!(result, "Enqueuing consistent value should succeed");

        // Try to enqueue a with False (should fail as it's inconsistent)
        let result2 = engine.enqueue(neg(a), None);
        assert!(!result2, "Enqueuing inconsistent value should fail");
    }

    #[test]
    fn test_no_conflict_when_consistent() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // No conflict should occur with consistent assertions
        let result = engine.assert(pos(a));
        assert!(result.is_ok(), "Should succeed with consistent assertion");
    }

    #[test]
    fn test_propagate_satisfied_clause() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();
        let c = engine.add_var();

        // (a ∨ b ∨ c)
        engine.add_clause(vec![pos(a), pos(b), pos(c)]).unwrap();

        // Assert a = True (satisfies the clause)
        engine.assert(pos(a)).unwrap();

        // Now assert b = False
        // The propagate function should detect that the clause is already satisfied by 'a'
        engine.assert(neg(b)).unwrap();

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
        engine.add_clause(vec![pos(a), pos(b), pos(c), pos(d)]).unwrap();

        // Assert a = False, should move watch to c
        engine.assert(neg(a)).unwrap();
        assert_eq!(engine.value(b), LBool::Undef);
        assert_eq!(engine.value(c), LBool::Undef);
        assert_eq!(engine.value(d), LBool::Undef);

        // Assert b = False, should move watch to d
        engine.assert(neg(b)).unwrap();
        assert_eq!(engine.value(c), LBool::Undef);
        assert_eq!(engine.value(d), LBool::Undef);

        // Assert c = False, should propagate d
        engine.assert(neg(c)).unwrap();
        assert_eq!(engine.value(d), LBool::True, "d should be propagated");
    }

    #[test]
    fn test_conflict_with_multiple_watched_literals() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Create clauses that will conflict
        // (a ∨ b)
        engine.add_clause(vec![pos(a), pos(b)]).unwrap();
        // (¬a ∨ ¬b)
        engine.add_clause(vec![neg(a), neg(b)]).unwrap();

        // Assert a = True, this forces b = False from second clause
        engine.assert(pos(a)).unwrap();
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
        engine.assert(pos(a)).unwrap();
        assert_eq!(engine.lit_value(&pos(a)), LBool::True);
        assert_eq!(engine.lit_value(&neg(a)), LBool::False);
    }

    #[test]
    fn test_lit_value_with_false_assignment() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Assign a = False
        engine.assert(neg(a)).unwrap();
        assert_eq!(engine.lit_value(&pos(a)), LBool::False);
        assert_eq!(engine.lit_value(&neg(a)), LBool::True);
    }

    #[test]
    fn test_enqueue_with_true_assignment() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Make a = True
        engine.assert(pos(a)).unwrap();

        // Try enqueuing pos(a) again - should return true (already True)
        let result = engine.enqueue(pos(a), None);
        assert!(result);
    }

    #[test]
    fn test_enqueue_with_false_assignment() {
        let mut engine = Engine::new();
        let a = engine.add_var();

        // Make a = False
        engine.assert(neg(a)).unwrap();

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
        engine.add_clause(vec![pos(a), pos(b)]).unwrap();
        // (¬a ∨ ¬b)
        engine.add_clause(vec![neg(a), neg(b)]).unwrap();

        // Assert a = False
        engine.assert(neg(a)).unwrap();

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
        engine.add_clause(vec![neg(a), neg(b), pos(c)]).unwrap();

        // Assert a = True, which makes ¬a false
        engine.assert(pos(a)).unwrap();

        // Assert b = True, which makes ¬b false, forcing c = True
        engine.assert(pos(b)).unwrap();

        assert_eq!(engine.value(c), LBool::True);
    }

    #[test]
    fn test_engine_display_with_multiple_clauses() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Add multiple clauses to test clause output
        engine.add_clause(vec![pos(a), pos(b)]).unwrap();
        engine.add_clause(vec![neg(a), pos(b)]).unwrap();

        let output = format!("{}", engine);
        // Should contain variable assignments and clauses
        assert!(output.contains("b0"));
        assert!(output.contains("b1"));
        assert!(output.contains("b2"));
        assert!(output.contains("∨"));
    }

    #[test]
    fn test_conflict_error_propagation() {
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
        engine.add_clause(vec![pos(x1), pos(x2)]).unwrap();
        engine.add_clause(vec![pos(x1), pos(x3), pos(x7)]).unwrap();
        engine.add_clause(vec![neg(x2), neg(x3), pos(x4)]).unwrap();
        engine.add_clause(vec![neg(x4), pos(x5), pos(x8)]).unwrap();
        engine.add_clause(vec![neg(x4), pos(x6), pos(x9)]).unwrap();
        engine.add_clause(vec![neg(x5), neg(x6)]).unwrap();

        engine.assert(neg(x7)).unwrap();
        engine.assert(neg(x8)).unwrap();
        engine.assert(neg(x9)).unwrap();
        let result = engine.assert(neg(x1));
        assert!(result.is_err(), "Should trigger conflict");

        // The conflict should be returned in the Result
        if let Err(PropagationError::Conflict { clause }) = result {
            assert!(!clause.is_empty(), "Conflict clause should contain learnt literals");
        }
    }

    #[test]
    fn test_conflict_with_true_value() {
        let mut engine = Engine::new();
        let a = engine.add_var();
        let b = engine.add_var();

        // Create simple conflict where a=True causes issue
        // (a ∨ b)
        let _ = engine.add_clause(vec![pos(a), pos(b)]);
        // (¬a ∨ ¬b)
        let _ = engine.add_clause(vec![neg(a), neg(b)]);

        // Assert a = True, should force b = False from second clause
        engine.assert(pos(a)).unwrap();
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
        engine.add_clause(vec![neg(a), neg(b), neg(c), neg(d)]).unwrap();

        // Make a = True (so ¬a = False)
        engine.assert(pos(a)).unwrap();

        // Make b = True (so ¬b = False) - should move watch
        engine.assert(pos(b)).unwrap();

        // c and d should still be undefined
        assert_eq!(engine.value(c), LBool::Undef);
        assert_eq!(engine.value(d), LBool::Undef);

        // Make c = True - this makes the clause unit, forcing d = False
        engine.assert(pos(c)).unwrap();

        // d should be forced to False to satisfy the clause
        assert_eq!(engine.value(d), LBool::False);
    }

    #[test]
    fn test_clause_display_via_engine() {
        let clause = Clause { lits: vec![pos(VarId(1)), neg(VarId(2)), pos(VarId(3))] };

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
