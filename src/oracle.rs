//! Truth-table oracles, independent of the proof procedure (PRD §36).

use std::collections::BTreeMap;

use crate::cnf::Cnf;
use crate::formula::Formula;
use crate::solver::Model;

/// Formulas with more variables than this are refused by the oracles.
pub const MAX_VARIABLES: usize = 22;

/// The oracle refused an input with too many variables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooManyVariables(pub usize);

/// Enumerates all assignments of `formula`'s variables, and returns the
/// first satisfying one in binary counting order, or `None` if the formula
/// is unsatisfiable.
///
/// ```
/// use stalmarck::{oracle, Formula};
///
/// let f: Formula = "a & !a".parse().unwrap();
/// assert_eq!(oracle::satisfying_assignment(&f), Ok(None));
/// ```
///
/// # Errors
///
/// Returns [`TooManyVariables`] if the formula has more than
/// [`MAX_VARIABLES`] variables.
#[allow(clippy::missing_panics_doc)] // Only variables of `formula` are looked up.
pub fn satisfying_assignment(
    formula: &Formula,
) -> Result<Option<Model>, TooManyVariables> {
    let vars = formula.variables();
    if vars.len() > MAX_VARIABLES {
        return Err(TooManyVariables(vars.len()));
    }
    for bits in 0u32..1 << vars.len() {
        let value = |name: &str| {
            let i = vars.iter().position(|v| *v == name).unwrap();
            bits >> i & 1 == 1
        };
        if formula.evaluate(&value) {
            let map: BTreeMap<String, bool> =
                vars.iter().map(|v| (v.to_string(), value(v))).collect();
            return Ok(Some(Model::from_map(map)));
        }
    }
    Ok(None)
}

/// Like [`satisfying_assignment`] for a CNF. The model names variable `i`
/// `x<i>`.
///
/// # Errors
///
/// Returns [`TooManyVariables`] if the CNF has more than [`MAX_VARIABLES`]
/// variables.
pub fn satisfying_assignment_cnf(
    cnf: &Cnf,
) -> Result<Option<Model>, TooManyVariables> {
    let n = cnf.variables as usize;
    if n > MAX_VARIABLES {
        return Err(TooManyVariables(n));
    }
    let mut assignment = vec![false; n];
    for bits in 0u32..1 << n {
        for (i, a) in assignment.iter_mut().enumerate() {
            *a = bits >> i & 1 == 1;
        }
        if cnf.evaluate(&assignment) {
            let map = (0..n)
                .map(|i| (format!("x{}", i + 1), assignment[i]))
                .collect();
            return Ok(Some(Model::from_map(map)));
        }
    }
    Ok(None)
}
