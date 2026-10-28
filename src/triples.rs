//! Structural decomposition into triples (formal specification §3).

use std::collections::HashMap;
use std::ops::Not;

use crate::formula::Formula;

/// A literal: a node together with a polarity (SPEC-D2).
///
/// Node 0 is the constant ⊤, so [`Lit::TRUE`] is node 0 and
/// [`Lit::FALSE`] is its negation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lit(u32);

impl Lit {
    /// The constant ⊤.
    pub const TRUE: Lit = Lit(0);
    /// The constant ⊥.
    pub const FALSE: Lit = Lit(1);

    /// The literal `node` (`negated == false`) or `¬node`.
    ///
    /// # Panics
    /// If `node >= 2^31`.
    pub fn new(node: u32, negated: bool) -> Lit {
        assert!(node < 1 << 31, "node id {node} out of range");
        Lit(node << 1 | negated as u32)
    }

    /// The literal's node.
    pub fn node(self) -> u32 {
        self.0 >> 1
    }

    /// Whether the literal is negated.
    pub fn is_negated(self) -> bool {
        self.0 & 1 == 1
    }

    /// The integer encoding `2·node + negated`.
    pub fn code(self) -> u32 {
        self.0
    }
}

impl Not for Lit {
    type Output = Lit;

    fn not(self) -> Lit {
        Lit(self.0 ^ 1)
    }
}

/// The two triple kinds (SPEC-D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TripleKind {
    /// `x ↔ (a ∧ b)`.
    And,
    /// `x ↔ (a ↔ b)`.
    Eq,
}

/// A triple `x ↔ (a ∘ b)` with output node `x` and operand literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Triple {
    /// The connective.
    pub kind: TripleKind,
    /// The output node.
    pub out: u32,
    /// The first operand (`a <= b`).
    pub a: Lit,
    /// The second operand.
    pub b: Lit,
}

impl Triple {
    /// Whether the triple holds when nodes take the values `value`.
    pub fn holds(&self, value: impl Fn(u32) -> bool) -> bool {
        let lit = |l: Lit| value(l.node()) != l.is_negated();
        let (a, b) = (lit(self.a), lit(self.b));
        let rhs = match self.kind {
            TripleKind::And => a && b,
            TripleKind::Eq => a == b,
        };
        value(self.out) == rhs
    }
}

/// A formula decomposed into triples.
///
/// Nodes are numbered `0..node_count()`: node 0 is ⊤, each variable gets a
/// node on its first occurrence (left to right), and each triple gets a fresh
/// output node when it is created. Triples are therefore listed in
/// topological order.
#[derive(Clone, Debug)]
pub struct Triples {
    node_count: u32,
    variables: Vec<(String, u32)>,
    triples: Vec<Triple>,
    root: Lit,
}

impl Triples {
    /// Decomposes `formula` (SPEC-D4). With `sharing`, identical
    /// subformulas, after normalization, share one triple (SPEC-D5).
    pub fn from_formula(formula: &Formula, sharing: bool) -> Triples {
        let mut b = Builder {
            node_count: 1,
            variables: Vec::new(),
            var_index: HashMap::new(),
            triples: Vec::new(),
            shared: HashMap::new(),
            sharing,
        };
        let root = b.lit(formula);
        Triples {
            node_count: b.node_count,
            variables: b.variables,
            triples: b.triples,
            root,
        }
    }

    /// Number of nodes, including ⊤.
    pub fn node_count(&self) -> u32 {
        self.node_count
    }

    /// The input variables and their nodes, in node order.
    pub fn variables(&self) -> &[(String, u32)] {
        &self.variables
    }

    /// The triples, in topological order.
    pub fn triples(&self) -> &[Triple] {
        &self.triples
    }

    /// The literal standing for the whole formula.
    pub fn root(&self) -> Lit {
        self.root
    }

    /// Values of all nodes when the variables take `value`, computed
    /// bottom-up through the triples.
    pub fn evaluate(&self, value: impl Fn(&str) -> bool) -> Vec<bool> {
        let mut val = vec![false; self.node_count as usize];
        val[0] = true;
        for (name, node) in &self.variables {
            val[*node as usize] = value(name);
        }
        for t in &self.triples {
            let lit = |l: Lit| val[l.node() as usize] != l.is_negated();
            let (a, b) = (lit(t.a), lit(t.b));
            val[t.out as usize] = match t.kind {
                TripleKind::And => a && b,
                TripleKind::Eq => a == b,
            };
        }
        val
    }
}

struct Builder<'f> {
    node_count: u32,
    variables: Vec<(String, u32)>,
    var_index: HashMap<&'f str, u32>,
    triples: Vec<Triple>,
    shared: HashMap<(TripleKind, Lit, Lit), Lit>,
    sharing: bool,
}

impl<'f> Builder<'f> {
    fn fresh(&mut self) -> u32 {
        let n = self.node_count;
        self.node_count =
            n.checked_add(1).filter(|&c| c < 1 << 31).expect("too many nodes");
        n
    }

    fn lit(&mut self, f: &'f Formula) -> Lit {
        match f {
            Formula::Const(true) => Lit::TRUE,
            Formula::Const(false) => Lit::FALSE,
            Formula::Var(name) => {
                if let Some(&n) = self.var_index.get(name.as_str()) {
                    return Lit::new(n, false);
                }
                let n = self.fresh();
                self.var_index.insert(name, n);
                self.variables.push((name.clone(), n));
                Lit::new(n, false)
            }
            Formula::Not(g) => !self.lit(g),
            Formula::And(g, h) => {
                let (a, b) = (self.lit(g), self.lit(h));
                self.mk(TripleKind::And, a, b)
            }
            Formula::Or(g, h) => {
                let (a, b) = (self.lit(g), self.lit(h));
                !self.mk(TripleKind::And, !a, !b)
            }
            Formula::Implies(g, h) => {
                let (a, b) = (self.lit(g), self.lit(h));
                !self.mk(TripleKind::And, a, !b)
            }
            Formula::Iff(g, h) => {
                let (a, b) = (self.lit(g), self.lit(h));
                self.mk(TripleKind::Eq, a, b)
            }
            Formula::Xor(g, h) => {
                let (a, b) = (self.lit(g), self.lit(h));
                !self.mk(TripleKind::Eq, a, b)
            }
        }
    }

    /// SPEC-D5: EQ operand signs move to the output, operands are sorted,
    /// and identical triples are shared.
    fn mk(&mut self, kind: TripleKind, a: Lit, b: Lit) -> Lit {
        let (a, b, flip) = match kind {
            TripleKind::And => (a, b, false),
            TripleKind::Eq => (
                Lit::new(a.node(), false),
                Lit::new(b.node(), false),
                a.is_negated() != b.is_negated(),
            ),
        };
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let out = match self.shared.get(&(kind, a, b)) {
            Some(&out) if self.sharing => out,
            _ => {
                let out = Lit::new(self.fresh(), false);
                self.triples.push(Triple { kind, out: out.node(), a, b });
                self.shared.insert((kind, a, b), out);
                out
            }
        };
        if flip { !out } else { out }
    }
}
