//! Propositional formulas (formal specification §2).

use std::fmt;
use std::ops::{BitAnd, BitOr, BitXor, Not};

/// A propositional formula over named variables.
///
/// Formulas can be built with the constructors and combinators below, or
/// with the operators `!`, `&`, `|` and `^`.
///
/// ```
/// use stalmarck::Formula;
///
/// let (a, b) = (Formula::var("a"), Formula::var("b"));
/// let f = (a.clone() & b.clone()).implies(a | b);
/// assert_eq!(f.to_string(), "((a & b) -> (a | b))");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Formula {
    /// The constant ⊤ (`true`) or ⊥ (`false`).
    Const(bool),
    /// A propositional variable.
    Var(String),
    /// Negation.
    Not(Box<Formula>),
    /// Conjunction.
    And(Box<Formula>, Box<Formula>),
    /// Disjunction.
    Or(Box<Formula>, Box<Formula>),
    /// Implication.
    Implies(Box<Formula>, Box<Formula>),
    /// Biconditional.
    Iff(Box<Formula>, Box<Formula>),
    /// Exclusive or.
    Xor(Box<Formula>, Box<Formula>),
}

impl Formula {
    /// A variable named `name`.
    pub fn var(name: impl Into<String>) -> Self {
        Formula::Var(name.into())
    }

    /// The constant `value`.
    pub fn constant(value: bool) -> Self {
        Formula::Const(value)
    }

    /// `self ∧ other`.
    pub fn and(self, other: Formula) -> Self {
        Formula::And(Box::new(self), Box::new(other))
    }

    /// `self ∨ other`.
    pub fn or(self, other: Formula) -> Self {
        Formula::Or(Box::new(self), Box::new(other))
    }

    /// `self → other`.
    pub fn implies(self, other: Formula) -> Self {
        Formula::Implies(Box::new(self), Box::new(other))
    }

    /// `self ↔ other`.
    pub fn iff(self, other: Formula) -> Self {
        Formula::Iff(Box::new(self), Box::new(other))
    }

    /// `self ⊕ other`.
    pub fn xor(self, other: Formula) -> Self {
        Formula::Xor(Box::new(self), Box::new(other))
    }

    /// Evaluates the formula, looking up variables with `value`.
    pub fn evaluate<F: Fn(&str) -> bool>(&self, value: &F) -> bool {
        match self {
            Formula::Const(c) => *c,
            Formula::Var(name) => value(name),
            Formula::Not(f) => !f.evaluate(value),
            Formula::And(f, g) => f.evaluate(value) && g.evaluate(value),
            Formula::Or(f, g) => f.evaluate(value) || g.evaluate(value),
            Formula::Implies(f, g) => !f.evaluate(value) || g.evaluate(value),
            Formula::Iff(f, g) => f.evaluate(value) == g.evaluate(value),
            Formula::Xor(f, g) => f.evaluate(value) != g.evaluate(value),
        }
    }

    /// The distinct variable names, in order of first occurrence
    /// (left to right).
    pub fn variables(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_variables(&mut out);
        out
    }

    fn collect_variables<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Formula::Const(_) => {}
            Formula::Var(name) => {
                if !out.contains(&name.as_str()) {
                    out.push(name);
                }
            }
            Formula::Not(f) => f.collect_variables(out),
            Formula::And(f, g)
            | Formula::Or(f, g)
            | Formula::Implies(f, g)
            | Formula::Iff(f, g)
            | Formula::Xor(f, g) => {
                f.collect_variables(out);
                g.collect_variables(out);
            }
        }
    }
}

impl Not for Formula {
    type Output = Formula;

    fn not(self) -> Formula {
        Formula::Not(Box::new(self))
    }
}

impl BitAnd for Formula {
    type Output = Formula;

    fn bitand(self, other: Formula) -> Formula {
        self.and(other)
    }
}

impl BitOr for Formula {
    type Output = Formula;

    fn bitor(self, other: Formula) -> Formula {
        self.or(other)
    }
}

impl BitXor for Formula {
    type Output = Formula;

    fn bitxor(self, other: Formula) -> Formula {
        self.xor(other)
    }
}

impl fmt::Display for Formula {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (op, l, r) = match self {
            Formula::Const(true) => return write!(f, "T"),
            Formula::Const(false) => return write!(f, "F"),
            Formula::Var(name) => return write!(f, "{name}"),
            Formula::Not(g) => return write!(f, "!{g}"),
            Formula::And(l, r) => ("&", l, r),
            Formula::Or(l, r) => ("|", l, r),
            Formula::Implies(l, r) => ("->", l, r),
            Formula::Iff(l, r) => ("<->", l, r),
            Formula::Xor(l, r) => ("^", l, r),
        };
        write!(f, "({l} {op} {r})")
    }
}
