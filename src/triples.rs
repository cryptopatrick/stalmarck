//! Structural decomposition into triples (formal specification §3).

use std::collections::HashMap;
use std::ops::Not;

use crate::cnf::Cnf;
use crate::formula::Formula;
use crate::parse::ParseError;

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
        let mut b = Builder::new(sharing);
        let root = b.lit(formula);
        b.finish(root)
    }

    /// Decomposes a CNF (`triple-format.md` §2): variable `i` is node `i`
    /// and is named `x<i>`.
    pub fn from_cnf(cnf: &Cnf, sharing: bool) -> Triples {
        let mut b = Builder::new(sharing);
        for i in 1..=cnf.variables {
            b.var(&format!("x{i}"));
        }
        let lit = |l: i32| Lit::new(l.unsigned_abs(), l < 0);
        let mut root: Option<Lit> = None;
        for clause in &cnf.clauses {
            let mut c: Option<Lit> = None;
            for &l in clause {
                c = Some(match c {
                    None => lit(l),
                    Some(c) => !b.mk(TripleKind::And, !c, !lit(l)),
                });
            }
            let c = c.unwrap_or(Lit::FALSE);
            root = Some(match root {
                None => c,
                Some(r) => b.mk(TripleKind::And, r, c),
            });
        }
        b.finish(root.unwrap_or(Lit::TRUE))
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

    /// The `.tri` text form (`triple-format.md` §3).
    pub fn to_text(&self) -> String {
        let mut s = format!(
            "stalmarck-triples 1\nnodes {}\nvars {}\n",
            self.node_count,
            self.variables.len()
        );
        for (name, node) in &self.variables {
            s += &format!("v {node} {name}\n");
        }
        s += &format!("triples {}\n", self.triples.len());
        for t in &self.triples {
            let kind = match t.kind {
                TripleKind::And => "and",
                TripleKind::Eq => "eq",
            };
            s += &format!("t {kind} {} {} {}\n", t.out, t.a.0, t.b.0);
        }
        s += &format!("root {}\n", self.root.0);
        s
    }

    /// Reads the `.tri` text form, checking its validity rules
    /// (`triple-format.md` §3).
    pub fn parse_text(src: &str) -> Result<Triples, ParseError> {
        let mut lines = src
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.trim()))
            .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'));
        let mut last = 1;
        let mut next = |key: &str, arity: usize| {
            let (n, l) = lines.next().ok_or_else(|| {
                ParseError::new(last, 1, format!("missing '{key}' line"))
            })?;
            last = n;
            let f: Vec<String> =
                l.split_whitespace().map(str::to_string).collect();
            if f.len() != arity + 1 || f[0] != key {
                let m = format!("expected '{key}' with {arity} field(s)");
                return Err(ParseError::new(n, 1, m));
            }
            Ok((n, f))
        };
        let num = |n: usize, s: &str| {
            s.parse::<u32>().map_err(|_| {
                ParseError::new(n, 1, format!("bad number '{s}'"))
            })
        };
        let (n, f) = next("stalmarck-triples", 1)?;
        if f[1] != "1" {
            return Err(ParseError::new(n, 1, "unsupported version".into()));
        }
        let (n, f) = next("nodes", 1)?;
        let nodes = num(n, &f[1])?;
        if nodes == 0 || nodes >= 1 << 31 {
            return Err(ParseError::new(n, 1, "bad node count".into()));
        }
        let mut defined = vec![false; nodes as usize];
        defined[0] = true;
        let mut define = |n: usize, v: u32| {
            if v == 0 || v >= nodes || defined[v as usize] {
                let m = format!("node {v} out of range or defined twice");
                return Err(ParseError::new(n, 1, m));
            }
            defined[v as usize] = true;
            Ok(())
        };
        let (n, f) = next("vars", 1)?;
        let mut variables = Vec::new();
        let mut names = std::collections::HashSet::new();
        for _ in 0..num(n, &f[1])? {
            let (n, f) = next("v", 2)?;
            let v = num(n, &f[1])?;
            define(n, v)?;
            if variables.last().is_some_and(|&(_, u)| u >= v) {
                let m = "variables must be in ascending node order";
                return Err(ParseError::new(n, 1, m.into()));
            }
            if !names.insert(f[2].clone()) {
                let m = format!("duplicate variable name '{}'", f[2]);
                return Err(ParseError::new(n, 1, m));
            }
            variables.push((f[2].clone(), v));
        }
        let (n, f) = next("triples", 1)?;
        let mut triples: Vec<Triple> = Vec::new();
        for _ in 0..num(n, &f[1])? {
            let (n, f) = next("t", 4)?;
            let kind = match f[1].as_str() {
                "and" => TripleKind::And,
                "eq" => TripleKind::Eq,
                k => {
                    let m = format!("unknown triple kind '{k}'");
                    return Err(ParseError::new(n, 1, m));
                }
            };
            let (out, a, b) = (num(n, &f[2])?, num(n, &f[3])?, num(n, &f[4])?);
            let (a, b) = (Lit(a), Lit(b));
            define(n, out)?;
            let bad = if triples.last().is_some_and(|t| t.out >= out) {
                Some("triples must be in ascending output order")
            } else if a.node() >= out || b.node() >= out {
                Some("operand does not precede the output")
            } else if a > b {
                Some("operands must satisfy a <= b")
            } else if kind == TripleKind::Eq
                && (a.is_negated() || b.is_negated())
            {
                Some("eq operands must be positive")
            } else {
                None
            };
            if let Some(m) = bad {
                return Err(ParseError::new(n, 1, m.into()));
            }
            triples.push(Triple { kind, out, a, b });
        }
        let (n, f) = next("root", 1)?;
        let root = Lit(num(n, &f[1])?);
        if root.node() >= nodes {
            return Err(ParseError::new(n, 1, "root out of range".into()));
        }
        if let Some(v) = defined.iter().position(|d| !d) {
            let m = format!("node {v} is never defined");
            return Err(ParseError::new(n, 1, m));
        }
        if let Some((n, _)) = lines.next() {
            return Err(ParseError::new(n, 1, "trailing content".into()));
        }
        Ok(Triples { node_count: nodes, variables, triples, root })
    }
}

struct Builder {
    node_count: u32,
    variables: Vec<(String, u32)>,
    var_index: HashMap<String, u32>,
    triples: Vec<Triple>,
    shared: HashMap<(TripleKind, Lit, Lit), Lit>,
    sharing: bool,
}

impl Builder {
    fn new(sharing: bool) -> Builder {
        Builder {
            node_count: 1,
            variables: Vec::new(),
            var_index: HashMap::new(),
            triples: Vec::new(),
            shared: HashMap::new(),
            sharing,
        }
    }

    fn finish(self, root: Lit) -> Triples {
        Triples {
            node_count: self.node_count,
            variables: self.variables,
            triples: self.triples,
            root,
        }
    }

    fn var(&mut self, name: &str) -> Lit {
        if let Some(&n) = self.var_index.get(name) {
            return Lit::new(n, false);
        }
        let n = self.fresh();
        self.var_index.insert(name.to_string(), n);
        self.variables.push((name.to_string(), n));
        Lit::new(n, false)
    }

    fn fresh(&mut self) -> u32 {
        let n = self.node_count;
        self.node_count =
            n.checked_add(1).filter(|&c| c < 1 << 31).expect("too many nodes");
        n
    }

    /// SPEC-D4, as a post-order fold (no recursion).
    fn lit(&mut self, f: &Formula) -> Lit {
        f.fold(|node, args: &[Lit]| match node {
            Formula::Const(true) => Lit::TRUE,
            Formula::Const(false) => Lit::FALSE,
            Formula::Var(name) => self.var(name),
            Formula::Not(_) => !args[0],
            Formula::And(..) => self.mk(TripleKind::And, args[0], args[1]),
            Formula::Or(..) => !self.mk(TripleKind::And, !args[0], !args[1]),
            Formula::Implies(..) => {
                !self.mk(TripleKind::And, args[0], !args[1])
            }
            Formula::Iff(..) => self.mk(TripleKind::Eq, args[0], args[1]),
            Formula::Xor(..) => !self.mk(TripleKind::Eq, args[0], args[1]),
        })
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
