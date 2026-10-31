//! Propositional formulas (formal specification §2).

use std::collections::HashSet;
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
    #[must_use]
    pub fn constant(value: bool) -> Self {
        Formula::Const(value)
    }

    /// `self ∧ other`.
    #[must_use]
    pub fn and(self, other: Formula) -> Self {
        Formula::And(Box::new(self), Box::new(other))
    }

    /// `self ∨ other`.
    #[must_use]
    pub fn or(self, other: Formula) -> Self {
        Formula::Or(Box::new(self), Box::new(other))
    }

    /// `self → other`.
    #[must_use]
    pub fn implies(self, other: Formula) -> Self {
        Formula::Implies(Box::new(self), Box::new(other))
    }

    /// `self ↔ other`.
    #[must_use]
    pub fn iff(self, other: Formula) -> Self {
        Formula::Iff(Box::new(self), Box::new(other))
    }

    /// `self ⊕ other`.
    #[must_use]
    pub fn xor(self, other: Formula) -> Self {
        Formula::Xor(Box::new(self), Box::new(other))
    }

    /// Evaluates the formula, looking up variables with `value`.
    pub fn evaluate<F: Fn(&str) -> bool>(&self, value: &F) -> bool {
        self.fold(|f, args: &[bool]| match f {
            Formula::Const(c) => *c,
            Formula::Var(name) => value(name),
            Formula::Not(_) => !args[0],
            Formula::And(..) => args[0] && args[1],
            Formula::Or(..) => args[0] || args[1],
            Formula::Implies(..) => !args[0] || args[1],
            Formula::Iff(..) => args[0] == args[1],
            Formula::Xor(..) => args[0] != args[1],
        })
    }

    /// The distinct variable names, in order of first occurrence
    /// (left to right).
    #[must_use]
    pub fn variables(&self) -> Vec<&str> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut todo = vec![self];
        while let Some(f) = todo.pop() {
            if let Formula::Var(name) = f
                && seen.insert(name.as_str())
            {
                out.push(name.as_str());
            }
            let [l, r] = f.children();
            todo.extend(r);
            todo.extend(l);
        }
        out
    }

    /// The operands, left first.
    fn children(&self) -> [Option<&Formula>; 2] {
        match self {
            Formula::Const(_) | Formula::Var(_) => [None, None],
            Formula::Not(g) => [Some(g), None],
            Formula::And(l, r)
            | Formula::Or(l, r)
            | Formula::Implies(l, r)
            | Formula::Iff(l, r)
            | Formula::Xor(l, r) => [Some(l), Some(r)],
        }
    }

    /// Post-order fold without recursion, so that deeply nested formulas
    /// cannot overflow the stack. `f` gets each node and its operands'
    /// results, left first.
    pub(crate) fn fold<'a, T>(
        &'a self,
        mut f: impl FnMut(&'a Formula, &[T]) -> T,
    ) -> T {
        let mut todo = vec![(self, false)];
        let mut values: Vec<T> = Vec::new();
        while let Some((node, expanded)) = todo.pop() {
            let [l, r] = node.children();
            if !expanded && l.is_some() {
                todo.push((node, true));
                todo.extend(r.map(|r| (r, false)));
                todo.extend(l.map(|l| (l, false)));
                continue;
            }
            let start = values.len()
                - usize::from(l.is_some())
                - usize::from(r.is_some());
            let v = f(node, &values[start..]);
            values.truncate(start);
            values.push(v);
        }
        values.pop().expect("fold produces one value")
    }

    /// Moves the operands out, leaving constants behind.
    fn take_children(&mut self, out: &mut Vec<Formula>) {
        let take = |b: &mut Box<Formula>| {
            std::mem::replace(&mut **b, Formula::Const(false))
        };
        match self {
            Formula::Const(_) | Formula::Var(_) => {}
            Formula::Not(g) => out.push(take(g)),
            Formula::And(l, r)
            | Formula::Or(l, r)
            | Formula::Implies(l, r)
            | Formula::Iff(l, r)
            | Formula::Xor(l, r) => {
                out.push(take(l));
                out.push(take(r));
            }
        }
    }
}

impl Drop for Formula {
    /// Drops iteratively, so that deep formulas cannot overflow the stack.
    fn drop(&mut self) {
        let mut todo = Vec::new();
        self.take_children(&mut todo);
        while let Some(mut f) = todo.pop() {
            f.take_children(&mut todo);
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
    /// Fully parenthesized `.prop` syntax (`triple-format.md` §1.2).
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        enum Piece<'a> {
            Formula(&'a Formula),
            Text(&'static str),
        }
        let mut todo = vec![Piece::Formula(self)];
        while let Some(piece) = todo.pop() {
            let f = match piece {
                Piece::Text(s) => {
                    out.write_str(s)?;
                    continue;
                }
                Piece::Formula(f) => f,
            };
            let (l, op, r) = match f {
                Formula::Const(c) => {
                    out.write_str(if *c { "T" } else { "F" })?;
                    continue;
                }
                Formula::Var(name) => {
                    out.write_str(name)?;
                    continue;
                }
                Formula::Not(g) => {
                    out.write_str("!")?;
                    todo.push(Piece::Formula(g));
                    continue;
                }
                Formula::And(l, r) => (l, " & ", r),
                Formula::Or(l, r) => (l, " | ", r),
                Formula::Implies(l, r) => (l, " -> ", r),
                Formula::Iff(l, r) => (l, " <-> ", r),
                Formula::Xor(l, r) => (l, " ^ ", r),
            };
            out.write_str("(")?;
            todo.push(Piece::Text(")"));
            todo.push(Piece::Formula(r));
            todo.push(Piece::Text(op));
            todo.push(Piece::Formula(l));
        }
        Ok(())
    }
}
