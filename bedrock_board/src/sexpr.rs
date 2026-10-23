//! A small S-expression reader for KiCad files: atoms (bare or quoted) and lists, nothing else.

use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq)]
pub enum Sx {
    Atom(String),
    List(Vec<Sx>),
}

impl Sx {
    /// The list's head atom, `""` for an empty list or an atom.
    pub fn head(&self) -> &str {
        match self {
            Sx::List(items) => items.first().and_then(Sx::atom).unwrap_or(""),
            Sx::Atom(_) => "",
        }
    }

    pub fn atom(&self) -> Option<&str> {
        match self {
            Sx::Atom(s) => Some(s),
            Sx::List(_) => None,
        }
    }

    pub fn items(&self) -> &[Sx] {
        match self {
            Sx::List(items) => items,
            Sx::Atom(_) => &[],
        }
    }

    /// The list's n-th item (0 is the head) as an atom.
    pub fn arg(&self, n: usize) -> Option<&str> {
        self.items().get(n).and_then(Sx::atom)
    }

    pub fn num(&self, n: usize) -> Option<f64> {
        self.arg(n).and_then(|s| s.parse().ok())
    }

    /// Every child list whose head is `name`.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sx> + 'a {
        self.items()
            .iter()
            .skip(1)
            .filter(move |c| c.head() == name)
    }

    /// The first child list whose head is `name`.
    pub fn child(&self, name: &str) -> Option<&Sx> {
        self.items().iter().skip(1).find(|c| c.head() == name)
    }
}

pub fn parse(text: &str) -> Result<Sx> {
    let mut chars = text.char_indices().peekable();
    let mut stack: Vec<Vec<Sx>> = vec![];
    let mut root: Option<Sx> = None;
    while let Some((i, c)) = chars.next() {
        match c {
            '(' => stack.push(vec![]),
            ')' => {
                let items = match stack.pop() {
                    Some(items) => items,
                    None => bail!("unbalanced ')' at byte {i}"),
                };
                let list = Sx::List(items);
                match stack.last_mut() {
                    Some(parent) => parent.push(list),
                    None => root = Some(list),
                }
            }
            '"' => {
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some((_, '\\')) => {
                            if let Some((_, e)) = chars.next() {
                                s.push(match e {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        Some((_, '"')) => break,
                        Some((_, ch)) => s.push(ch),
                        None => bail!("unterminated string"),
                    }
                }
                match stack.last_mut() {
                    Some(parent) => parent.push(Sx::Atom(s)),
                    None => bail!("string outside a list at byte {i}"),
                }
            }
            c if c.is_whitespace() => {}
            _ => {
                let mut s = String::new();
                s.push(c);
                while let Some(&(_, n)) = chars.peek() {
                    if n.is_whitespace() || n == '(' || n == ')' {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                match stack.last_mut() {
                    Some(parent) => parent.push(Sx::Atom(s)),
                    None => bail!("atom outside a list at byte {i}"),
                }
            }
        }
    }
    if !stack.is_empty() {
        bail!("unbalanced '(': {} lists still open", stack.len());
    }
    root.ok_or_else(|| anyhow::anyhow!("empty file"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_lists_and_strings() {
        let sx = parse(r#"(pad "1" thru_hole rect (at -1.27 0 90) (net "GND") (size 1.85 1.85))"#)
            .unwrap();
        assert_eq!(sx.head(), "pad");
        assert_eq!(sx.arg(1), Some("1"));
        assert_eq!(sx.child("at").unwrap().num(1), Some(-1.27));
        assert_eq!(sx.child("net").unwrap().arg(1), Some("GND"));
        assert_eq!(sx.children("size").count(), 1);
    }
}
