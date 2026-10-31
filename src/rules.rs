//! The simple rules of one triple, defined semantically (SPEC-S1).

use crate::relation::Relation;
use crate::triples::{Lit, Triple};

/// Computes the local closure of `t` under `rel`. New equivalences are
/// written to `out` in ascending node-pair order (SPEC-Z4). Returns `false`
/// if no assignment is consistent with `t` and `rel` (a contradiction).
#[allow(clippy::needless_range_loop)] // i, j index several 4x4 tables
pub(crate) fn local_closure(
    rel: &Relation,
    t: &Triple,
    out: &mut Vec<(Lit, Lit)>,
) -> bool {
    out.clear();
    // V(t) = {0, x, node(a), node(b)}, sorted and deduplicated.
    let mut nodes = [0, t.out, t.a.node(), t.b.node()];
    nodes.sort_unstable();
    let mut k = 1;
    for i in 1..4 {
        if nodes[i] != nodes[k - 1] {
            nodes[k] = nodes[i];
            k += 1;
        }
    }
    let nodes = &nodes[..k];
    let idx = |n: u32| nodes.iter().position(|&m| m == n).unwrap();

    let mut known = [[None; 4]; 4];
    for i in 0..k {
        for j in i + 1..k {
            known[i][j] = rel.related(nodes[i], nodes[j]);
        }
    }

    // seen[i][j] bit p: parity p between nodes i and j occurs in a model.
    let mut seen = [[0u8; 4]; 4];
    let mut any = false;
    for mask in 0u32..1 << (k - 1) {
        let val = |i: usize| i == 0 || (mask >> (i - 1)) & 1 == 1;
        if !t.holds(|n| val(idx(n))) {
            continue;
        }
        let consistent = (0..k).all(|i| {
            (i + 1..k).all(|j| match known[i][j] {
                Some(p) => u8::from(val(i) != val(j)) == p,
                None => true,
            })
        });
        if !consistent {
            continue;
        }
        any = true;
        for i in 0..k {
            for j in i + 1..k {
                seen[i][j] |= 1 << u8::from(val(i) != val(j));
            }
        }
    }
    if !any {
        return false;
    }
    for i in 0..k {
        for j in i + 1..k {
            if known[i][j].is_none() && seen[i][j] != 3 {
                let negated = seen[i][j] == 2;
                out.push((
                    Lit::new(nodes[i], false),
                    Lit::new(nodes[j], negated),
                ));
            }
        }
    }
    true
}
