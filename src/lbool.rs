use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum LBool {
    True,
    False,
    #[default]
    Undef,
}

impl fmt::Display for LBool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            LBool::True => "True",
            LBool::False => "False",
            LBool::Undef => "Undef",
        };
        write!(f, "{}", s)
    }
}
