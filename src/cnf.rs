//! DIMACS CNF input (`docs/triple-format.md` §2).

use std::fmt::Write;

use crate::formula::Formula;
use crate::parse::ParseError;

/// A formula in conjunctive normal form, as in DIMACS: variables are
/// `1..=variables`, and a literal is `±i`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cnf {
    /// Number of variables.
    pub variables: u32,
    /// The clauses, each a list of non-zero literals.
    pub clauses: Vec<Vec<i32>>,
}

impl Cnf {
    /// Parses DIMACS CNF text.
    ///
    /// ```
    /// use stalmarck::Cnf;
    ///
    /// let cnf = Cnf::parse_dimacs("p cnf 2 2\n1 -2 0\n2 0\n").unwrap();
    /// assert_eq!(cnf.clauses, vec![vec![1, -2], vec![2]]);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a [`ParseError`] if `src` is not valid DIMACS CNF.
    pub fn parse_dimacs(src: &str) -> Result<Cnf, ParseError> {
        let mut header: Option<(u32, usize)> = None;
        let mut clauses = Vec::new();
        let mut clause = Vec::new();
        let mut last_line = 0;
        for (i, line) in src.lines().enumerate() {
            let (n, t) = (i + 1, line.trim());
            last_line = n;
            if t.starts_with('%') {
                break;
            }
            if t.is_empty() || t.starts_with('c') {
                continue;
            }
            let err = |m: &str| ParseError::new(n, 1, m.to_string());
            if t.starts_with('p') {
                let f: Vec<&str> = t.split_whitespace().collect();
                if header.is_some() {
                    return Err(err("duplicate 'p' line"));
                }
                match f.as_slice() {
                    ["p", "cnf", v, c] => match (v.parse(), c.parse()) {
                        (Ok(v), Ok(c)) if v < 1 << 30 => header = Some((v, c)),
                        _ => return Err(err("malformed 'p cnf' line")),
                    },
                    _ => return Err(err("expected 'p cnf <vars> <clauses>'")),
                }
                continue;
            }
            let Some((vars, _)) = header else {
                return Err(err("clause before 'p cnf' line"));
            };
            for word in t.split_whitespace() {
                let lit: i32 = word
                    .parse()
                    .map_err(|_| err(&format!("bad literal '{word}'")))?;
                if lit == 0 {
                    clauses.push(std::mem::take(&mut clause));
                } else if lit.unsigned_abs() > vars {
                    return Err(err(&format!("literal {lit} exceeds {vars}")));
                } else {
                    clause.push(lit);
                }
            }
        }
        let (variables, expected) = header.ok_or_else(|| {
            ParseError::new(last_line.max(1), 1, "missing 'p cnf' line".into())
        })?;
        if !clause.is_empty() {
            clauses.push(clause);
        }
        if clauses.len() != expected {
            let msg = format!(
                "header declares {expected} clauses, found {}",
                clauses.len()
            );
            return Err(ParseError::new(last_line.max(1), 1, msg));
        }
        Ok(Cnf { variables, clauses })
    }

    /// DIMACS text, one clause per line.
    #[must_use]
    pub fn to_dimacs(&self) -> String {
        let mut s =
            format!("p cnf {} {}\n", self.variables, self.clauses.len());
        for c in &self.clauses {
            for l in c {
                let _ = write!(s, "{l} ");
            }
            s.push_str("0\n");
        }
        s
    }

    /// Whether `assignment` satisfies every clause. `assignment[i - 1]` is
    /// the value of variable `i`.
    #[must_use]
    pub fn evaluate(&self, assignment: &[bool]) -> bool {
        self.clauses.iter().all(|c| {
            c.iter()
                .any(|&l| assignment[l.unsigned_abs() as usize - 1] == (l > 0))
        })
    }

    /// The formula of the CNF translation: left folds of `∨` and `∧`, with
    /// variable `i` named `x<i>`.
    pub fn to_formula(&self) -> Formula {
        let lit = |l: i32| {
            let v = Formula::var(format!("x{}", l.unsigned_abs()));
            if l > 0 { v } else { !v }
        };
        let clause = |c: &Vec<i32>| {
            c.iter()
                .map(|&l| lit(l))
                .reduce(Formula::or)
                .unwrap_or(Formula::constant(false))
        };
        self.clauses
            .iter()
            .map(clause)
            .reduce(Formula::and)
            .unwrap_or(Formula::constant(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_comments() {
        let src = "c hi\np cnf 3 2\n1 -3\n 2 0 -1\n0\n%\n0\n";
        let cnf = Cnf::parse_dimacs(src).unwrap();
        assert_eq!(cnf.clauses, vec![vec![1, -3, 2], vec![-1]]);
        assert_eq!(Cnf::parse_dimacs(&cnf.to_dimacs()).unwrap(), cnf);
        assert_eq!(cnf.to_formula().to_string(), "(((x1 | !x3) | x2) & !x1)");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Cnf::parse_dimacs("1 2 0\n").is_err());
        assert!(Cnf::parse_dimacs("p cnf 2 1\n3 0\n").is_err());
        assert!(Cnf::parse_dimacs("p cnf 2 2\n1 0\n").is_err());
        assert!(Cnf::parse_dimacs("p dnf 2 1\n1 0\n").is_err());
    }
}
