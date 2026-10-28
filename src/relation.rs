//! Formula relations as union-find with parity (formal specification §4,
//! ADR-0003).

use std::collections::HashMap;

use crate::triples::Lit;

/// The canonical form of a formula relation (SPEC-R4).
///
/// Two relations are equal exactly when their canonical forms are equal,
/// whatever their internal representation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CanonicalRelation {
    /// The contradictory relation ⊥⊥.
    Contradiction,
    /// For each node `v`, the smallest node `m` of its class and whether
    /// `v ≡ ¬m` (`true`) or `v ≡ m` (`false`).
    Classes(Vec<(u32, bool)>),
}

impl std::fmt::Display for CanonicalRelation {
    /// The one-line form of `triple-format.md` §4.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CanonicalRelation::Contradiction => write!(f, "contradiction"),
            CanonicalRelation::Classes(classes) => {
                write!(f, "classes")?;
                for &(m, neg) in classes {
                    write!(f, " {}", 2 * m + neg as u32)?;
                }
                Ok(())
            }
        }
    }
}

/// Outcome of asserting one equivalence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Union {
    /// Already implied.
    Redundant,
    /// Two classes were merged.
    Merged,
    /// The relation became contradictory.
    Contradiction,
}

/// Union by size without path compression, plus a circular member list per
/// class (needed for the affected-triple scan, SPEC-Z3), and an undo trail
/// for branch rollback (ADR-0004).
#[derive(Clone, Debug)]
pub(crate) struct Relation {
    parent: Vec<u32>,
    parity: Vec<u8>,
    size: Vec<u32>,
    next: Vec<u32>,
    min: Vec<u32>,
    contradiction: bool,
    /// One entry per merge: the linked child root and the previous `min`
    /// of the root it was linked under.
    trail: Vec<(u32, u32)>,
}

/// A point to roll back to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Checkpoint {
    trail_len: usize,
    contradiction: bool,
}

impl Relation {
    /// The identity relation on `n` nodes.
    pub fn new(n: u32) -> Relation {
        let ids: Vec<u32> = (0..n).collect();
        Relation {
            parent: ids.clone(),
            parity: vec![0; n as usize],
            size: vec![1; n as usize],
            next: ids.clone(),
            min: ids,
            contradiction: false,
            trail: Vec::new(),
        }
    }

    pub fn node_count(&self) -> u32 {
        self.parent.len() as u32
    }

    pub fn is_contradiction(&self) -> bool {
        self.contradiction
    }

    pub fn set_contradiction(&mut self) {
        self.contradiction = true;
    }

    /// Root of `v`'s class and `v`'s parity relative to it.
    pub fn find(&self, mut v: u32) -> (u32, u8) {
        let mut p = 0;
        while self.parent[v as usize] != v {
            p ^= self.parity[v as usize];
            v = self.parent[v as usize];
        }
        (v, p)
    }

    /// Parity between nodes `a` and `b`, if they are related.
    pub fn related(&self, a: u32, b: u32) -> Option<u8> {
        let ((ra, pa), (rb, pb)) = (self.find(a), self.find(b));
        (ra == rb).then_some(pa ^ pb)
    }

    /// The value of `v`, if it is determined (related to ⊤).
    pub fn value(&self, v: u32) -> Option<bool> {
        self.related(v, 0).map(|p| p == 0)
    }

    /// The smallest node of `v`'s class.
    pub fn class_min(&self, v: u32) -> u32 {
        self.min[self.find(v).0 as usize]
    }

    /// Asserts `a ≡ b`. On a merge, the nodes of the scan class (SPEC-Z3)
    /// are written to `scan`.
    pub fn union(&mut self, a: Lit, b: Lit, scan: &mut Vec<u32>) -> Union {
        if self.contradiction {
            return Union::Contradiction;
        }
        let (ra, pa) = self.find(a.node());
        let (rb, pb) = self.find(b.node());
        let p = (a.is_negated() != b.is_negated()) as u8;
        if ra == rb {
            if pa ^ pb != p {
                self.contradiction = true;
                return Union::Contradiction;
            }
            return Union::Redundant;
        }
        let rt = self.find(0).0;
        let key = |r: u32| {
            (self.size[r as usize], std::cmp::Reverse(self.min[r as usize]))
        };
        let scan_root = if ra == rt {
            rb
        } else if rb == rt || key(ra) < key(rb) {
            ra
        } else {
            rb
        };
        scan.clear();
        let mut v = scan_root;
        loop {
            scan.push(v);
            v = self.next[v as usize];
            if v == scan_root {
                break;
            }
        }
        let (child, root) =
            match self.size[ra as usize].cmp(&self.size[rb as usize]) {
                std::cmp::Ordering::Less => (ra, rb),
                std::cmp::Ordering::Greater => (rb, ra),
                std::cmp::Ordering::Equal if scan_root == ra => (ra, rb),
                std::cmp::Ordering::Equal => (rb, ra),
            };
        let (c, r) = (child as usize, root as usize);
        self.trail.push((child, self.min[r]));
        self.parent[c] = root;
        self.parity[c] = pa ^ pb ^ p;
        self.size[r] += self.size[c];
        self.min[r] = self.min[r].min(self.min[c]);
        self.next.swap(c, r);
        Union::Merged
    }

    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            trail_len: self.trail.len(),
            contradiction: self.contradiction,
        }
    }

    /// Undoes every merge made since `cp`, newest first.
    pub fn rollback(&mut self, cp: Checkpoint) {
        while self.trail.len() > cp.trail_len {
            let (child, old_min) = self.trail.pop().unwrap();
            let c = child as usize;
            let r = self.parent[c] as usize;
            self.next.swap(c, r);
            self.size[r] -= self.size[c];
            self.min[r] = old_min;
            self.parent[c] = child;
            self.parity[c] = 0;
        }
        self.contradiction = cp.contradiction;
    }

    /// Whether anything was merged since `cp`.
    pub fn changed_since(&self, cp: Checkpoint) -> bool {
        self.trail.len() > cp.trail_len
            || self.contradiction != cp.contradiction
    }

    /// The links made since `cp`, as `(child, root, parity)`. Asserting
    /// them again on the state at `cp` rebuilds the current relation.
    pub fn links_since(&self, cp: Checkpoint) -> Vec<(u32, u32, u8)> {
        self.trail[cp.trail_len..]
            .iter()
            .map(|&(c, _)| {
                (c, self.parent[c as usize], self.parity[c as usize])
            })
            .collect()
    }

    /// Every node whose class changed since `cp`, ascending.
    pub fn nodes_changed_since(&self, cp: Checkpoint) -> Vec<u32> {
        let mut roots: Vec<u32> = self.trail[cp.trail_len..]
            .iter()
            .map(|&(c, _)| self.find(c).0)
            .collect();
        roots.sort_unstable();
        roots.dedup();
        let mut out = Vec::new();
        for r in roots {
            let mut v = r;
            loop {
                out.push(v);
                v = self.next[v as usize];
                if v == r {
                    break;
                }
            }
        }
        out.sort_unstable();
        out
    }

    pub fn canonical(&self) -> CanonicalRelation {
        if self.contradiction {
            return CanonicalRelation::Contradiction;
        }
        let classes = (0..self.node_count())
            .map(|v| {
                let (r, p) = self.find(v);
                let m = self.min[r as usize];
                (m, p != self.find(m).1)
            })
            .collect();
        CanonicalRelation::Classes(classes)
    }

    /// `r1 ⊓ r2` (SPEC-R5): intersection, with ⊥⊥ as the identity.
    /// Computed by grouping on the key of SPEC-R6.
    pub fn meet(r1: &Relation, r2: &Relation) -> Relation {
        if r1.contradiction {
            return r2.clone();
        }
        if r2.contradiction {
            return r1.clone();
        }
        let n = r1.node_count();
        let mut out = Relation::new(n);
        let mut first: HashMap<(u32, u32, u8), (u32, u8)> = HashMap::new();
        let mut scan = Vec::new();
        for v in 0..n {
            let ((g1, p1), (g2, p2)) = (r1.find(v), r2.find(v));
            match first.get(&(g1, g2, p1 ^ p2)) {
                Some(&(u, pu)) => {
                    let lu = Lit::new(u, false);
                    out.union(lu, Lit::new(v, pu != p1), &mut scan);
                }
                None => {
                    first.insert((g1, g2, p1 ^ p2), (v, p1));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(r: &Relation) -> Vec<(u32, u32, u8)> {
        let n = r.node_count();
        let mut out = Vec::new();
        for a in 0..n {
            for b in a + 1..n {
                if let Some(p) = r.related(a, b) {
                    out.push((a, b, p));
                }
            }
        }
        out
    }

    fn random_relation(n: u32, seed: &mut u64) -> Relation {
        let mut r = Relation::new(n);
        let mut scan = Vec::new();
        for _ in 0..n / 2 {
            *seed ^= *seed << 13;
            *seed ^= *seed >> 7;
            *seed ^= *seed << 17;
            let a = (*seed % n as u64) as u32;
            let b = ((*seed >> 20) % n as u64) as u32;
            let neg = (*seed >> 40) & 1 == 1;
            if r.related(a, b).is_none() {
                r.union(Lit::new(a, false), Lit::new(b, neg), &mut scan);
            }
        }
        r
    }

    #[test]
    fn meet_is_pair_set_intersection() {
        let mut seed = 0x2545_f491_4f6c_dd1d;
        for _ in 0..500 {
            let r1 = random_relation(9, &mut seed);
            let r2 = random_relation(9, &mut seed);
            let expect: Vec<_> = pairs(&r1)
                .into_iter()
                .filter(|q| pairs(&r2).contains(q))
                .collect();
            assert_eq!(pairs(&Relation::meet(&r1, &r2)), expect);
        }
    }

    #[test]
    fn contradiction_on_conflicting_parity() {
        let mut r = Relation::new(3);
        let mut scan = Vec::new();
        let (a, b) = (Lit::new(1, false), Lit::new(2, false));
        assert_eq!(r.union(a, b, &mut scan), Union::Merged);
        assert_eq!(r.union(a, !b, &mut scan), Union::Contradiction);
        assert_eq!(r.canonical(), CanonicalRelation::Contradiction);
    }
}
