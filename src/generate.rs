//! Deterministic instance generators for tests and benchmarks (PRD §36,
//! §40). The same seed and parameters always give the same instance, for a
//! given [`GENERATOR_VERSION`].

use crate::cnf::Cnf;
use crate::formula::Formula;

/// Bumped whenever any generator's output changes for the same inputs.
pub const GENERATOR_VERSION: u32 = 1;

/// `SplitMix64`.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn x(i: u32) -> Formula {
    Formula::var(format!("x{i}"))
}

/// A random formula over `x1..=x<variables>` with nesting at most `depth`.
/// Each binary node is one of `∧ ∨ → ↔ ⊕` with equal probability, and 1 in
/// 7 inner nodes is a negation. About 1 in 25 leaves is a constant.
///
/// # Panics
///
/// Panics if `variables` is 0.
#[must_use]
pub fn random_formula(seed: u64, variables: u32, depth: u32) -> Formula {
    fn go(rng: &mut Rng, vars: u32, depth: u32) -> Formula {
        if depth == 0 || rng.below(5) == 0 {
            return match rng.below(25) {
                0 => Formula::constant(rng.below(2) == 0),
                _ => x(1 + rng.below(u64::from(vars)) as u32),
            };
        }
        if rng.below(7) == 0 {
            return !go(rng, vars, depth - 1);
        }
        let op = rng.below(5);
        let (l, r) = (go(rng, vars, depth - 1), go(rng, vars, depth - 1));
        match op {
            0 => l & r,
            1 => l | r,
            2 => l.implies(r),
            3 => l.iff(r),
            _ => l ^ r,
        }
    }
    assert!(variables > 0, "need at least one variable");
    go(&mut Rng(seed), variables, depth)
}

/// A uniform random k-CNF: each clause has `k` distinct variables with
/// random signs.
///
/// # Panics
///
/// Panics unless `1 <= k <= variables`.
#[must_use]
pub fn random_kcnf(seed: u64, variables: u32, clauses: u32, k: u32) -> Cnf {
    assert!(k >= 1 && k <= variables, "need 1 <= k <= variables");
    let mut rng = Rng(seed);
    let clauses = (0..clauses)
        .map(|_| {
            let mut c: Vec<i32> = Vec::new();
            while c.len() < k as usize {
                let v = 1 + rng.below(u64::from(variables)) as i32;
                if !c.iter().any(|l| l.abs() == v) {
                    c.push(if rng.below(2) == 0 { v } else { -v });
                }
            }
            c
        })
        .collect();
    Cnf { variables, clauses }
}

/// The pigeonhole principle: `holes + 1` pigeons in `holes` holes.
/// Unsatisfiable for every `holes >= 1`. Variable `p·holes + h + 1` means
/// pigeon `p` sits in hole `h`.
#[must_use]
pub fn pigeonhole(holes: u32) -> Cnf {
    let var = |p: u32, h: u32| (p * holes + h + 1) as i32;
    let mut clauses: Vec<Vec<i32>> =
        (0..=holes).map(|p| (0..holes).map(|h| var(p, h)).collect()).collect();
    for h in 0..holes {
        for p in 0..=holes {
            for q in p + 1..=holes {
                clauses.push(vec![-var(p, h), -var(q, h)]);
            }
        }
    }
    Cnf { variables: (holes + 1) * holes, clauses }
}

/// `x1 ⊕ … ⊕ xn` (left fold) asserted equal to `parity`. Satisfiable.
///
/// # Panics
///
/// Panics if `n` is 0.
#[must_use]
pub fn parity(n: u32, parity: bool) -> Formula {
    assert!(n > 0, "need at least one variable");
    let chain = (2..=n).fold(x(1), |acc, i| acc ^ x(i));
    chain.iff(Formula::constant(parity))
}

/// The XOR chain `x1 ⊕ … ⊕ xn` (left fold) is asserted equal to the same
/// chain in reverse order, but negated. Unsatisfiable for every `n`.
/// Without sharing, refuting it needs reasoning across the two chains.
///
/// # Panics
///
/// Panics if `n` is 0.
#[must_use]
pub fn parity_mismatch(n: u32) -> Formula {
    assert!(n > 0, "need at least one variable");
    let forward = (2..=n).fold(x(1), |acc, i| acc ^ x(i));
    let backward = (1..n).rev().fold(x(n), |acc, i| acc ^ x(i));
    forward.iff(!backward)
}

/// `x1 ∧ (x1 → x2) ∧ … ∧ (x_{n−1} → xn) ∧ ¬xn`. Unsatisfiable, refuted by
/// 0-saturation alone, through a long chain of propagation.
///
/// # Panics
///
/// Panics if `n` is 0.
#[must_use]
pub fn implication_chain(n: u32) -> Formula {
    assert!(n > 0, "need at least one variable");
    let chain = (1..n).fold(x(1), |acc, i| acc & x(i).implies(x(i + 1)));
    chain & !x(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle;

    #[test]
    fn deterministic() {
        assert_eq!(random_formula(7, 4, 5), random_formula(7, 4, 5));
        assert_eq!(random_kcnf(7, 10, 40, 3), random_kcnf(7, 10, 40, 3));
        assert_ne!(random_kcnf(7, 10, 40, 3), random_kcnf(8, 10, 40, 3));
    }

    #[test]
    fn structured_families_have_known_answers() {
        for n in 1..=3 {
            let php = oracle::satisfying_assignment_cnf(&pigeonhole(n));
            assert_eq!(php, Ok(None));
        }
        for n in 1..=6 {
            let sat = |f: &Formula| {
                oracle::satisfying_assignment(f).unwrap().is_some()
            };
            assert!(sat(&parity(n, true)) && sat(&parity(n, false)));
            assert!(!sat(&parity_mismatch(n)));
            assert!(!sat(&implication_chain(n)));
        }
    }
}
