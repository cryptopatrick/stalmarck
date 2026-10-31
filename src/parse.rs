//! The `.prop` formula syntax (`docs/triple-format.md` §1).

use std::fmt;
use std::str::FromStr;

use crate::formula::Formula;

/// A syntax error with a 1-based position.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseError {
    /// Line of the offending token.
    pub line: usize,
    /// Column of the offending token.
    pub column: usize,
    /// What went wrong.
    pub message: String,
}

impl ParseError {
    pub(crate) fn new(line: usize, column: usize, message: String) -> Self {
        ParseError { line, column, message }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Not,
    And,
    Xor,
    Or,
    Imp,
    Iff,
    Const(bool),
    Ident(String),
    Open,
    Close,
    End,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Not => write!(f, "'!'"),
            Tok::And => write!(f, "'&'"),
            Tok::Xor => write!(f, "'^'"),
            Tok::Or => write!(f, "'|'"),
            Tok::Imp => write!(f, "'->'"),
            Tok::Iff => write!(f, "'<->'"),
            Tok::Const(true) => write!(f, "'T'"),
            Tok::Const(false) => write!(f, "'F'"),
            Tok::Ident(s) => write!(f, "identifier '{s}'"),
            Tok::Open => write!(f, "'('"),
            Tok::Close => write!(f, "')'"),
            Tok::End => write!(f, "end of input"),
        }
    }
}

fn keyword(word: &str) -> Option<Tok> {
    if word == "T" {
        return Some(Tok::Const(true));
    }
    if word == "F" {
        return Some(Tok::Const(false));
    }
    Some(match word.to_ascii_lowercase().as_str() {
        "not" => Tok::Not,
        "and" => Tok::And,
        "xor" => Tok::Xor,
        "or" => Tok::Or,
        "implies" => Tok::Imp,
        "iff" => Tok::Iff,
        "true" => Tok::Const(true),
        "false" => Tok::Const(false),
        _ => return None,
    })
}

fn lex(src: &str) -> Result<Vec<(Tok, usize, usize)>, ParseError> {
    let mut toks = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("");
        let chars: Vec<char> = line.chars().collect();
        let mut j = 0;
        while j < chars.len() {
            let (c, col) = (chars[j], j + 1);
            let rest: String =
                chars[j..chars.len().min(j + 3)].iter().collect();
            let (tok, len) = if c.is_whitespace() {
                j += 1;
                continue;
            } else if rest.starts_with("<->") {
                (Tok::Iff, 3)
            } else if rest.starts_with("->") {
                (Tok::Imp, 2)
            } else if c.is_ascii_alphabetic() || c == '_' {
                let mut k = j;
                while k < chars.len()
                    && (chars[k].is_ascii_alphanumeric()
                        || chars[k] == '_'
                        || chars[k] == '.')
                {
                    k += 1;
                }
                let word: String = chars[j..k].iter().collect();
                (keyword(&word).unwrap_or(Tok::Ident(word)), k - j)
            } else {
                let tok = match c {
                    '!' | '~' => Tok::Not,
                    '&' => Tok::And,
                    '^' => Tok::Xor,
                    '|' => Tok::Or,
                    '(' => Tok::Open,
                    ')' => Tok::Close,
                    _ => {
                        let msg = format!("unexpected character {c:?}");
                        return Err(ParseError::new(i + 1, col, msg));
                    }
                };
                (tok, 1)
            };
            toks.push((tok, i + 1, col));
            j += len;
        }
    }
    let line = src.lines().count().max(1);
    toks.push((Tok::End, line, src.lines().last().map_or(1, |l| l.len() + 1)));
    Ok(toks)
}

struct Parser {
    toks: Vec<(Tok, usize, usize)>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].0
    }

    fn error(&self, expected: &str) -> ParseError {
        let (tok, line, col) = &self.toks[self.pos];
        ParseError::new(
            *line,
            *col,
            format!("expected {expected}, found {tok}"),
        )
    }

    /// Operator-precedence parsing with explicit stacks, so that deeply
    /// nested input cannot overflow the call stack.
    fn formula(&mut self) -> Result<Formula, ParseError> {
        let mut ops: Vec<Tok> = Vec::new();
        let mut operands: Vec<Formula> = Vec::new();
        loop {
            // Expecting an operand: prefix operators and '(' first.
            loop {
                let tok = self.peek().clone();
                match tok {
                    Tok::Not | Tok::Open => {
                        self.pos += 1;
                        ops.push(tok);
                    }
                    Tok::Const(c) => {
                        self.pos += 1;
                        operands.push(Formula::constant(c));
                        break;
                    }
                    Tok::Ident(name) => {
                        self.pos += 1;
                        operands.push(Formula::Var(name));
                        break;
                    }
                    _ => return Err(self.error("a formula")),
                }
            }
            // Expecting an operator, ')' or the end.
            loop {
                let tok = self.peek().clone();
                match tok {
                    Tok::Close => {
                        loop {
                            match ops.pop() {
                                Some(Tok::Open) => break,
                                Some(op) => reduce(op, &mut operands),
                                None => return Err(self.error("end of input")),
                            }
                        }
                        self.pos += 1;
                    }
                    Tok::And | Tok::Xor | Tok::Or | Tok::Imp | Tok::Iff => {
                        let p = precedence(&tok);
                        while let Some(top) = ops.last() {
                            let q = precedence(top);
                            if *top == Tok::Open
                                || q < p
                                || (q == p && tok == Tok::Imp)
                            {
                                break;
                            }
                            let op = ops.pop().unwrap();
                            reduce(op, &mut operands);
                        }
                        self.pos += 1;
                        ops.push(tok);
                        break;
                    }
                    Tok::End => {
                        while let Some(op) = ops.pop() {
                            if op == Tok::Open {
                                return Err(self.error("')'"));
                            }
                            reduce(op, &mut operands);
                        }
                        return Ok(operands.pop().expect("one operand"));
                    }
                    _ if ops.contains(&Tok::Open) => {
                        return Err(self.error("')'"));
                    }
                    _ => return Err(self.error("end of input")),
                }
            }
        }
    }
}

/// Binding strength; `!` binds tightest.
fn precedence(tok: &Tok) -> u8 {
    match tok {
        Tok::Not => 6,
        Tok::And => 5,
        Tok::Xor => 4,
        Tok::Or => 3,
        Tok::Imp => 2,
        Tok::Iff => 1,
        _ => 0,
    }
}

fn reduce(op: Tok, operands: &mut Vec<Formula>) {
    let r = operands.pop().expect("operand");
    let f = if op == Tok::Not {
        !r
    } else {
        let l = operands.pop().expect("operand");
        match op {
            Tok::And => l.and(r),
            Tok::Xor => l.xor(r),
            Tok::Or => l.or(r),
            Tok::Imp => l.implies(r),
            _ => l.iff(r),
        }
    };
    operands.push(f);
}

impl FromStr for Formula {
    type Err = ParseError;

    /// Parses the `.prop` syntax.
    ///
    /// ```
    /// use stalmarck::Formula;
    ///
    /// let f: Formula = "((a AND b) -> c) AND (a AND b)".parse().unwrap();
    /// assert_eq!(f.to_string(), "(((a & b) -> c) & (a & b))");
    /// ```
    fn from_str(src: &str) -> Result<Formula, ParseError> {
        Parser { toks: lex(src)?, pos: 0 }.formula()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> String {
        s.parse::<Formula>().unwrap().to_string()
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(p("a | b & c"), "(a | (b & c))");
        assert_eq!(p("a ^ b | c ^ d"), "((a ^ b) | (c ^ d))");
        assert_eq!(p("a & b ^ c"), "((a & b) ^ c)");
        assert_eq!(p("a -> b -> c"), "(a -> (b -> c))");
        assert_eq!(p("a <-> b <-> c"), "((a <-> b) <-> c)");
        assert_eq!(p("a | b -> c <-> d"), "(((a | b) -> c) <-> d)");
        assert_eq!(p("!a & ~b"), "(!a & !b)");
        assert_eq!(p("not not T or false"), "(!!T | F)");
    }

    #[test]
    fn keywords_comments_and_names() {
        assert_eq!(
            p("x.1 IMPLIES _y # comment\n iff z2"),
            "((x.1 -> _y) <-> z2)"
        );
        assert_eq!(p("a xor B"), "(a ^ B)");
    }

    #[test]
    fn errors_have_positions() {
        let e = "a &\n(b | )".parse::<Formula>().unwrap_err();
        assert_eq!((e.line, e.column), (2, 6));
        let e = "a b".parse::<Formula>().unwrap_err();
        assert_eq!(e.message, "expected end of input, found identifier 'b'");
        assert!("a $ b".parse::<Formula>().is_err());
        assert!("".parse::<Formula>().is_err());
    }
}
