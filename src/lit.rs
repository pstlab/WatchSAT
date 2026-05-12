use std::{cmp, fmt, ops};

/// A literal is represented as a variable index and a sign (true for positive, false for negative).
///
/// # Examples
/// ```
/// # use watchsat::{Lit, pos, neg};
/// let a = pos(0); // Represents the literal b0
/// let not_a = neg(0); // Represents the literal ¬b0
///
/// assert_eq!(a.var(), 0);
/// assert!(a.is_positive());
/// assert_eq!(not_a.var(), 0);
/// assert!(!not_a.is_positive());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lit {
    x: usize,
    sign: bool,
}

/// The literal that is always true.
pub const TRUE_LIT: Lit = Lit { x: 0, sign: true };
/// The literal that is always false.
pub const FALSE_LIT: Lit = Lit { x: 0, sign: false };

impl Lit {
    /// Creates a literal from a variable index and a sign.
    pub fn new(x: usize, sign: bool) -> Self {
        Lit { x, sign }
    }

    /// Creates a positive literal for the given variable index.
    pub fn pos(x: usize) -> Self {
        Lit { x, sign: true }
    }

    /// Creates a negative literal for the given variable index.
    pub fn neg(x: usize) -> Self {
        Lit { x, sign: false }
    }

    /// Returns the variable index associated with this literal.
    pub fn var(&self) -> usize {
        self.x
    }

    /// Returns `true` when the literal is positive.
    pub fn is_positive(&self) -> bool {
        self.sign
    }
}

/// Creates a positive literal for the given variable index.
pub fn pos(x: usize) -> Lit {
    Lit::pos(x)
}

/// Creates a negative literal for the given variable index.
pub fn neg(x: usize) -> Lit {
    Lit::neg(x)
}

impl Default for Lit {
    /// Returns a sentinel literal with an invalid variable index.
    fn default() -> Self {
        Lit { x: usize::MAX, sign: false }
    }
}

impl fmt::Display for Lit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.sign {
            true => write!(f, "b{}", self.x),
            false => write!(f, "¬b{}", self.x),
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
        let positive = Lit::new(3, true);
        let negative = Lit::new(3, false);

        assert_eq!(positive.var(), 3);
        assert!(positive.is_positive());
        assert_eq!(negative.var(), 3);
        assert!(!negative.is_positive());
    }

    #[test]
    fn negation_flips_the_sign_only() {
        let literal = Lit::new(7, true);

        assert_eq!(!literal, Lit::new(7, false));
        assert_eq!(!&literal, Lit::new(7, false));
    }

    #[test]
    fn display_uses_expected_symbol() {
        assert_eq!(format!("{}", Lit::new(2, true)), "b2");
        assert_eq!(format!("{}", Lit::new(2, false)), "¬b2");
    }

    #[test]
    fn partial_order_compares_variable_first_then_sign() {
        let a = Lit::new(1, false);
        let b = Lit::new(1, true);
        let c = Lit::new(2, false);

        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }

    #[test]
    fn default_literal_uses_sentinel_values() {
        let literal = Lit::default();

        assert_eq!(literal.var(), usize::MAX);
        assert!(!literal.is_positive());
    }
}
