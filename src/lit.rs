use std::{cmp, fmt, ops};

use crate::VarId;

/// A literal is represented as a variable index and a sign (true for positive, false for negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lit {
    x: VarId,
    sign: bool,
}

/// The literal that is always true.
pub const TRUE_LIT: Lit = Lit { x: VarId::new(0), sign: true };
/// The literal that is always false.
pub const FALSE_LIT: Lit = Lit { x: VarId::new(0), sign: false };

impl Lit {
    /// Creates a literal from a variable index and a sign.
    ///
    /// `sign = true` means positive literal, `sign = false` means negated literal.
    pub fn new(x: VarId, sign: bool) -> Self {
        Lit { x, sign }
    }

    /// Creates a positive literal for the given variable index.
    pub fn pos(x: VarId) -> Self {
        Lit { x, sign: true }
    }

    /// Creates a negative literal for the given variable index.
    pub fn neg(x: VarId) -> Self {
        Lit { x, sign: false }
    }

    /// Returns the variable index associated with this literal.
    pub fn var(&self) -> VarId {
        self.x
    }

    /// Returns `true` when the literal is positive.
    pub fn is_positive(&self) -> bool {
        self.sign
    }
}

/// Creates a positive literal for the given variable index.
pub fn pos(x: VarId) -> Lit {
    Lit::pos(x)
}

/// Creates a negative literal for the given variable index.
pub fn neg(x: VarId) -> Lit {
    Lit::neg(x)
}

impl Default for Lit {
    /// Returns a sentinel literal with an invalid variable index.
    ///
    /// This value is used internally as a temporary placeholder.
    fn default() -> Self {
        Lit { x: VarId::new(usize::MAX), sign: false }
    }
}

impl fmt::Display for Lit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.sign {
            true => write!(f, "{}", self.x),
            false => write!(f, "¬{}", self.x),
        }
    }
}

impl ops::Not for Lit {
    type Output = Lit;

    fn not(self) -> Lit {
        Lit { x: self.x, sign: !self.sign }
    }
}

impl ops::Not for &Lit {
    type Output = Lit;

    fn not(self) -> Lit {
        Lit { x: self.x, sign: !self.sign }
    }
}

impl PartialOrd for Lit {
    fn partial_cmp(&self, other: &Lit) -> Option<cmp::Ordering> {
        match self.x.partial_cmp(&other.x) {
            Some(cmp::Ordering::Equal) => self.sign.partial_cmp(&other.sign),
            ord => ord,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_variable_and_sign() {
        let positive = Lit::new(VarId::new(3), true);
        let negative = Lit::new(VarId::new(3), false);

        assert_eq!(positive.var(), VarId::new(3));
        assert!(positive.is_positive());
        assert_eq!(negative.var(), VarId::new(3));
        assert!(!negative.is_positive());
    }

    #[test]
    fn negation_flips_the_sign_only() {
        let literal = Lit::new(VarId::new(7), true);

        assert_eq!(!literal, Lit::new(VarId::new(7), false));
        assert_eq!(!&literal, Lit::new(VarId::new(7), false));
    }

    #[test]
    fn display_uses_expected_symbol() {
        assert_eq!(format!("{}", Lit::new(VarId::new(2), true)), "b2");
        assert_eq!(format!("{}", Lit::new(VarId::new(2), false)), "¬b2");
    }

    #[test]
    fn partial_order_compares_variable_first_then_sign() {
        let a = Lit::new(VarId::new(1), false);
        let b = Lit::new(VarId::new(1), true);
        let c = Lit::new(VarId::new(2), false);

        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }

    #[test]
    fn default_literal_uses_sentinel_values() {
        let literal = Lit::default();

        assert_eq!(literal.var(), VarId::new(usize::MAX));
        assert!(!literal.is_positive());
    }
}
