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

pub const TRUE_LIT: Lit = Lit { x: 0, sign: true };
pub const FALSE_LIT: Lit = Lit { x: 0, sign: false };

impl Lit {
    pub fn new(x: usize, sign: bool) -> Self {
        Lit { x, sign }
    }

    pub fn pos(x: usize) -> Self {
        Lit { x, sign: true }
    }

    pub fn neg(x: usize) -> Self {
        Lit { x, sign: false }
    }

    pub fn var(&self) -> usize {
        self.x
    }

    pub fn is_positive(&self) -> bool {
        self.sign
    }
}

pub fn pos(x: usize) -> Lit {
    Lit::pos(x)
}

pub fn neg(x: usize) -> Lit {
    Lit::neg(x)
}

impl Default for Lit {
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
