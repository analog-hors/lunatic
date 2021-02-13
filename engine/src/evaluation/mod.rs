mod standard;
pub use standard::*;

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Evaluation(i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvaluationKind {
    Centipawn(i32),
    MateIn(u8),
    MatedIn(u8)
}

impl Display for Evaluation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.kind().fmt(f)
    }
}

impl Display for EvaluationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            EvaluationKind::Centipawn(cp) => write!(f, "{}.{}", cp / 100, cp.abs() % 100),
            EvaluationKind::MateIn(m) => write!(f, "M{}", (m + 1) / 2),
            EvaluationKind::MatedIn(m) => write!(f, "-M{}", (m + 1) / 2)
        }
    }
}

impl Evaluation {
    pub const DRAW: Self = Self(0);

    pub const INFINITY: Self = Self(i32::MAX);

    pub const fn from_centipawns(centipawns: i32) -> Self {
        Self(centipawns)
    }

    pub const fn mate_in(plies_to_mate: u8) -> Self {
        Self(i32::MAX - plies_to_mate as i32)
    }

    pub const fn mated_in(plies_to_mate: u8) -> Self {
        Self(-Self::mate_in(plies_to_mate).0)
    }

    pub const fn kind(self) -> EvaluationKind {
        const MAX_MATE_IN: i32 = Evaluation::mate_in(u8::MAX).0;
        const MIN_MATE_IN: i32 = Evaluation::mate_in(u8::MIN).0;
        const MAX_MATED_IN: i32 = Evaluation::mated_in(u8::MAX).0;
        const MIN_MATED_IN: i32 = Evaluation::mated_in(u8::MIN).0;
        
        match self.0 {
            v if v >= MAX_MATE_IN => EvaluationKind::MateIn((MIN_MATE_IN - v) as u8),
            v if v <= MAX_MATED_IN => EvaluationKind::MatedIn((v - MIN_MATED_IN) as u8),
            v => EvaluationKind::Centipawn(v),
        }
    }
}

macro_rules! impl_math_ops {
    ($($trait:ident,$fn:ident,$op:tt;)*) => {
        $(
            impl std::ops::$trait for Evaluation {
                type Output = Self;
    
                fn $fn(self, other: Self) -> Self::Output {
                    Self(self.0 $op other.0)
                }
            }
        )*
    };
}
impl_math_ops! {
    Add, add, +;
    Sub, sub, -;
    Mul, mul, *;
    Div, div, /;
}

macro_rules! impl_math_assign_ops {
    ($($trait:ident,$fn:ident,$op:tt;)*) => {
        $(
            impl std::ops::$trait for Evaluation {
                fn $fn(&mut self, other: Self) {
                    self.0 $op other.0;
                }
            }
        )*
    };
}
impl_math_assign_ops! {
    AddAssign, add_assign, +=;
    SubAssign, sub_assign, -=;
    MulAssign, mul_assign, *=;
    DivAssign, div_assign, /=;
}

impl std::ops::Neg for Evaluation {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

pub trait Evaluator {
    fn evaluate(&self, board: &chess::Board, depth: u8) -> Evaluation;
}
